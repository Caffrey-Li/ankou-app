//! Codex 桌面应用接入占位。
//! M0 不读取 rollouts，也不连接 app-server。

pub fn availability() -> &'static str {
    "未验证"
}

pub fn installed(home: &std::path::Path, applications: &std::path::Path) -> bool {
    home.join(".codex").is_dir() || applications.join("Codex.app").is_dir()
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
            "wanxiang-codex-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn discovery_uses_the_temp_home_not_this_machine() {
        let home = temp();
        let apps = home.join("apps");
        fs::create_dir_all(&apps).unwrap();
        assert!(!super::installed(&home, &apps));
        fs::write(home.join(".codex"), b"not-a-directory").unwrap();
        assert!(!super::installed(&home, &apps));
        fs::remove_file(home.join(".codex")).unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        assert!(super::installed(&home, &apps));
        let _ = fs::remove_dir_all(&home);
    }
}
