//! Cursor 接入：对话只读本机对话数据库里与对话有关的键（见 chats.rs）。

mod chats;

pub use chats::{chat_chunks, list_chats, state_db, valid_id, CursorChat, STATE_PATTERN};

use wanxiang_core::{Capability, Guard, GuardError, ToolId};

pub fn installed(roaming: &std::path::Path, applications: &std::path::Path) -> bool {
    roaming.join("Cursor").is_dir() || applications.join("Cursor.app").is_dir()
}

pub fn read_fixture(guard: &Guard) -> Result<&'static str, GuardError> {
    if !guard.has_grant(ToolId::Cursor, Capability::Conversation) {
        return Err(GuardError::NotGranted);
    }
    Ok(include_str!("../fixtures/conversations.json"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wanxiang-cursor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn discovery_uses_the_temp_home_not_this_machine() {
        let home = temp();
        let roaming = home.join("roaming");
        let apps = home.join("apps");
        fs::create_dir_all(&roaming).unwrap();
        fs::create_dir_all(&apps).unwrap();
        assert!(!super::installed(&roaming, &apps));
        fs::create_dir(roaming.join("Cursor")).unwrap();
        assert!(super::installed(&roaming, &apps));
        let _ = fs::remove_dir_all(&home);
    }
}
