use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use wanxiang_core::{
    classify_cursor_global_key, classify_keychain, lexical_within, Capability, Clock,
    CredentialKind, FileMeta, Guard, GuardError, KeychainEntry, PathScope, ScopedRead, ToolId,
    ToolScheduler, CONFIRM_TTL,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let raw = std::env::temp_dir().join(format!("wanxiang-m1a-{}-{}", std::process::id(), n));
        fs::create_dir_all(&raw).unwrap();
        Self {
            path: fs::canonicalize(&raw).unwrap(),
        }
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::canonicalize(&path).unwrap()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Rec {
    events: Mutex<Vec<(ToolId, Capability)>>,
}

impl Rec {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
        })
    }

    fn events(&self) -> Vec<(ToolId, Capability)> {
        self.events.lock().unwrap().clone()
    }
}

impl ToolScheduler for Rec {
    fn on_revoked(&self, tool: ToolId, capability: Capability) {
        self.events.lock().unwrap().push((tool, capability));
    }
}

struct FakeClock {
    now: Mutex<SystemTime>,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        })
    }

    fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap()
    }
}

struct Env {
    guard: Guard,
    rec: Arc<Rec>,
    clock: Arc<FakeClock>,
}

fn env() -> Env {
    let rec = Rec::new();
    let clock = FakeClock::new();
    let guard = Guard::new(rec.clone(), clock.clone());
    Env { guard, rec, clock }
}

fn scope(root: &Path, pattern: &str) -> PathScope {
    PathScope {
        root: root.to_path_buf(),
        pattern: pattern.to_string(),
    }
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn read_all(handle: &mut ScopedRead) -> Result<Vec<u8>, GuardError> {
    let mut out = Vec::new();
    let mut buf = [0u8; 32];
    loop {
        let n = handle.read(&mut buf)?;
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&buf[..n]);
    }
}

fn link_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).unwrap();
}

fn link_dir(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(target, link).unwrap();
}

const FIXTURE: &[u8] = b"wanxiang-fixture";

fn grant_read(guard: &Guard, root: &Path) {
    guard
        .grant(ToolId::Cursor, Capability::Usage, scope(root, "**"))
        .unwrap();
}

#[test]
fn relative_path_opens_inside_the_granted_root() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("a.txt"), FIXTURE);
    let env = env();
    grant_read(&env.guard, &root);
    assert_eq!(open_fixture(&env.guard, Path::new("a.txt")), FIXTURE);
    assert_eq!(fs::read(root.join("a.txt")).unwrap(), FIXTURE);
}

fn open_fixture(guard: &Guard, path: &Path) -> Vec<u8> {
    let mut handle = guard
        .open_read(ToolId::Cursor, Capability::Usage, path)
        .unwrap();
    read_all(&mut handle).unwrap()
}

#[test]
fn empty_grant_table_is_empty() {
    let env = env();
    assert!(env.guard.grants().is_empty());
    assert!(env.guard.grants_for(ToolId::Cursor).is_empty());
}

#[test]
fn grant_is_visible_with_canonical_root() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    let id = env
        .guard
        .grant(
            ToolId::DsBot,
            Capability::Conversation,
            scope(&root, "sessions/*"),
        )
        .unwrap();
    let grants = env.guard.grants();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].id, id);
    assert_eq!(grants[0].tool, ToolId::DsBot);
    assert_eq!(grants[0].capability, Capability::Conversation);
    assert_eq!(grants[0].scope.root, root);
    assert_eq!(grants[0].scope.pattern, "sessions/*");
    assert_eq!(env.guard.grants_for(ToolId::DsBot), grants);
    assert!(env.guard.grants_for(ToolId::Cursor).is_empty());
}

#[test]
fn grants_keep_tools_capabilities_and_scopes_apart() {
    let tmp = Tmp::new();
    let root_a = tmp.dir("a");
    let root_b = tmp.dir("b");
    let env = env();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root_a, "**"))
        .unwrap();
    env.guard
        .grant(
            ToolId::Cursor,
            Capability::Conversation,
            scope(&root_a, "**"),
        )
        .unwrap();
    env.guard
        .grant(ToolId::Codex, Capability::Usage, scope(&root_b, "*.json"))
        .unwrap();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root_b, "**"))
        .unwrap();
    assert_eq!(env.guard.grants().len(), 4);
    assert_eq!(env.guard.grants_for(ToolId::Cursor).len(), 3);
    assert_eq!(env.guard.grants_for(ToolId::Codex).len(), 1);
    assert_eq!(env.guard.grants_for(ToolId::DeepSeekHarness).len(), 0);
}

