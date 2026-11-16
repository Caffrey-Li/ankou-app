use std::path::Path;

use wanxiang_core::{Capability, PathScope, Store, ToolId};

const BOT_PATTERN: &str = "bot/usage.sqlite*";
const CODEX_PATTERN: &str = "sessions/**/*.jsonl";

pub fn set(store: &Store, kind: &str, on: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "找不到本机目录".to_string())?;
    let home = std::path::PathBuf::from(home);
    set_at(
        store,
        kind,
        on,
        &home.join(".dsh"),
        &home.join(".codex"),
        &cursor_support(&home),
    )
}

fn cursor_support(home: &Path) -> std::path::PathBuf {
    home.join("Library")
        .join(format!("{} {}", "Application", "Support"))
        .join("Cursor")
}

pub fn apply_space_fixture(store: &Store) -> Result<(), String> {
    let Some(root) = space_fixture_root() else {
        return Ok(());
    };
    let grants = store.grants().map_err(show)?;
    for grant in grants {
        if grant.tool == ToolId::DeepSeekHarness
            && (grant.capability == Capability::SpaceScan
                || grant.capability == Capability::Cleanup)
        {
            store.revoke(grant.id).map_err(show)?;
        }
    }
    set_at(store, "harness_space", true, &root, &root, &root)?;
    set_at(store, "harness_cleanup", true, &root, &root, &root)
}

#[cfg(debug_assertions)]
pub fn space_fixture_active() -> bool {
    space_fixture_root().is_some()
}

#[cfg(not(debug_assertions))]
pub fn space_fixture_active() -> bool {
    false
}

#[cfg(debug_assertions)]
fn space_fixture_root() -> Option<std::path::PathBuf> {
    let raw = std::env::var_os("WANXIANG_SPACE_FIXTURE")?;
    if raw.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(raw))
    }
}

#[cfg(not(debug_assertions))]
fn space_fixture_root() -> Option<std::path::PathBuf> {
    None
}

pub fn set_at(
    store: &Store,
    kind: &str,
    on: bool,
    dsh: &Path,
    codex: &Path,
    cursor: &Path,
) -> Result<(), String> {
    match kind {
        "ds_bot_usage" => set_path(
            store,
            ToolId::DsBot,
            Capability::Usage,
            dsh,
            BOT_PATTERN,
            on,
        ),
        "codex_usage" => set_path(
            store,
            ToolId::Codex,
            Capability::Usage,
            codex,
            CODEX_PATTERN,
            on,
        ),
        "harness_space" => set_path(
            store,
            ToolId::DeepSeekHarness,
            Capability::SpaceScan,
            dsh,
            "**",
            on,
        ),
        "codex_space" => set_path(store, ToolId::Codex, Capability::SpaceScan, codex, "**", on),
        "cursor_space" => set_path(
            store,
            ToolId::Cursor,
            Capability::SpaceScan,
            cursor,
            "**",
            on,
        ),
        "harness_cleanup" => {
            for pattern in wanxiang_core::CLEANUP_PATTERNS {
                set_path(
                    store,
                    ToolId::DeepSeekHarness,
                    Capability::Cleanup,
                    dsh,
                    pattern,
                    on,
                )?;
            }
            Ok(())
        }
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
        "codex_conversation" => set_path(
            store,
            ToolId::Codex,
            Capability::Conversation,
            codex,
            CODEX_PATTERN,
            on,
        ),
        "ds_bot_conversation" => {
            // 列表来自 Bot 的调用记录，正文来自 Bot 工作区的会话文件
            set_path(store, ToolId::DsBot, Capability::Conversation, dsh, BOT_PATTERN, on)?;
            set_path(
                store,
                ToolId::DsBot,
                Capability::Conversation,
                dsh,
                wanxiang_adapter_dsh::BOT_SESSION_PATTERN,
                on,
            )
        }
        "harness_usage" => set_path(
            store,
            ToolId::DeepSeekHarness,
            Capability::Usage,
            dsh,
            wanxiang_adapter_dsh::HARNESS_PATTERN,
            on,
        ),
        "harness_conversation" => set_path(
            store,
            ToolId::DeepSeekHarness,
            Capability::Conversation,
            dsh,
            wanxiang_adapter_dsh::HARNESS_PATTERN,
            on,
        ),
        "cursor_conversation" => set_path(
            store,
            ToolId::Cursor,
            Capability::Conversation,
            cursor,
            wanxiang_adapter_cursor::STATE_PATTERN,
            on,
        ),
        _ => Err("没有这项授权".to_string()),
    }
}

