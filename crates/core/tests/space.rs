use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use wanxiang_core::{
    canonical_bytes, classify, cleanup, cleanup_block, harness_executable_names, scan, ui_entry,
    CleanEntry, ConfirmToken, Guard, GuardError, PathScope, RunningView, SpaceClass, SpaceError,
    ToolId, ToolScheduler, TrashFault, Trasher, LABEL_BUSY, LABEL_CHANGED, LABEL_OUTSIDE,
    LABEL_PERMISSION, LABEL_PROTECTED, LABEL_QUIT_CODEX, LABEL_QUIT_CURSOR, LABEL_QUIT_DSH_PROCESS,
    LABEL_QUIT_HARNESS, LABEL_REAUTH, LABEL_SCAN_CANCEL, LABEL_SCAN_DENIED, LABEL_TRASH,
    LABEL_TRASHED, LABEL_UNKNOWN_APP, LABEL_UNKNOWN_DSH,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let raw = std::env::temp_dir().join(format!("wanxiang-m4a-{}-{}", std::process::id(), n));
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

struct Noop;
impl ToolScheduler for Noop {
    fn on_revoked(&self, _tool: ToolId, _capability: wanxiang_core::Capability) {}
}

fn open_guard() -> Guard {
    Guard::new(Arc::new(Noop), Arc::new(wanxiang_core::SystemClock))
}

fn grant(guard: &Guard, tool: ToolId, root: &Path) {
    guard
        .grant(
            tool,
            wanxiang_core::Capability::SpaceScan,
            PathScope {
                root: root.to_path_buf(),
                pattern: "**".to_string(),
            },
        )
        .unwrap();
    for pattern in wanxiang_core::CLEANUP_PATTERNS {
        guard
            .grant(
                tool,
                wanxiang_core::Capability::Cleanup,
                PathScope {
                    root: root.to_path_buf(),
                    pattern: (*pattern).to_string(),
                },
            )
            .unwrap();
    }
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn idle() -> (Vec<String>, Vec<String>, Vec<String>) {
    (
        Vec::new(),
        Vec::new(),
        vec!["DeepSeek Harness".into(), "dsh".into()],
    )
}

fn view<'a>(
    bundles: &'a [String],
    processes: &'a [String],
    names: &'a [String],
) -> RunningView<'a> {
    RunningView {
        bundle_ids: Some(bundles),
        process_names: Some(processes),
        harness_executables: Some(names),
    }
}

#[derive(Default)]
struct RecTrash {
    paths: Vec<PathBuf>,
    fault: Option<TrashFault>,
}

impl Trasher for RecTrash {
    fn trash(&mut self, path: &Path) -> Result<(), TrashFault> {
        self.paths.push(path.to_path_buf());
        match self.fault {
            Some(fault) => Err(fault),
            None => Ok(()),
        }
    }
}

fn issue(guard: &Guard, tool: ToolId, entries: &[CleanEntry]) -> ConfirmToken {
    guard
        .issue_confirmation(
            tool,
            wanxiang_core::Capability::Cleanup,
            &canonical_bytes(tool, entries),
        )
        .unwrap()
}

#[test]
fn classes_follow_the_plan() {
    let harness = ToolId::DeepSeekHarness;
    assert_eq!(classify(harness, Path::new("cache")), SpaceClass::Cleanable);
    assert_eq!(
        classify(harness, Path::new("cache/a")),
        SpaceClass::Cleanable
    );
    assert_eq!(classify(harness, Path::new("logs")), SpaceClass::Cleanable);
    assert_eq!(
        classify(harness, Path::new("attachments/v1/tmp")),
        SpaceClass::Cleanable
    );
    assert_eq!(
        classify(harness, Path::new("attachments/v1/tmp/x")),
        SpaceClass::Cleanable
    );
    assert_eq!(
        classify(harness, Path::new("storages/session_projcache")),
        SpaceClass::Cleanable
    );
    assert_eq!(
        classify(harness, Path::new("bot/usage.sqlite")),
        SpaceClass::Protected
    );
    assert_eq!(
        classify(harness, Path::new("notes.txt")),
        SpaceClass::Protected
    );
    assert_eq!(classify(harness, Path::new("auth.json")), SpaceClass::Skip);
    assert_eq!(
        classify(harness, Path::new(".credentials.yaml")),
        SpaceClass::Skip
    );
    assert_eq!(
        classify(harness, Path::new("cursorAuth/key")),
        SpaceClass::Skip
    );
    assert_eq!(
        classify(ToolId::DsBot, Path::new("cache/a")),
        SpaceClass::Cleanable
    );
    assert_eq!(
        classify(ToolId::DsBot, Path::new("bot/usage.sqlite")),
        SpaceClass::Protected
    );
    for name in ["Cache", "CachedData", "Code Cache", "GPUCache", "logs"] {
        assert_eq!(
            classify(ToolId::Cursor, Path::new(name)),
            SpaceClass::Protected,
            "{name}"
        );
    }
    assert_eq!(
        classify(ToolId::Cursor, Path::new(&format!("state.{}", "vscdb"))),
        SpaceClass::Protected
    );
    assert_eq!(
        classify(ToolId::Codex, Path::new("cache")),
        SpaceClass::Protected
    );
    assert_eq!(
        classify(ToolId::Codex, Path::new("sessions/a.jsonl")),
        SpaceClass::Protected
    );
}