#[test]
fn identical_grant_returns_the_same_id() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    let first = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    let second = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(env.guard.grants().len(), 1);
}

#[test]
fn revoke_removes_only_that_grant() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    let cursor = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    let codex = env
        .guard
        .grant(ToolId::Codex, Capability::Usage, scope(&root, "**"))
        .unwrap();
    env.guard.revoke(cursor).unwrap();
    let left = env.guard.grants();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, codex);
    assert_eq!(left[0].tool, ToolId::Codex);
    let err = env.guard.revoke(cursor).unwrap_err();
    assert!(matches!(err, GuardError::GrantNotFound));
    assert!(env.rec.events().is_empty() || env.rec.events().len() == 1);
    assert_eq!(env.rec.events(), vec![(ToolId::Cursor, Capability::Usage)]);
}

#[test]
fn grant_rejects_a_missing_or_unusable_root() {
    let tmp = Tmp::new();
    let env = env();
    let missing = tmp.path.join("missing-root");
    let err = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&missing, "**"))
        .unwrap_err();
    assert!(matches!(err, GuardError::RootMissing));
    let err = env
        .guard
        .grant(
            ToolId::Cursor,
            Capability::Usage,
            scope(Path::new("relative-root"), "**"),
        )
        .unwrap_err();
    assert!(matches!(err, GuardError::RootNotAbsolute));
    let file = tmp.path.join("file.txt");
    write_file(&file, b"x");
    let err = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&file, "**"))
        .unwrap_err();
    assert!(matches!(err, GuardError::RootNotDirectory));
    assert!(env.guard.grants().is_empty());
}

#[test]
fn grant_rejects_invalid_patterns() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    for pattern in ["", "/abs", "a/../b", "a\\b", "a//b"] {
        let err = env
            .guard
            .grant(ToolId::Cursor, Capability::Usage, scope(&root, pattern))
            .unwrap_err();
        assert!(
            matches!(err, GuardError::InvalidPattern),
            "{pattern} -> {err:?}"
        );
    }
    assert!(env.guard.grants().is_empty());
}

#[test]
fn grant_stores_the_canonical_root() {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let raw = std::env::temp_dir().join(format!("wanxiang-m1a-raw-{}-{}", std::process::id(), n));
    fs::create_dir_all(&raw).unwrap();
    let canon = fs::canonicalize(&raw).unwrap();
    let env = env();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&raw, "**"))
        .unwrap();
    assert_eq!(env.guard.grants()[0].scope.root, canon);
    write_file(&raw.join("a.txt"), FIXTURE);
    assert_eq!(open_fixture(&env.guard, &raw.join("a.txt")), FIXTURE);
    let _ = fs::remove_dir_all(&canon);
}

#[test]
fn symlink_root_is_stored_as_its_target() {
    let tmp = Tmp::new();
    let real = tmp.dir("real");
    let link = tmp.path.join("link");
    link_dir(&real, &link);
    let env = env();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&link, "**"))
        .unwrap();
    assert_eq!(env.guard.grants()[0].scope.root, real);
}

