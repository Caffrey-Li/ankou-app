//! ChatGPT。用量只读 rollouts，额度窗口优先走 app-server，不打开登录文件。

mod locate;
mod rollout;
mod server;
mod threads;

pub use locate::{find_codex_cli, find_codex_cli_in};
pub use rollout::{
    combine_rollouts, parse_app_server_result, parse_rollout, read_rollouts, rollout_paths, RolloutUsage,
};
pub use server::query_rate_limits;
pub use threads::{
    context_percent, list_jsonl, list_threads, parse_jsonl_head, parse_jsonl_text, parse_thread_list,
    parse_thread_read, read_jsonl, read_thread, thread_list_line, thread_read_line, title_of, ThreadHead,
    ThreadText,
};

pub fn availability() -> &'static str {
    "未验证"
}

pub fn app_with_bundle_id(
    root: &std::path::Path,
    want: &str,
    bundle_id: &impl Fn(&std::path::Path) -> Option<String>,
) -> Option<std::path::PathBuf> {
    let mut found = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if !name.to_str().is_some_and(|name| name.ends_with(".app")) || !path.is_dir() {
            continue;
        }
        if bundle_id(&path).as_deref() != Some(want) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(path);
    }
    found
}

pub fn installed(
    home: &std::path::Path,
    applications: &std::path::Path,
    bundle_id: impl Fn(&std::path::Path) -> Option<String>,
) -> bool {
    home.join(".codex").is_dir()
        || app_with_bundle_id(applications, "com.openai.codex", &bundle_id).is_some()
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
        assert!(!super::installed(&home, &apps, |_| None));
        fs::write(home.join(".codex"), b"not-a-directory").unwrap();
        assert!(!super::installed(&home, &apps, |_| None));
        fs::remove_file(home.join(".codex")).unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        assert!(super::installed(&home, &apps, |_| None));
        let _ = fs::remove_dir_all(&home);
    }

    fn write_app(apps: &std::path::Path, bundle_id: &str) {
        let contents = apps.join("ChatGPT.app").join("Contents");
        fs::create_dir_all(&contents).unwrap();
        fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{bundle_id}</string></dict></plist>"#
            ),
        )
        .unwrap();
    }

    fn plist_bundle_id(app: &std::path::Path) -> Option<String> {
        let text = fs::read_to_string(app.join("Contents").join("Info.plist")).ok()?;
        let key = text.find("<key>CFBundleIdentifier</key>")?;
        let rest = &text[key..];
        let start = rest.find("<string>")? + "<string>".len();
        let end = start + rest[start..].find("</string>")?;
        Some(rest[start..end].to_string())
    }

    #[test]
    fn chat_bundle_is_not_installed_and_codex_bundle_is() {
        let home = temp();
        let apps = home.join("Applications");
        fs::create_dir_all(&apps).unwrap();
        write_app(&apps, "com.openai.chat");
        assert!(!super::installed(&home, &apps, plist_bundle_id));
        write_app(&apps, "com.openai.codex");
        assert!(super::installed(&home, &apps, plist_bundle_id));
        let _ = fs::remove_dir_all(&home);
    }
}
