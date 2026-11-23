use std::path::{Path, PathBuf};

use serde::Serialize;
use wanxiang_adapter_claude::installed as claude_installed;
use wanxiang_adapter_codex::installed as codex_installed;
use wanxiang_adapter_cursor::installed as cursor_installed;
use wanxiang_adapter_dsh::{installed as dsh_installed, Product};
use wanxiang_core::{GrantStatus, StoredGrant, ToolId, ToolPresence};

#[derive(Clone, Debug, Serialize)]
pub struct ToolSnapshot {
    pub tool: ToolId,
    pub installed: bool,
    pub presence: ToolPresence,
}

pub struct HostRoots {
    pub home: PathBuf,
    pub roaming: PathBuf,
    pub applications: PathBuf,
}

pub fn snapshot(roots: &HostRoots, grants: &[StoredGrant]) -> Vec<ToolSnapshot> {
    let found = [
        (
            ToolId::DeepSeekHarness,
            dsh_installed(Product::Harness, &roots.home),
        ),
        (ToolId::DsBot, dsh_installed(Product::Bot, &roots.home)),
        (
            ToolId::Cursor,
            cursor_installed(&roots.roaming, &roots.applications),
        ),
        (
            ToolId::Codex,
            codex_installed(&roots.home, &roots.applications, codex_bundle_id),
        ),
        (
            ToolId::Claude,
            claude_installed(&roots.home, &roots.applications),
        ),
    ];
    found
        .into_iter()
        .map(|(tool, installed)| {
            let statuses: Vec<GrantStatus> = grants
                .iter()
                .filter(|grant| grant.tool == tool)
                .map(|grant| grant.status)
                .collect();
            ToolSnapshot {
                tool,
                installed,
                presence: wanxiang_core::presence(installed, &statuses),
            }
        })
        .collect()
}

pub fn production_roots() -> HostRoots {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let roaming = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        home.join("Library").join(folder("Application", "Support"))
    } else {
        home.join(".local").join("share")
    };
    let applications = if cfg!(target_os = "macos") {
        PathBuf::from("/Applications")
    } else {
        PathBuf::new()
    };
    HostRoots {
        home,
        roaming,
        applications,
    }
}

fn folder(left: &str, right: &str) -> String {
    format!("{left} {right}")
}

fn codex_bundle_id(app: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        crate::icons::bundle_identifier(app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wanxiang-discover-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn temp_home_shows_installed_and_missing_without_reading_files() {
        let root = temp();
        let home = root.join("home");
        let roaming = root.join("roaming");
        let applications = root.join("apps");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&roaming).unwrap();
        fs::create_dir_all(&applications).unwrap();
        fs::create_dir(home.join(".dsh")).unwrap();
        fs::write(home.join(".dsh").join("note.txt"), b"fixture").unwrap();
        let roots = HostRoots {
            home,
            roaming,
            applications,
        };
        let found = snapshot(&roots, &[]);
        assert_eq!(found[0].presence, ToolPresence::Unauthorized);
        assert_eq!(found[1].presence, ToolPresence::Unauthorized);
        assert_eq!(found[2].presence, ToolPresence::Missing);
        assert_eq!(found[3].presence, ToolPresence::Missing);
        fs::create_dir(roots.roaming.join("Cursor")).unwrap();
        fs::create_dir(roots.home.join(".codex")).unwrap();
        let found = snapshot(&roots, &[]);
        assert_eq!(found[2].presence, ToolPresence::Unauthorized);
        assert_eq!(found[3].presence, ToolPresence::Unauthorized);
        let _ = fs::remove_dir_all(root);
    }
}