#[test]
fn scan_splits_cleanable_children_and_skips_secret_bytes() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    fs::create_dir(&root).unwrap();
    write(&root.join("bot/usage.sqlite"), b"database");
    write(&root.join("auth.json"), vec![9; 4000].as_slice());
    write(&root.join(".credentials.yaml"), vec![8; 4000].as_slice());
    write(&root.join("cursorAuth/key"), vec![7; 4000].as_slice());
    write(&root.join("cache/item"), b"abcd");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scan = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    assert_eq!(scan.protected_bytes, b"database".len() as u64);
    assert_eq!(scan.cleanable_bytes, 4);
    assert_eq!(scan.entries.len(), 1);
    assert_eq!(scan.entries[0].rel, Path::new("cache/item"));
    assert_eq!(scan.entries[0].bytes, 4);
    assert_eq!(scan.entries[0].files, 1);
    assert!(scan.entries[0].ino > 0);
}

#[test]
fn grown_file_counts_even_when_directory_mtime_stays() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    fs::create_dir(&root).unwrap();
    write(&root.join("note.txt"), b"0123456789");
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    File::open(&root).unwrap().set_modified(stamp).unwrap();
    let guard = open_guard();
    grant(&guard, ToolId::Codex, &root);
    let first = scan(&guard, ToolId::Codex, &AtomicBool::new(false)).unwrap();
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(root.join("note.txt"))
        .unwrap();
    file.write_all(b"more!").unwrap();
    drop(file);
    File::open(&root).unwrap().set_modified(stamp).unwrap();
    assert_eq!(fs::metadata(&root).unwrap().modified().unwrap(), stamp);
    let second = scan(&guard, ToolId::Codex, &AtomicBool::new(false)).unwrap();
    assert!(second.protected_bytes > first.protected_bytes);
    assert_eq!(first.protected_bytes, 10);
    assert_eq!(second.protected_bytes, 15);
}

#[test]
fn symlink_is_not_followed_and_hardlink_is_counted_once() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    fs::create_dir_all(root.join("cache")).unwrap();
    let outside = tmp.path.join("outside");
    write(&outside, vec![1; 8000].as_slice());
    write(&root.join("cache/a"), b"12345");
    fs::hard_link(root.join("cache/a"), root.join("cache/b")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("cache/link")).unwrap();
    write(&root.join("cache/keep"), b"xy");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scan = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    assert_eq!(scan.cleanable_bytes, 7);
    let rels: Vec<_> = scan.entries.iter().map(|entry| entry.rel.clone()).collect();
    assert_eq!(rels, vec![PathBuf::from("cache/keep")]);
}

#[test]
fn cancel_keeps_nothing_from_later_entries() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("big.txt"), vec![1; 100].as_slice());
    let guard = open_guard();
    grant(&guard, ToolId::Cursor, &root);
    let scan = scan(&guard, ToolId::Cursor, &AtomicBool::new(true)).unwrap();
    assert!(scan.cancelled);
    assert_eq!(scan.protected_bytes, 0);
    assert_eq!(wanxiang_core::scan_label(&scan), Some(LABEL_SCAN_CANCEL));
}

