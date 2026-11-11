use proptest::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use wanxiang_core::{
    canonical_bytes, content_changed, scan, CleanEntry, Guard, PathScope, ToolId, ToolScheduler,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let raw =
            std::env::temp_dir().join(format!("wanxiang-m4a-prop-{}-{}", std::process::id(), n));
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

proptest! {
    #[test]
    fn list_bytes_ignore_order_and_change_with_content(
        bytes in 0u64..10_000,
        files in 0u64..20,
        dev in 1u64..9,
        ino in 1u64..9_000,
        extra in 1u64..50,
    ) {
        let left = CleanEntry {
            rel: PathBuf::from("cache/b"),
            bytes,
            files,
            dev,
            ino,
            modified: 0,
        };
        let right = CleanEntry {
            rel: PathBuf::from("cache/a"),
            bytes: bytes + extra,
            files,
            dev,
            ino: ino + 1,
            modified: 0,
        };
        let forward = canonical_bytes(ToolId::DeepSeekHarness, &[left.clone(), right.clone()]);
        let backward = canonical_bytes(ToolId::DeepSeekHarness, &[right.clone(), left.clone()]);
        prop_assert_eq!(forward.clone(), backward);
        let mut changed = left.clone();
        changed.bytes = changed.bytes.wrapping_add(extra);
        let tweaked = canonical_bytes(ToolId::DeepSeekHarness, &[changed, right]);
        prop_assert_ne!(forward, tweaked);
    }

    #[test]
    fn content_delta_skips_and_mtime_is_not_an_input(
        listed_bytes in 0u64..5_000,
        listed_files in 0u64..30,
        listed_dev in 1u64..5,
        listed_ino in 1u64..5_000,
        now_bytes in 0u64..5_000,
        now_files in 0u64..30,
        now_dev in 1u64..5,
        now_ino in 1u64..5_000,
        _mtime in 0u64..2_000_000_000,
    ) {
        let entry = CleanEntry {
            rel: PathBuf::from("cache/job"),
            bytes: listed_bytes,
            files: listed_files,
            dev: listed_dev,
            ino: listed_ino,
            modified: 0,
        };
        let changed = content_changed(&entry, now_bytes, now_files, now_dev, now_ino);
        prop_assert_eq!(
            changed,
            listed_bytes != now_bytes
                || listed_files != now_files
                || listed_dev != now_dev
                || listed_ino != now_ino
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn symlink_target_is_excluded_and_hardlink_bytes_are_once(
        size in 1usize..40,
        extra in 1usize..40,
        outside in 10usize..80,
    ) {
        let tmp = Tmp::new();
        let root = tmp.path.join("root");
        fs::create_dir_all(root.join("cache")).unwrap();
        let body = vec![1u8; size];
        fs::write(root.join("cache/a"), &body).unwrap();
        fs::hard_link(root.join("cache/a"), root.join("cache/b")).unwrap();
        fs::write(root.join("cache/keep"), vec![2u8; extra]).unwrap();
        let target = tmp.path.join("outside");
        fs::write(&target, vec![3u8; outside]).unwrap();
        std::os::unix::fs::symlink(&target, root.join("cache/link")).unwrap();
        let guard = Guard::new(Arc::new(Noop), Arc::new(wanxiang_core::SystemClock));
        guard
            .grant(
                ToolId::DeepSeekHarness,
                wanxiang_core::Capability::SpaceScan,
                PathScope {
                    root: root.clone(),
                    pattern: "**".into(),
                },
            )
            .unwrap();
        let scanned = scan(&guard, ToolId::DeepSeekHarness, &AtomicBool::new(false)).unwrap();
        prop_assert_eq!(scanned.cleanable_bytes, (size + extra) as u64);
        prop_assert!(scanned.entries.iter().all(|entry| entry.rel != PathBuf::from("cache/a")));
        prop_assert!(scanned.entries.iter().all(|entry| entry.rel != PathBuf::from("cache/b")));
        prop_assert!(scanned.entries.iter().all(|entry| entry.rel != PathBuf::from("cache/link")));
        prop_assert!(scanned.entries.iter().any(|entry| entry.rel == PathBuf::from("cache/keep")));
    }
}
