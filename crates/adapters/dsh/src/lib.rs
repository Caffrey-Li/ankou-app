//! DeepSeek Harness 与 DS Bot 的接入占位。
//! M0 不读取这两个工具的本机数据。

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
