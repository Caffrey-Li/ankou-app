use proptest::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use wanxiang_core::{
    lexical_within, Capability, Clock, CredentialKind, Guard, GuardError, PathScope, ScopedRead,
    ToolId, ToolScheduler,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let raw =
            std::env::temp_dir().join(format!("wanxiang-m1a-prop-{}-{}", std::process::id(), n));
        fs::create_dir_all(&raw).unwrap();
        Self {
            path: fs::canonicalize(&raw).unwrap(),
        }
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Rec;
impl ToolScheduler for Rec {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

struct FakeClock;
impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }
}

fn guard() -> Guard {
    Guard::new(Arc::new(Rec), Arc::new(FakeClock))
}

fn reserved(name: &str) -> bool {
    matches!(
        name,
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

fn parts() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z0-9]{1,8}", 1..6usize)
        .prop_filter("windows reserved name", |items| {
            items.iter().all(|p| !reserved(p))
        })
}

fn read_all(handle: &mut ScopedRead) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 32];
    loop {
        let n = handle.read(&mut buf).unwrap();
        if n == 0 {
            return out;
        }
        out.extend_from_slice(&buf[..n]);
    }
}

fn relative(parts: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for part in parts {
        path.push(part);
    }
    path
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn safe_relative_paths_open(parts in parts()) {
        let tmp = Tmp::new();
        let root = tmp.path.join("root");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(&root).unwrap();
        let mut file = root.clone();
        for (i, part) in parts.iter().enumerate() {
            file.push(part);
            if i + 1 == parts.len() {
                fs::write(&file, b"wanxiang-fixture").unwrap();
            } else {
                fs::create_dir(&file).unwrap();
            }
        }
        let guard = guard();
        guard
            .grant(ToolId::Cursor, Capability::Usage, PathScope { root: root.clone(), pattern: "**".into() })
            .unwrap();
        let lexical = lexical_within(&root, &relative(&parts)).unwrap();
        prop_assert_eq!(&lexical, &file);
        let mut handle = guard.open_read(ToolId::Cursor, Capability::Usage, &file).unwrap();
        prop_assert_eq!(read_all(&mut handle), b"wanxiang-fixture");
    }

    #[test]
    fn dotdot_is_always_rejected(parts in parts(), index in 0..6usize) {
        let tmp = Tmp::new();
        let root = tmp.path.join("root");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(&root).unwrap();
        let canary = tmp.path.join("canary.txt");
        fs::write(&canary, b"canary").unwrap();
        let mut dotted = parts.clone();
        dotted.insert(index % (parts.len() + 1), "..".into());
        let rel = relative(&dotted);
        prop_assert!(matches!(
            lexical_within(&root, &rel).unwrap_err(),
            GuardError::PathEscape
        ));
        let guard = guard();
        guard
            .grant(ToolId::Cursor, Capability::Usage, PathScope { root: root.clone(), pattern: "**".into() })
            .unwrap();
        prop_assert!(matches!(
            guard.open_read(ToolId::Cursor, Capability::Usage, &root.join(&rel)).unwrap_err(),
            GuardError::PathEscape
        ));
        prop_assert_eq!(fs::read(&canary).unwrap(), b"canary");
    }

    #[test]
    fn sibling_directories_are_unregistered(name in "[a-z0-9]{1,8}") {
        prop_assume!(!reserved(&name) && name != "root");
        let tmp = Tmp::new();
        let root = tmp.path.join("root");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(&root).unwrap();
        let sibling = tmp.path.join(&name);
        fs::create_dir(&sibling).unwrap();
        let file = sibling.join("a.txt");
        fs::write(&file, b"out").unwrap();
        let guard = guard();
        guard
            .grant(ToolId::Cursor, Capability::Usage, PathScope { root: root.clone(), pattern: "**".into() })
            .unwrap();
        prop_assert!(matches!(
            lexical_within(&root, &file).unwrap_err(),
            GuardError::UnregisteredRoot
        ));
        prop_assert!(matches!(
            guard.open_read(ToolId::Cursor, Capability::Usage, &file).unwrap_err(),
            GuardError::UnregisteredRoot
        ));
        prop_assert_eq!(fs::read(&file).unwrap(), b"out");
    }

    #[test]
    fn string_prefix_of_the_root_is_unregistered(suffix in "[a-z0-9]{1,4}") {
        let tmp = Tmp::new();
        let root = tmp.path.join("root");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(&root).unwrap();
        let evil = tmp.path.join(format!("root{suffix}"));
        fs::create_dir(&evil).unwrap();
        let file = evil.join("a.txt");
        fs::write(&file, b"out").unwrap();
        let guard = guard();
        guard
            .grant(ToolId::Cursor, Capability::Usage, PathScope { root: root.clone(), pattern: "**".into() })
            .unwrap();
        prop_assert!(matches!(
            lexical_within(&root, &file).unwrap_err(),
            GuardError::UnregisteredRoot
        ));
        prop_assert!(matches!(
            guard.open_read(ToolId::Cursor, Capability::Usage, &file).unwrap_err(),
            GuardError::UnregisteredRoot
        ));
    }

    #[test]
    fn auth_json_leaf_is_denied(dirs in prop::collection::vec("[a-z0-9]{1,8}", 0..4usize)) {
        prop_assume!(dirs.iter().all(|p| !reserved(p)));
        let tmp = Tmp::new();
        let root = tmp.path.join("root");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(&root).unwrap();
        let mut file = root.clone();
        for dir in &dirs {
            file.push(dir);
        }
        file.push("auth.json");
        let guard = guard();
        guard
            .grant(ToolId::Cursor, Capability::Usage, PathScope { root, pattern: "**".into() })
            .unwrap();
        prop_assert!(matches!(
            guard.open_read(ToolId::Cursor, Capability::Usage, &file).unwrap_err(),
            GuardError::CredentialDenied(CredentialKind::AuthJson)
        ));
    }
}