#[test]
fn unreadable_directory_is_permission_and_unscanned() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    let locked = root.join("locked");
    write(&locked.join("secret"), vec![1; 80].as_slice());
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0);
    }
    fs::set_permissions(&locked, perms).unwrap();
    let guard = open_guard();
    grant(&guard, ToolId::Cursor, &root);
    let scan = scan(&guard, ToolId::Cursor, &AtomicBool::new(false)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&locked).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&locked, perms).unwrap();
    }
    assert!(scan.permission);
    assert!(scan.unscanned >= 1);
    assert_eq!(scan.protected_bytes, 0);
    assert_eq!(wanxiang_core::scan_label(&scan), Some(LABEL_SCAN_DENIED));
}

#[test]
fn replaced_root_needs_reauth_and_revoked_grant_is_refused() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    fs::create_dir(&root).unwrap();
    let guard = open_guard();
    guard
        .grant(
            ToolId::Codex,
            wanxiang_core::Capability::SpaceScan,
            PathScope {
                root: root.clone(),
                pattern: "**".into(),
            },
        )
        .unwrap();
    fs::rename(&root, tmp.path.join("moved")).unwrap();
    fs::create_dir(&root).unwrap();
    let err = scan(&guard, ToolId::Codex, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(err, SpaceError::NeedsReauth);
    assert_eq!(err.label(), LABEL_REAUTH);
    let root = tmp.path.join("other");
    fs::create_dir(&root).unwrap();
    let fresh = open_guard();
    let id = fresh
        .grant(
            ToolId::Codex,
            wanxiang_core::Capability::SpaceScan,
            PathScope {
                root: root.clone(),
                pattern: "**".into(),
            },
        )
        .unwrap();
    fresh.revoke(id).unwrap();
    let err = scan(&fresh, ToolId::Codex, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(err, SpaceError::NotGranted);
}

#[test]
fn appended_nested_file_is_skipped_even_if_the_entry_inode_stays() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/job/nested/log"), b"a");
    write(&root.join("cache/keep"), b"zz");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    let job = scanned
        .entries
        .iter()
        .find(|entry| entry.rel == Path::new("cache/job"))
        .unwrap()
        .clone();
    let keep = scanned
        .entries
        .iter()
        .find(|entry| entry.rel == Path::new("cache/keep"))
        .unwrap()
        .clone();
    write(&root.join("cache/job/nested/extra"), b"bbbb");
    let now = fs::symlink_metadata(root.join("cache/job")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(now.ino(), job.ino);
    }
    let entries = vec![job, keep.clone()];
    let token = issue(&guard, ToolId::DeepSeekHarness, &entries);
    let (bundles, processes, names) = idle();
    let mut trash = RecTrash::default();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &entries,
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_CHANGED);
    assert_eq!(report.items[1].label, LABEL_TRASHED);
    assert_eq!(trash.paths.len(), 1);
    assert!(trash.paths[0].ends_with("keep"));
    assert!(trash.paths[0].starts_with(&root));
    assert_eq!(report.record.entry_count, 1);
    assert_eq!(report.record.total_bytes, keep.bytes);
    let text = format!("{:?}", report.record);
    assert!(!text.contains("cache"));
}

#[test]
fn directory_mtime_alone_does_not_skip() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/item"), b"abcd");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    let entry = scanned.entries[0].clone();
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    File::open(root.join("cache"))
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    let token = issue(&guard, ToolId::DeepSeekHarness, &[entry.clone()]);
    let (bundles, processes, names) = idle();
    let mut trash = RecTrash::default();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[entry],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_TRASHED);
    assert_eq!(trash.paths.len(), 1);
}

#[test]
fn changed_list_voids_the_token_and_trash_is_not_called() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/item"), b"abcd");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    let mut entry = scanned.entries[0].clone();
    let token = issue(&guard, ToolId::DeepSeekHarness, &[entry.clone()]);
    entry.bytes += 1;
    let (bundles, processes, names) = idle();
    let mut trash = RecTrash::default();
    let err = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[entry],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap_err();
    assert!(matches!(err, GuardError::ContentMismatch));
    assert!(trash.paths.is_empty());
}

