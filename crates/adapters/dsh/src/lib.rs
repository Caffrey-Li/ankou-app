//! DeepSeek Harness 与 DS Bot。DS Bot 用量读 `bot/usage.sqlite`，Harness 用量与对话读
//! `sessions/**/session.v4.jsonl.zstd`（排除 Bot 工作区）；余额只走钥匙串里的 Key。

mod balance;
mod calls;
mod sessions;

pub use balance::{fetch_balance, fetch_summary, parse_balance_json};
pub use sessions::{
    decode_zstd, is_bot_workspace, parse_session, read_session_bytes, session_chunks, session_paths,
    DshCall, DshSession, BOT_SESSION_PATTERN, HARNESS_PATTERN, SESSION_FILE,
};
pub use calls::{
    daily_counts, list_sessions, owner_kind, read_usage_copy, read_usage_set, session_transcript, summarize_calls,
    usage_db_candidates, CallRow, CallSummary, DayCount, OwnerKind, SessionHead,
};

pub fn availability() -> &'static str {
    "未验证"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Product {
    Harness,
    Bot,
}

pub fn data_dir(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".dsh")
}

pub fn installed(product: Product, home: &std::path::Path) -> bool {
    let _ = product;
    data_dir(home).is_dir()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn stays_unverified() {
        assert_eq!(super::availability(), "未验证");
    }

    fn temp() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wanxiang-dsh-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn discovery_checks_only_the_declared_directory() {
        let home = temp();
        assert!(!super::installed(super::Product::Harness, &home));
        assert!(!super::installed(super::Product::Bot, &home));
        fs::create_dir(home.join(".dsh")).unwrap();
        fs::write(home.join(".dsh").join("note.txt"), b"fixture").unwrap();
        assert!(super::installed(super::Product::Harness, &home));
        assert!(super::installed(super::Product::Bot, &home));
        let _ = fs::remove_dir_all(&home);
    }
}