fn set_path(
    store: &Store,
    tool: ToolId,
    capability: Capability,
    root: &Path,
    pattern: &str,
    on: bool,
) -> Result<(), String> {
    if !on {
        let grants = store.grants().map_err(show)?;
        for grant in grants {
            if grant.tool == tool && grant.capability == capability && grant.pattern == pattern {
                store.revoke(grant.id).map_err(show)?;
            }
        }
        return Ok(());
    }
    store
        .grant(
            tool,
            capability,
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

    use super::{
        apply_space_fixture, cursor_support, set, set_at,
    };

    fn temp(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("wanxiang-measured-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("dsh")).unwrap();
        std::fs::create_dir_all(path.join("codex")).unwrap();
        path
    }

    fn open_store(dir: &Path) -> Store {
        let supervisor = Supervisor::new();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(Guard::new(
            Arc::clone(&gate) as Arc<dyn ToolScheduler>,
            Arc::clone(&clock),
        ));
        Store::open(&dir.join("wanxiang.sqlite"), guard, clock).unwrap()
    }

    #[test]
    fn measured_grants_use_the_two_roots_and_can_turn_off() {
        let dir = temp("four");
        let store = open_store(&dir);
        let dsh = dir.join("dsh");
        let codex = dir.join("codex");
        set_at(
            &store,
            "ds_bot_usage",
            true,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        assert_eq!(store.grants().unwrap()[0].pattern, "bot/usage.sqlite*");
        assert_eq!(store.grants().unwrap()[0].root, dsh.canonicalize().unwrap());
        set_at(
            &store,
            "codex_usage",
            true,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        assert!(store.grants().unwrap().iter().any(|grant| {
            grant.pattern == "sessions/**/*.jsonl" && grant.root == codex.canonicalize().unwrap()
        }));
        set_at(
            &store,
            "deepseek_balance",
            true,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        assert_eq!(
            store.account_link_status().unwrap().as_deref(),
            Some("active")
        );
        set_at(
            &store,
            "codex_app_server",
            true,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        assert!(store.settings().unwrap().codex_app_server);
        set_at(
            &store,
            "ds_bot_usage",
            false,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        set_at(
            &store,
            "codex_usage",
            false,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        set_at(
            &store,
            "deepseek_balance",
            false,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        set_at(
            &store,
            "codex_app_server",
            false,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        assert!(store.grants().unwrap().is_empty());
        assert_ne!(
            store.account_link_status().unwrap().as_deref(),
            Some("active")
        );
        assert!(!store.settings().unwrap().codex_app_server);
        std::fs::create_dir_all(dir.join("cursor")).unwrap();
        for kind in [
            "codex_conversation",
            "ds_bot_conversation",
            "harness_usage",
            "harness_conversation",
            "cursor_conversation",
        ] {
            set_at(&store, kind, true, &dsh, &codex, &dir.join("cursor")).unwrap();
        }
        let patterns: Vec<(wanxiang_core::ToolId, wanxiang_core::Capability, String)> = store
            .grants()
            .unwrap()
            .into_iter()
            .map(|grant| (grant.tool, grant.capability, grant.pattern))
            .collect();
        assert_eq!(patterns.len(), 6, "{patterns:?}");
        assert!(patterns.contains(&(
            wanxiang_core::ToolId::Cursor,
            wanxiang_core::Capability::Conversation,
            wanxiang_adapter_cursor::STATE_PATTERN.to_string()
        )));
        assert!(patterns.contains(&(
            wanxiang_core::ToolId::DsBot,
            wanxiang_core::Capability::Conversation,
            wanxiang_adapter_dsh::BOT_SESSION_PATTERN.to_string()
        )));
        assert!(patterns.contains(&(
            wanxiang_core::ToolId::DeepSeekHarness,
            wanxiang_core::Capability::Usage,
            wanxiang_adapter_dsh::HARNESS_PATTERN.to_string()
        )));
        for kind in [
            "codex_conversation",
            "ds_bot_conversation",
            "harness_usage",
            "harness_conversation",
            "cursor_conversation",
        ] {
            set_at(&store, kind, false, &dsh, &codex, &dir.join("cursor")).unwrap();
        }
        assert!(store.grants().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn harness_cleanup_is_four_direct_child_patterns() {
        let dir = temp("clean");
        let store = open_store(&dir);
        let dsh = dir.join("dsh");
        let codex = dir.join("codex");
        std::fs::create_dir_all(dsh.join("cache/job")).unwrap();
        std::fs::write(dsh.join("cache/item"), b"ok").unwrap();
        std::fs::write(dsh.join("cache/job/nested"), b"deep").unwrap();
        std::fs::create_dir_all(dsh.join("bot")).unwrap();
        std::fs::write(dsh.join("bot/note"), b"no").unwrap();
        set_at(
            &store,
            "harness_cleanup",
            true,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        let patterns: Vec<_> = store
            .grants()
            .unwrap()
            .into_iter()
            .map(|grant| grant.pattern)
            .collect();
        assert_eq!(
            patterns,
            wanxiang_core::CLEANUP_PATTERNS
                .iter()
                .map(|pattern| (*pattern).to_string())
                .collect::<Vec<_>>()
        );
        let nested = store
            .guard()
            .metadata(
                wanxiang_core::ToolId::DeepSeekHarness,
                wanxiang_core::Capability::Cleanup,
                &dsh.join("cache/job/nested").canonicalize().unwrap(),
            )
            .unwrap_err();
        let protected = store
            .guard()
            .metadata(
                wanxiang_core::ToolId::DeepSeekHarness,
                wanxiang_core::Capability::Cleanup,
                &dsh.join("bot/note").canonicalize().unwrap(),
            )
            .unwrap_err();
        assert!(
            matches!(nested, wanxiang_core::GuardError::PatternMismatch),
            "{nested:?}"
        );
        assert!(
            matches!(protected, wanxiang_core::GuardError::PatternMismatch),
            "{protected:?}"
        );
        store
            .guard()
            .metadata(
                wanxiang_core::ToolId::DeepSeekHarness,
                wanxiang_core::Capability::Cleanup,
                &dsh.join("cache/item").canonicalize().unwrap(),
            )
            .unwrap();
        set_at(
            &store,
            "harness_cleanup",
            false,
            &dsh,
            &codex,
            &dir.join("cursor"),
        )
        .unwrap();
        assert!(store.grants().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn space_fixture_replaces_the_dsh_scan_and_cleanup_roots() {
        let dir = temp("fixture");
        let root = dir.join("fake-dsh");
        std::fs::create_dir_all(root.join("cache")).unwrap();
        let store = open_store(&dir);
        set_at(
            &store,
            "harness_space",
            true,
            &dir.join("dsh"),
            &dir.join("codex"),
            &dir.join("cursor"),
        )
        .unwrap();
        std::env::set_var("WANXIANG_SPACE_FIXTURE", &root);
        let applied = apply_space_fixture(&store).and_then(|_| apply_space_fixture(&store));
        std::env::remove_var("WANXIANG_SPACE_FIXTURE");
        applied.unwrap();
        let grants = store.grants().unwrap();
        let canonical = root.canonicalize().unwrap();
        assert!(grants.iter().all(|grant| grant.root == canonical));
        assert_eq!(
            grants
                .iter()
                .filter(|grant| grant.capability == wanxiang_core::Capability::Cleanup)
                .count(),
            4
        );
        assert_eq!(
            grants
                .iter()
                .filter(|grant| grant.capability == wanxiang_core::Capability::SpaceScan)
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn codex_and_cursor_space_grants_cannot_read_or_clean() {
        let dir = temp("space-only");
        let store = open_store(&dir);
        let dsh = dir.join("dsh");
        let codex = dir.join("codex");
        let cursor = dir.join("cursor");
        std::fs::create_dir_all(&cursor).unwrap();
        std::fs::write(codex.join("note"), b"x").unwrap();
        std::fs::write(cursor.join("item"), b"db").unwrap();
        set_at(&store, "codex_space", true, &dsh, &codex, &cursor).unwrap();
        set_at(&store, "cursor_space", true, &dsh, &codex, &cursor).unwrap();
        let grants = store.grants().unwrap();
        assert!(grants.iter().any(|grant| {
            grant.tool == wanxiang_core::ToolId::Codex
                && grant.capability == wanxiang_core::Capability::SpaceScan
                && grant.pattern == "**"
                && grant.root == codex.canonicalize().unwrap()
        }));
        assert!(grants.iter().any(|grant| {
            grant.tool == wanxiang_core::ToolId::Cursor
                && grant.capability == wanxiang_core::Capability::SpaceScan
                && grant.pattern == "**"
                && grant.root == cursor.canonicalize().unwrap()
        }));
        let guard = store.guard();
        let note = codex.join("note").canonicalize().unwrap();
        let db = cursor.join("item").canonicalize().unwrap();
        assert!(matches!(
            guard.open_read(
                wanxiang_core::ToolId::Codex,
                wanxiang_core::Capability::SpaceScan,
                &note
            ),
            Err(wanxiang_core::GuardError::NotGranted)
        ));
        assert!(matches!(
            guard.open_read(
                wanxiang_core::ToolId::Cursor,
                wanxiang_core::Capability::SpaceScan,
                &db
            ),
            Err(wanxiang_core::GuardError::NotGranted)
        ));
        guard
            .metadata(
                wanxiang_core::ToolId::Codex,
                wanxiang_core::Capability::SpaceScan,
                &note,
            )
            .unwrap();
        guard
            .metadata(
                wanxiang_core::ToolId::Cursor,
                wanxiang_core::Capability::SpaceScan,
                &db,
            )
            .unwrap();
        assert!(matches!(
            guard.issue_confirmation(
                wanxiang_core::ToolId::Codex,
                wanxiang_core::Capability::Cleanup,
                b"plan"
            ),
            Err(wanxiang_core::GuardError::NotGranted)
        ));
        assert!(matches!(
            guard.issue_confirmation(
                wanxiang_core::ToolId::Cursor,
                wanxiang_core::Capability::Cleanup,
                b"plan"
            ),
            Err(wanxiang_core::GuardError::NotGranted)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn space_kinds_grant_the_fixed_roots() {
        let dir = temp("gate");
        let store = open_store(&dir);
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &dir);
        std::fs::create_dir_all(dir.join(".dsh")).unwrap();
        std::fs::create_dir_all(dir.join(".codex")).unwrap();
        std::fs::create_dir_all(cursor_support(&dir)).unwrap();
        set(&store, "harness_space", true).unwrap();
        set(&store, "harness_cleanup", true).unwrap();
        set(&store, "codex_space", true).unwrap();
        set(&store, "cursor_space", true).unwrap();
        let grants = store.grants().unwrap();
        assert!(grants.iter().any(|grant| {
            grant.tool == wanxiang_core::ToolId::Codex
                && grant.root == dir.join(".codex").canonicalize().unwrap()
        }));
        assert!(grants.iter().any(|grant| {
            grant.tool == wanxiang_core::ToolId::Cursor
                && grant.root == cursor_support(&dir).canonicalize().unwrap()
        }));
        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