#[test]
fn outside_protected_and_secret_entries_are_refused() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    fs::create_dir(&root).unwrap();
    write(&root.join("bot/usage.sqlite"), b"db");
    write(&root.join("auth.json"), b"secret");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let entries = vec![
        CleanEntry {
            rel: PathBuf::from("../out"),
            bytes: 1,
            files: 1,
            dev: 1,
            ino: 1,
        },
        CleanEntry {
            rel: PathBuf::from("bot/usage.sqlite"),
            bytes: 2,
            files: 1,
            dev: 1,
            ino: 1,
        },
        CleanEntry {
            rel: PathBuf::from("auth.json"),
            bytes: 6,
            files: 1,
            dev: 1,
            ino: 1,
        },
    ];
    let token = issue(&guard, ToolId::DeepSeekHarness, &entries);
    let (bundles, processes, names) = idle();
    let mut trash = RecTrash::default();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &entries,
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(
        report
            .items
            .iter()
            .map(|item| item.label)
            .collect::<Vec<_>>(),
        vec![LABEL_OUTSIDE, LABEL_PROTECTED, LABEL_PROTECTED]
    );
    assert!(trash.paths.is_empty());
    assert_eq!(report.record.entry_count, 0);
    assert!(!tmp.path.join("out").exists());
}

#[test]
fn trash_faults_use_the_planned_labels() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/busy"), b"a");
    write(&root.join("cache/nope"), b"b");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    let busy = scanned
        .entries
        .iter()
        .find(|entry| entry.rel.ends_with("busy"))
        .unwrap()
        .clone();
    let nope = scanned
        .entries
        .iter()
        .find(|entry| entry.rel.ends_with("nope"))
        .unwrap()
        .clone();
    let token = issue(&guard, ToolId::DeepSeekHarness, &[busy.clone()]);
    let (bundles, processes, names) = idle();
    let mut trash = RecTrash {
        fault: Some(TrashFault::Busy),
        ..RecTrash::default()
    };
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[busy],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_BUSY);
    let token = issue(&guard, ToolId::DeepSeekHarness, &[nope.clone()]);
    trash.fault = Some(TrashFault::Permission);
    trash.paths.clear();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[nope.clone()],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_PERMISSION);
    let token = issue(&guard, ToolId::DeepSeekHarness, &[nope.clone()]);
    trash.fault = Some(TrashFault::Other);
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[nope],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_TRASH);
    assert_eq!(report.record.entry_count, 0);
}

#[test]
fn running_harness_blocks_before_the_token_is_spent() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/item"), b"abcd");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    let entry = scanned.entries[0].clone();
    let token = issue(&guard, ToolId::DeepSeekHarness, &[entry.clone()]);
    let bundles = vec!["com.deepseek.dsh".into()];
    let processes = Vec::new();
    let names = vec!["DeepSeek Harness".into()];
    let mut trash = RecTrash::default();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[entry.clone()],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.blocked, Some(LABEL_QUIT_HARNESS));
    assert!(trash.paths.is_empty());
    let (bundles, processes, names) = idle();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[entry],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_TRASHED);
    assert_eq!(trash.paths.len(), 1);
}

#[test]
fn dsh_process_list_blocks_only_bundle_executable_names() {
    let names = vec!["DeepSeek Harness".into(), "dsh".into()];
    let bundles = Vec::new();
    let running = vec!["dsh".into()];
    assert_eq!(
        cleanup_block(ToolId::DsBot, &view(&bundles, &running, &names)),
        Some(LABEL_QUIT_DSH_PROCESS)
    );
    let unrelated = vec!["python3".into(), "node".into()];
    assert_eq!(
        cleanup_block(ToolId::DeepSeekHarness, &view(&bundles, &unrelated, &names)),
        None
    );
    assert_eq!(
        cleanup_block(
            ToolId::DeepSeekHarness,
            &RunningView {
                bundle_ids: Some(&bundles),
                process_names: None,
                harness_executables: Some(&names),
            }
        ),
        Some(LABEL_UNKNOWN_DSH)
    );
    assert_eq!(
        cleanup_block(
            ToolId::DsBot,
            &RunningView {
                bundle_ids: Some(&bundles),
                process_names: Some(&unrelated),
                harness_executables: None,
            }
        ),
        Some(LABEL_UNKNOWN_DSH)
    );
    let cursor = vec!["com.todesktop.230313mzl4w4u92".into()];
    assert_eq!(
        cleanup_block(ToolId::Cursor, &view(&cursor, &unrelated, &names)),
        Some(LABEL_QUIT_CURSOR)
    );
    let codex = vec!["com.openai.codex".into()];
    assert_eq!(
        cleanup_block(ToolId::Codex, &view(&codex, &unrelated, &names)),
        Some(LABEL_QUIT_CODEX)
    );
    assert_eq!(
        cleanup_block(
            ToolId::Cursor,
            &RunningView {
                bundle_ids: None,
                process_names: Some(&unrelated),
                harness_executables: Some(&names),
            }
        ),
        Some(LABEL_UNKNOWN_APP)
    );
}

