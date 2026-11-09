use std::path::Path;

use wanxiang_core::{Capability, PathScope, Store, ToolId};

const BOT_PATTERN: &str = "bot/usage.sqlite*";
const CODEX_PATTERN: &str = "sessions/**/*.jsonl";

pub fn set(store: &Store, kind: &str, on: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "找不到本机目录".to_string())?;
    let home = std::path::PathBuf::from(home);
    set_at(store, kind, on, &home.join(".dsh"), &home.join(".codex"))
}

pub fn set_at(store: &Store, kind: &str, on: bool, dsh: &Path, codex: &Path) -> Result<(), String> {
    match kind {
        "ds_bot_usage" => set_path(store, ToolId::DsBot, dsh, BOT_PATTERN, on),
        "codex_usage" => set_path(store, ToolId::Codex, codex, CODEX_PATTERN, on),
        "deepseek_balance" => {
            if on {
                store.grant_account_link().map_err(show)
            } else {
                store.revoke_account_link().map_err(show)
            }
        }
        "codex_app_server" => {
            let mut settings = store.settings().map_err(show)?;
            settings.codex_app_server = on;
            store.set_settings(&settings).map_err(show)
        }
        _ => Err("没有这项授权".to_string()),
    }
}

fn set_path(store: &Store, tool: ToolId, root: &Path, pattern: &str, on: bool) -> Result<(), String> {
    if !on {
        let grants = store.grants().map_err(show)?;
        for grant in grants {
            if grant.tool == tool && grant.capability == Capability::Usage && grant.pattern == pattern {
                store.revoke(grant.id).map_err(show)?;
            }
        }
        return Ok(());
    }
    store
        .grant(
            tool,
            Capability::Usage,
            PathScope {
                root: root.to_path_buf(),
                pattern: pattern.to_string(),
            },
        )
        .map(|_| ())
        .map_err(show)
}

fn show(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use wanxiang_core::{Clock, Guard, RefreshGate, Store, Supervisor, SystemClock, ToolScheduler};

    use super::set_at;

    fn temp(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("wanxiang-measured-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("dsh")).unwrap();
        std::fs::create_dir_all(path.join("codex")).unwrap();
        path
    }

    fn open_store(dir: &Path) -> Store {
        let supervisor = Supervisor::new();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(Guard::new(Arc::clone(&gate) as Arc<dyn ToolScheduler>, Arc::clone(&clock)));
        Store::open(&dir.join("wanxiang.sqlite"), guard, clock).unwrap()
    }

    #[test]
    fn measured_grants_use_the_two_roots_and_can_turn_off() {
        let dir = temp("four");
        let store = open_store(&dir);
        let dsh = dir.join("dsh");
        let codex = dir.join("codex");
        set_at(&store, "ds_bot_usage", true, &dsh, &codex).unwrap();
        assert_eq!(store.grants().unwrap()[0].pattern, "bot/usage.sqlite*");
        assert_eq!(store.grants().unwrap()[0].root, dsh.canonicalize().unwrap());
        set_at(&store, "codex_usage", true, &dsh, &codex).unwrap();
        assert!(store.grants().unwrap().iter().any(|grant| {
            grant.pattern == "sessions/**/*.jsonl" && grant.root == codex.canonicalize().unwrap()
        }));
        set_at(&store, "deepseek_balance", true, &dsh, &codex).unwrap();
        assert_eq!(store.account_link_status().unwrap().as_deref(), Some("active"));
        set_at(&store, "codex_app_server", true, &dsh, &codex).unwrap();
        assert!(store.settings().unwrap().codex_app_server);
        set_at(&store, "ds_bot_usage", false, &dsh, &codex).unwrap();
        set_at(&store, "codex_usage", false, &dsh, &codex).unwrap();
        set_at(&store, "deepseek_balance", false, &dsh, &codex).unwrap();
        set_at(&store, "codex_app_server", false, &dsh, &codex).unwrap();
        assert!(store.grants().unwrap().is_empty());
        assert_ne!(store.account_link_status().unwrap().as_deref(), Some("active"));
        assert!(!store.settings().unwrap().codex_app_server);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