#[test]
fn file_patterns_match_one_segment_or_a_tree() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("a.json"), b"a");
    write_file(&root.join("a.txt"), b"t");
    write_file(&root.join("sub").join("a.json"), b"s");
    let env = env();

    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "*"))
        .unwrap();
    assert_eq!(open_fixture(&env.guard, &root.join("a.json")), b"a");
    assert!(matches!(
        env.guard
            .open_read(
                ToolId::Cursor,
                Capability::Usage,
                &root.join("sub").join("a.json")
            )
            .unwrap_err(),
        GuardError::PatternMismatch
    ));

    env.guard.revoke(env.guard.grants()[0].id).unwrap();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "*.json"))
        .unwrap();
    assert_eq!(open_fixture(&env.guard, &root.join("a.json")), b"a");
    assert!(matches!(
        env.guard
            .open_read(ToolId::Cursor, Capability::Usage, &root.join("a.txt"))
            .unwrap_err(),
        GuardError::PatternMismatch
    ));
    assert!(matches!(
        env.guard
            .open_read(
                ToolId::Cursor,
                Capability::Usage,
                &root.join("sub").join("a.json")
            )
            .unwrap_err(),
        GuardError::PatternMismatch
    ));

    env.guard.revoke(env.guard.grants()[0].id).unwrap();
    env.guard
        .grant(
            ToolId::Cursor,
            Capability::Usage,
            scope(&root, "sub/*.json"),
        )
        .unwrap();
    assert_eq!(
        open_fixture(&env.guard, &root.join("sub").join("a.json")),
        b"s"
    );
    assert!(matches!(
        env.guard
            .open_read(ToolId::Cursor, Capability::Usage, &root.join("a.json"))
            .unwrap_err(),
        GuardError::PatternMismatch
    ));

    env.guard.revoke(env.guard.grants()[0].id).unwrap();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**/*.json"))
        .unwrap();
    assert_eq!(open_fixture(&env.guard, &root.join("a.json")), b"a");
    assert_eq!(
        open_fixture(&env.guard, &root.join("sub").join("a.json")),
        b"s"
    );
    assert!(matches!(
        env.guard
            .open_read(ToolId::Cursor, Capability::Usage, &root.join("a.txt"))
            .unwrap_err(),
        GuardError::PatternMismatch
    ));
}

#[test]
fn dot_component_is_normalized_and_dotdot_is_rejected() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("a.txt"), FIXTURE);
    write_file(&tmp.path.join("canary.txt"), b"canary");
    let env = env();
    grant_read(&env.guard, &root);
    assert_eq!(
        lexical_within(&root, Path::new("./a.txt")).unwrap(),
        root.join("a.txt")
    );
    assert_eq!(
        open_fixture(&env.guard, &root.join(".").join("a.txt")),
        FIXTURE
    );

    let inside = root.join("sub").join("..").join("a.txt");
    assert!(matches!(
        lexical_within(&root, Path::new("sub/../a.txt")).unwrap_err(),
        GuardError::PathEscape
    ));
    assert!(matches!(
        env.guard
            .open_read(ToolId::Cursor, Capability::Usage, &inside)
            .unwrap_err(),
        GuardError::PathEscape
    ));

    let outside = root.join("..").join("canary.txt");
    assert!(matches!(
        env.guard
            .open_read(ToolId::Cursor, Capability::Usage, &outside)
            .unwrap_err(),
        GuardError::PathEscape
    ));
    assert_eq!(fs::read(tmp.path.join("canary.txt")).unwrap(), b"canary");
}

#[test]
fn paths_outside_the_granted_root_are_rejected() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let other = tmp.dir("root-extra");
    write_file(&root.join("a.txt"), b"in");
    write_file(&other.join("a.txt"), b"out");
    let env = env();
    grant_read(&env.guard, &root);
    assert!(matches!(
        lexical_within(&root, &other.join("a.txt")).unwrap_err(),
        GuardError::UnregisteredRoot
    ));
    assert!(matches!(
        env.guard
            .open_read(ToolId::Cursor, Capability::Usage, &other.join("a.txt"))
            .unwrap_err(),
        GuardError::UnregisteredRoot
    ));
    assert!(matches!(
        env.guard
            .open_read(ToolId::Codex, Capability::Usage, &root.join("a.txt"))
            .unwrap_err(),
        GuardError::UnregisteredRoot
    ));
    assert!(matches!(
        env.guard
            .open_read(
                ToolId::Cursor,
                Capability::Conversation,
                &root.join("a.txt")
            )
            .unwrap_err(),
        GuardError::UnregisteredRoot
    ));
    assert_eq!(fs::read(other.join("a.txt")).unwrap(), b"out");
}

#[test]
fn missing_file_and_directory_are_not_read() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    fs::create_dir(root.join("sub")).unwrap();
    let env = env();
    grant_read(&env.guard, &root);
    let missing = root.join("missing.txt");
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &missing)
        .unwrap_err();
    assert!(matches!(err, GuardError::Io(std::io::ErrorKind::NotFound)));
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("sub"))
        .unwrap_err();
    assert!(matches!(err, GuardError::NotAFile));
}

#[test]
fn symlink_out_of_root_is_not_read() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let outside = tmp.dir("outside");
    write_file(&outside.join("secret.txt"), b"secret-bytes");
    write_file(&root.join("inside.txt"), FIXTURE);
    link_file(&outside.join("secret.txt"), &root.join("escape"));
    link_file(&root.join("inside.txt"), &root.join("stay"));
    let env = env();
    grant_read(&env.guard, &root);
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("escape"))
        .unwrap_err();
    assert!(matches!(err, GuardError::SymlinkEscape), "{err:?}");
    let err = env
        .guard
        .metadata(ToolId::Cursor, Capability::Usage, &root.join("escape"))
        .unwrap_err();
    assert!(matches!(err, GuardError::SymlinkEscape), "{err:?}");
    assert_eq!(open_fixture(&env.guard, &root.join("stay")), FIXTURE);
    assert_eq!(
        fs::read(outside.join("secret.txt")).unwrap(),
        b"secret-bytes"
    );
}

#[test]
fn replaced_root_inode_is_rejected() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("a.txt"), b"one");
    let env = env();
    grant_read(&env.guard, &root);
    let parked = tmp.path.join("parked");
    fs::rename(&root, &parked).unwrap();
    fs::create_dir(&root).unwrap();
    write_file(&root.join("a.txt"), b"two");
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("a.txt"))
        .unwrap_err();
    assert!(matches!(err, GuardError::SymlinkEscape), "{err:?}");
    assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"two");
}

#[test]
fn root_swapped_for_a_symlink_is_rejected() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let outside = tmp.dir("outside");
    write_file(&root.join("a.txt"), b"inside");
    write_file(&outside.join("a.txt"), b"outside");
    let env = env();
    grant_read(&env.guard, &root);
    let parked = tmp.path.join("parked-root");
    fs::rename(&root, &parked).unwrap();
    link_dir(&outside, &root);
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("a.txt"))
        .unwrap_err();
    assert!(matches!(err, GuardError::SymlinkEscape), "{err:?}");
}

#[test]
fn credential_files_are_denied_inside_a_grant() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("auth.json"), b"secret-auth");
    write_file(
        &root.join("nested").join(".credentials.yaml"),
        b"secret-yaml",
    );
    write_file(&root.join("notes.json"), b"notes");
    write_file(&root.join("models.json"), b"models");
    let env = env();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "auth.json"))
        .unwrap();
    for path in [
        root.join("auth.json"),
        root.join("nested").join(".credentials.yaml"),
    ] {
        let err = env
            .guard
            .open_read(ToolId::Cursor, Capability::Usage, &path)
            .unwrap_err();
        assert!(matches!(err, GuardError::CredentialDenied(_)), "{err:?}");
        let err = env
            .guard
            .metadata(ToolId::Cursor, Capability::SpaceScan, &path)
            .unwrap_err();
        assert!(matches!(err, GuardError::CredentialDenied(_)), "{err:?}");
    }
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("auth.json"))
        .unwrap_err();
    assert!(matches!(
        err,
        GuardError::CredentialDenied(CredentialKind::AuthJson)
    ));
    let err = env
        .guard
        .open_read(
            ToolId::Cursor,
            Capability::Usage,
            &root.join("nested").join(".credentials.yaml"),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        GuardError::CredentialDenied(CredentialKind::CredentialsYaml)
    ));
    assert_eq!(open_fixture(&env.guard, &root.join("notes.json")), b"notes");
    assert_eq!(
        open_fixture(&env.guard, &root.join("models.json")),
        b"models"
    );
    assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"secret-auth");
}

#[test]
fn credential_names_are_case_insensitive() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    grant_read(&env.guard, &root);
    for name in [
        "Auth.JSON",
        ".Credentials.YAML",
        "AUTH.JSON.",
        "auth.json ",
        "auth.json::$DATA",
    ] {
        let err = env
            .guard
            .open_read(ToolId::Cursor, Capability::Usage, &root.join(name))
            .unwrap_err();
        assert!(
            matches!(err, GuardError::CredentialDenied(_)),
            "{name} -> {err:?}"
        );
    }
}

#[test]
fn dotted_workbuddy_models_json_is_denied_and_other_files_can_be_read() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(
        &root.join(".workbuddy").join("models.json"),
        b"secret-models",
    );
    write_file(&root.join(".workbuddy").join("sessions.json"), b"sessions");
    let env = env();
    grant_read(&env.guard, &root);
    for path in [
        root.join(".workbuddy").join("models.json"),
        root.join(".WorkBuddy").join("models.json"),
    ] {
        let err = env
            .guard
            .open_read(ToolId::Cursor, Capability::Usage, &path)
            .unwrap_err();
        assert!(
            matches!(
                err,
                GuardError::CredentialDenied(CredentialKind::WorkBuddyModels)
            ),
            "{path:?} -> {err:?}"
        );
    }
    assert_eq!(
        open_fixture(&env.guard, &root.join(".workbuddy").join("sessions.json")),
        b"sessions"
    );
}

#[cfg(unix)]
#[test]
fn named_pipe_is_not_a_file_and_does_not_block() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let fifo = root.join("pipe");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success(), "mkfifo");
    let env = env();
    grant_read(&env.guard, &root);
    let started = std::time::Instant::now();
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &fifo)
        .unwrap_err();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(matches!(err, GuardError::NotAFile), "{err:?}");
    let err = env
        .guard
        .metadata(ToolId::Cursor, Capability::Usage, &fifo)
        .unwrap_err();
    assert!(matches!(err, GuardError::NotAFile), "{err:?}");
}

#[test]
fn workbuddy_models_json_is_denied() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(
        &root.join("WorkBuddy").join("models.json"),
        b"secret-models",
    );
    write_file(
        &root.join("workbuddy").join("config").join("models.json"),
        b"secret-models",
    );
    let env = env();
    grant_read(&env.guard, &root);
    for path in [
        root.join("WorkBuddy").join("models.json"),
        root.join("workbuddy").join("config").join("models.json"),
    ] {
        let err = env
            .guard
            .open_read(ToolId::Cursor, Capability::Usage, &path)
            .unwrap_err();
        assert!(
            matches!(
                err,
                GuardError::CredentialDenied(CredentialKind::WorkBuddyModels)
            ),
            "{err:?}"
        );
    }
}

#[test]
fn symlink_to_auth_json_is_denied() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("auth.json"), b"secret-auth");
    link_file(&root.join("auth.json"), &root.join("ok.txt"));
    let env = env();
    grant_read(&env.guard, &root);
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("ok.txt"))
        .unwrap_err();
    assert!(
        matches!(err, GuardError::CredentialDenied(CredentialKind::AuthJson)),
        "{err:?}"
    );
}

#[test]
fn auth_json_without_a_grant_is_still_denied() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("auth.json"), b"secret-auth");
    let env = env();
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &root.join("auth.json"))
        .unwrap_err();
    assert!(matches!(
        err,
        GuardError::CredentialDenied(CredentialKind::AuthJson)
    ));
    assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"secret-auth");
}

#[test]
fn credentials_path_component_is_denied() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    write_file(&root.join("credentials").join("notes.txt"), b"secret");
    write_file(&root.join(".credentials").join("notes.txt"), b"secret");
    write_file(&root.join(".Credentials.bak").join("notes.txt"), b"secret");
    let env = env();
    grant_read(&env.guard, &root);
    for path in [
        root.join("credentials").join("notes.txt"),
        root.join(".credentials").join("notes.txt"),
        root.join(".Credentials.bak").join("notes.txt"),
    ] {
        let err = env
            .guard
            .open_read(ToolId::Cursor, Capability::Usage, &path)
            .unwrap_err();
        assert!(
            matches!(
                err,
                GuardError::CredentialDenied(CredentialKind::CredentialsComponent)
            ),
            "{err:?}"
        );
    }
}

#[test]
fn cursor_global_keys_and_keychain_entries_are_classified_without_files() {
    assert!(matches!(
        classify_cursor_global_key("cursorAuth").unwrap_err(),
        GuardError::CredentialDenied(CredentialKind::CursorAuthKey)
    ));
    assert!(matches!(
        classify_cursor_global_key("cursorAuth/accessToken").unwrap_err(),
        GuardError::CredentialDenied(CredentialKind::CursorAuthKey)
    ));
    assert!(matches!(
        classify_cursor_global_key("CursorAuth/openAIKey").unwrap_err(),
        GuardError::CredentialDenied(CredentialKind::CursorAuthKey)
    ));
    assert!(classify_cursor_global_key("cursorAuthExtra").is_ok());
    assert!(classify_cursor_global_key("storage.serviceMachineId").is_ok());

    for entry in [
        KeychainEntry {
            service: "example".into(),
            account: "user".into(),
        },
        KeychainEntry {
            service: String::new(),
            account: String::new(),
        },
    ] {
        assert!(matches!(
            classify_keychain(&entry).unwrap_err(),
            GuardError::CredentialDenied(CredentialKind::Keychain)
        ));
    }
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    grant_read(&env.guard, &root);
    assert!(matches!(
        classify_keychain(&KeychainEntry {
            service: "still-denied".into(),
            account: "still-denied".into(),
        })
        .unwrap_err(),
        GuardError::CredentialDenied(CredentialKind::Keychain)
    ));
}

#[test]
fn space_scan_returns_metadata_and_cannot_read_bytes() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let file = root.join("usage.txt");
    write_file(&file, b"hello-scan");
    let dir = root.join("subdir");
    fs::create_dir(&dir).unwrap();
    let env = env();
    env.guard
        .grant(ToolId::Codex, Capability::SpaceScan, scope(&root, "**"))
        .unwrap();
    let err = env
        .guard
        .open_read(ToolId::Codex, Capability::SpaceScan, &file)
        .unwrap_err();
    assert!(matches!(err, GuardError::NotGranted));
    assert!(!Capability::SpaceScan.is_write());
    let meta = env
        .guard
        .metadata(ToolId::Codex, Capability::SpaceScan, &file)
        .unwrap();
    let disk = fs::metadata(&file).unwrap();
    assert_eq!(meta.len, b"hello-scan".len() as u64);
    assert_eq!(meta.len, disk.len());
    assert_eq!(meta.modified, disk.modified().unwrap());
    assert!(!meta.is_dir);
    let dir_meta: FileMeta = env
        .guard
        .metadata(ToolId::Codex, Capability::SpaceScan, &dir)
        .unwrap();
    assert!(dir_meta.is_dir);
}

#[test]
fn revoked_handle_cannot_read_and_other_capability_stays() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let file = root.join("a.txt");
    write_file(&file, FIXTURE);
    let env = env();
    let usage = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    env.guard
        .grant(ToolId::Cursor, Capability::Conversation, scope(&root, "**"))
        .unwrap();
    let mut usage_handle = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &file)
        .unwrap();
    let mut conversation = env
        .guard
        .open_read(ToolId::Cursor, Capability::Conversation, &file)
        .unwrap();
    assert_eq!(read_all(&mut usage_handle).unwrap(), FIXTURE);
    env.guard.revoke(usage).unwrap();
    let err = usage_handle.read(&mut [0; 8]).unwrap_err();
    assert!(matches!(err, GuardError::GrantRevoked));
    assert_eq!(read_all(&mut conversation).unwrap(), FIXTURE);
    assert_eq!(env.rec.events(), vec![(ToolId::Cursor, Capability::Usage)]);
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &file)
        .unwrap_err();
    assert!(matches!(err, GuardError::UnregisteredRoot));
}

#[test]
fn revoking_one_scope_leaves_the_other_scope() {
    let tmp = Tmp::new();
    let root_a = tmp.dir("a");
    let root_b = tmp.dir("b");
    write_file(&root_a.join("a.txt"), b"aaa");
    write_file(&root_b.join("b.txt"), b"bbb");
    let env = env();
    let id_a = env
        .guard
        .grant(ToolId::DsBot, Capability::Cleanup, scope(&root_a, "**"))
        .unwrap();
    env.guard
        .grant(ToolId::DsBot, Capability::Cleanup, scope(&root_b, "**"))
        .unwrap();
    let mut handle_a = env
        .guard
        .open_read(ToolId::DsBot, Capability::Cleanup, &root_a.join("a.txt"))
        .unwrap();
    let mut handle_b = env
        .guard
        .open_read(ToolId::DsBot, Capability::Cleanup, &root_b.join("b.txt"))
        .unwrap();
    env.guard.revoke(id_a).unwrap();
    assert!(matches!(
        handle_a.read(&mut [0; 4]).unwrap_err(),
        GuardError::GrantRevoked
    ));
    assert_eq!(read_all(&mut handle_b).unwrap(), b"bbb");
    assert_eq!(env.rec.events(), vec![(ToolId::DsBot, Capability::Cleanup)]);
}

#[test]
fn revoking_one_tool_leaves_another_tools_handle() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let file = root.join("a.txt");
    write_file(&file, FIXTURE);
    let env = env();
    let cursor = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    env.guard
        .grant(ToolId::Codex, Capability::Usage, scope(&root, "**"))
        .unwrap();
    let mut codex = env
        .guard
        .open_read(ToolId::Codex, Capability::Usage, &file)
        .unwrap();
    env.guard.revoke(cursor).unwrap();
    assert_eq!(read_all(&mut codex).unwrap(), FIXTURE);
    assert_eq!(env.rec.events(), vec![(ToolId::Cursor, Capability::Usage)]);
}

#[test]
fn confirmation_redeem_is_single_use_and_bound_to_the_plan() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    env.guard
        .grant(ToolId::Cursor, Capability::Cleanup, scope(&root, "**"))
        .unwrap();
    env.guard
        .grant(
            ToolId::Cursor,
            Capability::InsertContent,
            scope(&root, "**"),
        )
        .unwrap();
    let plan = b"delete cache";
    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();
    let other = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();
    assert_eq!(format!("{token:?}"), "ConfirmToken(..)");
    assert!(!format!("{token:?}").contains("delete"));
    env.guard
        .redeem_confirmation(&token, ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();
    assert!(matches!(
        env.guard
            .redeem_confirmation(&token, ToolId::Cursor, Capability::Cleanup, plan)
            .unwrap_err(),
        GuardError::ConfirmationUsed
    ));
    env.guard
        .redeem_confirmation(&other, ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();

    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();
    let mut changed = plan.to_vec();
    changed[0] ^= 1;
    assert!(matches!(
        env.guard
            .redeem_confirmation(&token, ToolId::Cursor, Capability::Cleanup, &changed)
            .unwrap_err(),
        GuardError::ContentMismatch
    ));
    assert!(matches!(
        env.guard
            .redeem_confirmation(&token, ToolId::Cursor, Capability::Cleanup, plan)
            .unwrap_err(),
        GuardError::ConfirmationVoid
    ));

    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();
    assert!(matches!(
        env.guard
            .redeem_confirmation(&token, ToolId::Codex, Capability::Cleanup, plan)
            .unwrap_err(),
        GuardError::ContentMismatch
    ));
    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, plan)
        .unwrap();
    assert!(matches!(
        env.guard
            .redeem_confirmation(&token, ToolId::Cursor, Capability::InsertContent, plan)
            .unwrap_err(),
        GuardError::ContentMismatch
    ));

    let empty = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, b"")
        .unwrap();
    env.guard
        .redeem_confirmation(&empty, ToolId::Cursor, Capability::Cleanup, b"")
        .unwrap();
}

#[test]
fn confirmation_rejects_unknown_read_only_and_ungranted_actions() {
    let env = env();
    let bogus = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, b"plan")
        .unwrap_err();
    assert!(matches!(bogus, GuardError::NotGranted));
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    assert!(matches!(
        env.guard
            .issue_confirmation(ToolId::Cursor, Capability::Usage, b"plan")
            .unwrap_err(),
        GuardError::NotAWriteAction
    ));
    assert!(matches!(
        env.guard
            .issue_confirmation(ToolId::Cursor, Capability::SpaceScan, b"plan")
            .unwrap_err(),
        GuardError::NotAWriteAction
    ));
    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, b"plan");
    assert!(matches!(token, Err(GuardError::NotGranted)));
    let issued = {
        env.guard
            .grant(ToolId::Cursor, Capability::Cleanup, scope(&root, "**"))
            .unwrap();
        env.guard
            .issue_confirmation(ToolId::Cursor, Capability::Cleanup, b"plan")
            .unwrap()
    };
    let unknown = env
        .guard
        .issue_confirmation(ToolId::Codex, Capability::Cleanup, b"plan");
    assert!(matches!(unknown, Err(GuardError::NotGranted)));
    env.guard
        .redeem_confirmation(&issued, ToolId::Cursor, Capability::Cleanup, b"nope")
        .unwrap_err();
    let err = env
        .guard
        .redeem_confirmation(&issued, ToolId::Cursor, Capability::Cleanup, b"plan")
        .unwrap_err();
    assert!(matches!(err, GuardError::ConfirmationVoid));
    drop(issued);
}

#[test]
fn unknown_token_is_rejected() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let issuer = env();
    let other = env();
    issuer
        .guard
        .grant(ToolId::Cursor, Capability::Cleanup, scope(&root, "**"))
        .unwrap();
    other
        .guard
        .grant(ToolId::Cursor, Capability::Cleanup, scope(&root, "**"))
        .unwrap();
    let token = issuer
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::Cleanup, b"plan")
        .unwrap();
    let err = other
        .guard
        .redeem_confirmation(&token, ToolId::Cursor, Capability::Cleanup, b"plan")
        .unwrap_err();
    assert!(matches!(err, GuardError::ConfirmationUnknown));
}

#[test]
fn confirmation_expires_after_ten_minutes() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    env.guard
        .grant(
            ToolId::Cursor,
            Capability::InsertContent,
            scope(&root, "**"),
        )
        .unwrap();
    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::InsertContent, b"put")
        .unwrap();
    env.clock.advance(CONFIRM_TTL - Duration::from_secs(1));
    env.guard
        .redeem_confirmation(&token, ToolId::Cursor, Capability::InsertContent, b"put")
        .unwrap();

    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::InsertContent, b"put")
        .unwrap();
    env.clock.advance(CONFIRM_TTL);
    let err = env
        .guard
        .redeem_confirmation(&token, ToolId::Cursor, Capability::InsertContent, b"put")
        .unwrap_err();
    assert!(matches!(err, GuardError::ConfirmationExpired));
    let err = env
        .guard
        .redeem_confirmation(
            &token,
            ToolId::Cursor,
            Capability::InsertContent,
            b"changed",
        )
        .unwrap_err();
    assert!(matches!(err, GuardError::ConfirmationExpired));
}

#[test]
fn confirmation_fails_after_its_grant_is_revoked() {
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let env = env();
    let id = env
        .guard
        .grant(
            ToolId::Cursor,
            Capability::CreateConversation,
            scope(&root, "**"),
        )
        .unwrap();
    let token = env
        .guard
        .issue_confirmation(ToolId::Cursor, Capability::CreateConversation, b"open")
        .unwrap();
    env.guard.revoke(id).unwrap();
    let err = env
        .guard
        .redeem_confirmation(
            &token,
            ToolId::Cursor,
            Capability::CreateConversation,
            b"open",
        )
        .unwrap_err();
    assert!(matches!(err, GuardError::NotGranted));
    assert_eq!(
        env.rec.events(),
        vec![(ToolId::Cursor, Capability::CreateConversation)]
    );
}

#[test]
fn error_text_omits_paths() {
    let tmp = Tmp::new();
    let missing = tmp.path.join("missing-root-marker");
    let env = env();
    let err = env
        .guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&missing, "**"))
        .unwrap_err();
    assert!(!err.to_string().contains("wanxiang-m1a"), "{}", err);
    assert!(!err.to_string().contains("missing-root-marker"), "{}", err);
    assert!(!format!("{err:?}").contains("wanxiang-m1a"));

    let root = tmp.dir("root");
    env.guard
        .grant(ToolId::Cursor, Capability::Usage, scope(&root, "**"))
        .unwrap();
    let absent = root.join("no-such-file");
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &absent)
        .unwrap_err();
    assert_eq!(err.to_string(), "文件不存在");
    assert!(!err.to_string().contains("wanxiang-m1a"));
    assert!(!err.to_string().contains("no-such-file"));
    assert!(!format!("{err:?}").contains("no-such-file"));
    assert!(!format!("{err:?}").contains("wanxiang-m1a"));
}

#[cfg(windows)]
#[test]
fn windows_short_name_of_auth_json_is_denied() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let tmp = Tmp::new();
    let root = tmp.dir("root");
    let file = root.join("auth.json");
    write_file(&file, b"secret-auth");
    let allowed = root.join("notes.txt");
    write_file(&allowed, b"notes");
    let env = env();
    grant_read(&env.guard, &root);

    let short_auth = short_path(&file);
    let short_notes = short_path(&allowed);
    assert_ne!(
        short_auth
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase(),
        "auth.json",
        "这台 Windows 没有生成 8.3 短文件名"
    );
    let err = env
        .guard
        .open_read(ToolId::Cursor, Capability::Usage, &short_auth)
        .unwrap_err();
    assert!(
        matches!(err, GuardError::CredentialDenied(CredentialKind::AuthJson)),
        "{err:?}"
    );
    assert_eq!(open_fixture(&env.guard, &short_notes), b"notes");

    fn short_path(path: &Path) -> PathBuf {
        extern "system" {
            fn GetShortPathNameW(long: *const u16, short: *mut u16, size: u32) -> u32;
        }
        let long: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut buf = vec![0u16; 32768];
        let n = unsafe { GetShortPathNameW(long.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        assert!(n > 0 && (n as usize) < buf.len(), "GetShortPathNameW 失败");
        PathBuf::from(std::ffi::OsString::from_wide(&buf[..n as usize]))
    }
}