#[test]
fn executable_names_come_from_the_bundle_directories_only() {
    let tmp = Tmp::new();
    let bundle = tmp.path.join("Harness.app");
    let macos = bundle.join("Contents/MacOS");
    let cli = bundle.join("Contents/Resources/runtime/cli/bin");
    let python = bundle.join("Contents/Resources/runtime/primary-runtime/dependencies/python/bin");
    fs::create_dir_all(&macos).unwrap();
    fs::create_dir_all(&cli).unwrap();
    fs::create_dir_all(&python).unwrap();
    for path in [
        macos.join("App Bin"),
        cli.join("cli-tool"),
        python.join("python3"),
    ] {
        fs::write(&path, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
    }
    let names = harness_executable_names(&bundle).unwrap();
    assert_eq!(names, vec!["App Bin".to_string(), "cli-tool".to_string()]);
    fs::remove_file(macos.join("App Bin")).unwrap();
    fs::remove_file(cli.join("cli-tool")).unwrap();
    assert!(harness_executable_names(&bundle).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn installed_harness_names_match_the_bundle() {
    let bundle = Path::new("/Applications/DeepSeek Harness.app");
    if !bundle.join("Contents/MacOS").is_dir() {
        return;
    }
    let names = harness_executable_names(bundle).unwrap();
    assert!(names.contains(&"DeepSeek Harness".to_string()), "{names:?}");
    assert!(names.contains(&"dsh".to_string()), "{names:?}");
    assert!(names.iter().all(|name| name != "python3" && name != "node"));
}

#[test]
fn ui_entries_only_accept_relative_paths() {
    assert!(ui_entry("cache/item", 1, 1, 1, 1).is_some());
    assert!(ui_entry("/tmp/cache/item", 1, 1, 1, 1).is_none());
    assert!(ui_entry("cache/../secret", 1, 1, 1, 1).is_none());
    assert!(ui_entry("C:/cache/item", 1, 1, 1, 1).is_none());
}

#[test]
fn swapped_cache_symlink_is_skipped() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/item"), b"abcd");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
    let entry = scanned.entries[0].clone();
    fs::remove_dir_all(root.join("cache")).unwrap();
    let outside = tmp.path.join("elsewhere");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("cache")).unwrap();
    let token = issue(&guard, ToolId::DeepSeekHarness, &[entry.clone()]);
    let (bundles, processes, names) = idle();
    let mut trash = RecTrash::default();
    let report = cleanup(
        &guard,
        ToolId::DeepSeekHarness,
        &[entry],
        &token,
        &view(&bundles, &processes, &names),
        &mut trash,
    )
    .unwrap();
    assert_eq!(report.items[0].label, LABEL_CHANGED);
    assert!(trash.paths.is_empty());
}

#[test]
fn guard_rejects_nested_and_protected_children() {
    let tmp = Tmp::new();
    let root = tmp.path.join("root");
    write(&root.join("cache/item"), b"ok");
    write(&root.join("cache/job/nested"), b"deep");
    write(&root.join("bot/note"), b"protected");
    let guard = open_guard();
    grant(&guard, ToolId::DeepSeekHarness, &root);
    let nested = guard
        .metadata(
            ToolId::DeepSeekHarness,
            wanxiang_core::Capability::Cleanup,
            &root.join("cache/job/nested"),
        )
        .unwrap_err();
    assert!(matches!(nested, GuardError::PatternMismatch));
    let protected = guard
        .metadata(
            ToolId::DeepSeekHarness,
            wanxiang_core::Capability::Cleanup,
            &root.join("bot/note"),
        )
        .unwrap_err();
    assert!(matches!(protected, GuardError::PatternMismatch));
    guard
        .metadata(
            ToolId::DeepSeekHarness,
            wanxiang_core::Capability::Cleanup,
            &root.join("cache/item"),
        )
        .unwrap();
}
