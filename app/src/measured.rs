use std::path::Path;

use wanxiang_core::{Capability, PathScope, Store, ToolId};

const BOT_PATTERN: &str = "bot/usage.sqlite*";
const CODEX_PATTERN: &str = "sessions/**/*.jsonl";

pub fn set(store: &Store, kind: &str, on: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "找不到本机目录".to_string())?;
    let home = std::path::PathBuf::from(home);
    if kind == "gemini_space" {
        let root = crate::discover::gemini_support(&home);
        return set_path(store, ToolId::Gemini, Capability::SpaceScan, &root, "**", on);
    }
    if kind.starts_with("antigravity_") {
        let roots = wanxiang_adapter_antigravity::data_roots(&home);
        return set_antigravity(
            store,
            kind,
            on,
            &roots,
            &wanxiang_adapter_antigravity::space_root(&home),
        );
    }
    if kind == "muse_space" {
        let root = crate::discover::muse_support(&home);
        return set_path(store, ToolId::Muse, Capability::SpaceScan, &root, "**", on);
    }
    if kind.starts_with("grok_") {
        return set_grok(store, kind, on, &wanxiang_adapter_grokbot::support_root(&home));
    }
    if kind.starts_with("codebuddy_") {
        return set_codebuddy(store, kind, on, &wanxiang_adapter_workbuddy::codebuddy_root(&home));
    }
    if kind.starts_with("cursor_cli_") {
        return set_cursor_cli(store, kind, on, &wanxiang_adapter_cursor::cli_root(&home));
    }
    if kind.starts_with("workbuddy_") {
        return set_workbuddy(store, kind, on, &wanxiang_adapter_workbuddy::config_root(&home));
    }
    if kind.starts_with("claude_") {
        return set_claude(
            store,
            kind,
            on,
            &wanxiang_adapter_claude::config_root(&home),
            &wanxiang_adapter_claude::support_root(&home),
        );
    }
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
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return std::path::PathBuf::from(appdata).join("Cursor");
        }
        return home.join("AppData").join("Roaming").join("Cursor");
    }
    #[cfg(not(target_os = "windows"))]
    {
        home.join("Library")
            .join(format!("{} {}", "Application", "Support"))
            .join("Cursor")
    }
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
        "cursor_cleanup" => {
            for pattern in wanxiang_core::CURSOR_CLEANUP_PATTERNS {
                set_path(store, ToolId::Cursor, Capability::Cleanup, cursor, pattern, on)?;
            }
            Ok(())
        }
        "codex_cleanup" => {
            for pattern in wanxiang_core::CODEX_CLEANUP_PATTERNS {
                set_path(store, ToolId::Codex, Capability::Cleanup, codex, pattern, on)?;
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

/// Claude 的授权：会话记录在 `~/.claude`，额度历史与会话标题在桌面应用目录。
/// 桌面应用没装时只授权 `~/.claude` 那一部分。
pub fn set_claude(
    store: &Store,
    kind: &str,
    on: bool,
    config: &Path,
    support: &Path,
) -> Result<(), String> {
    use wanxiang_adapter_claude::{DESKTOP_SESSIONS_PATTERN, PLAN_PATTERN, PROJECTS_PATTERN};
    let desktop = !on || support.is_dir();
    match kind {
        "claude_usage" => {
            set_path(store, ToolId::Claude, Capability::Usage, config, PROJECTS_PATTERN, on)?;
            if desktop {
                set_path(store, ToolId::Claude, Capability::Usage, support, PLAN_PATTERN, on)?;
            }
            Ok(())
        }
        "claude_conversation" => {
            set_path(
                store,
                ToolId::Claude,
                Capability::Conversation,
                config,
                PROJECTS_PATTERN,
                on,
            )?;
            if desktop {
                set_path(
                    store,
                    ToolId::Claude,
                    Capability::Conversation,
                    support,
                    DESKTOP_SESSIONS_PATTERN,
                    on,
                )?;
            }
            Ok(())
        }
        "claude_cleanup" => {
            // 只清理桌面应用目录里的缓存；没装桌面应用就没有可清理的
            if !support.is_dir() {
                return Err("没有找到 Claude 桌面应用".to_string());
            }
            for pattern in wanxiang_core::CLAUDE_CLEANUP_PATTERNS {
                set_path(store, ToolId::Claude, Capability::Cleanup, support, pattern, on)?;
            }
            Ok(())
        }
        "claude_space" => {
            // 空间只看一个目录：桌面应用目录占大头；没装桌面应用时看 ~/.claude
            let root = if support.is_dir() { support } else { config };
            set_path(store, ToolId::Claude, Capability::SpaceScan, root, "**", on)
        }
        _ => Err("没有这项授权".to_string()),
    }
}

/// Antigravity：桌面应用与命令行版各有一个数据目录，两处都授权。
/// 用量只读步骤库；对话读列表库与步骤库（上下文）；空间看桌面应用的数据目录。
pub fn set_antigravity(
    store: &Store,
    kind: &str,
    on: bool,
    roots: &[std::path::PathBuf],
    space: &Path,
) -> Result<(), String> {
    use wanxiang_adapter_antigravity::{CONVERSATIONS_PATTERN, SUMMARY_PATTERN};
    let patterns: Vec<&str> = match kind {
        "antigravity_usage" => vec![CONVERSATIONS_PATTERN],
        "antigravity_conversation" => vec![SUMMARY_PATTERN, CONVERSATIONS_PATTERN],
        "antigravity_space" => {
            return set_path(store, ToolId::Antigravity, Capability::SpaceScan, space, "**", on);
        }
        _ => return Err("没有这项授权".to_string()),
    };
    let capability = if kind == "antigravity_usage" {
        Capability::Usage
    } else {
        Capability::Conversation
    };
    if on && roots.is_empty() {
        return Err("没有找到 Antigravity 的本机记录".to_string());
    }
    for pattern in patterns {
        if on {
            for root in roots {
                set_path(store, ToolId::Antigravity, capability, root, pattern, true)?;
            }
        } else {
            set_path(store, ToolId::Antigravity, capability, space, pattern, false)?;
        }
    }
    Ok(())
}

/// Grok Bot：会话与额度都来自应用缓存目录里的 `*.blob`（代码里只打开三类键）；空间统计应用目录。
pub fn set_grok(store: &Store, kind: &str, on: bool, root: &Path) -> Result<(), String> {
    use wanxiang_adapter_grokbot::PERSIST_PATTERN;
    match kind {
        "grok_usage" => set_path(store, ToolId::GrokBot, Capability::Usage, root, PERSIST_PATTERN, on),
        "grok_conversation" => {
            set_path(store, ToolId::GrokBot, Capability::Conversation, root, PERSIST_PATTERN, on)
        }
        "grok_space" => set_path(store, ToolId::GrokBot, Capability::SpaceScan, root, "**", on),
        _ => Err("没有这项授权".to_string()),
    }
}

/// WorkBuddy：会话记录与会话表都在 `~/.workbuddy`；空间统计同一目录。
pub fn set_workbuddy(store: &Store, kind: &str, on: bool, config: &Path) -> Result<(), String> {
    use wanxiang_adapter_workbuddy::{DB_PATTERN, PROJECTS_PATTERN};
    match kind {
        "workbuddy_conversation" => {
            set_path(
                store,
                ToolId::WorkBuddy,
                Capability::Conversation,
                config,
                PROJECTS_PATTERN,
                on,
            )?;
            set_path(store, ToolId::WorkBuddy, Capability::Conversation, config, DB_PATTERN, on)
        }
        "workbuddy_space" => {
            set_path(store, ToolId::WorkBuddy, Capability::SpaceScan, config, "**", on)
        }
        _ => Err("没有这项授权".to_string()),
    }
}

/// CodeBuddy 命令行：会话记录在 `~/.codebuddy/projects`，与 WorkBuddy 同一格式。
pub fn set_codebuddy(store: &Store, kind: &str, on: bool, config: &Path) -> Result<(), String> {
    match kind {
        "codebuddy_conversation" => set_path(
            store,
            ToolId::CodeBuddy,
            Capability::Conversation,
            config,
            wanxiang_adapter_workbuddy::PROJECTS_PATTERN,
            on,
        ),
        "codebuddy_space" => set_path(store, ToolId::CodeBuddy, Capability::SpaceScan, config, "**", on),
        _ => Err("没有这项授权".to_string()),
    }
}

/// Cursor 命令行：只放行对话目录里的元数据与消息库。
pub fn set_cursor_cli(store: &Store, kind: &str, on: bool, root: &Path) -> Result<(), String> {
    use wanxiang_adapter_cursor::{CLI_META_PATTERN, CLI_STORE_PATTERN};
    match kind {
        "cursor_cli_conversation" => {
            set_path(store, ToolId::CursorCli, Capability::Conversation, root, CLI_META_PATTERN, on)?;
            set_path(store, ToolId::CursorCli, Capability::Conversation, root, CLI_STORE_PATTERN, on)
        }
        "cursor_cli_space" => set_path(store, ToolId::CursorCli, Capability::SpaceScan, root, "**", on),
        _ => Err("没有这项授权".to_string()),
    }
}

fn show(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[cfg(test)]
mod tests {
    /// `fs::canonicalize` 在 Windows 上带 `\\?\` 前缀；守卫存的是去掉前缀的路径。
    fn canon(path: &std::path::Path) -> std::path::PathBuf {
        let real = std::fs::canonicalize(path).unwrap();
        let text = real.to_string_lossy().into_owned();
        match text.strip_prefix(r"\\?\") {
            Some(rest) if !rest.starts_with("UNC\\") => std::path::PathBuf::from(rest),
            _ => real,
        }
    }

    #[test]
    fn codebuddy_and_cursor_cli_grants_use_their_own_tool() {
        let dir = std::env::temp_dir().join(format!("wanxiang-cli-grant-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cb = dir.join("codebuddy");
        let cc = dir.join("cursor");
        std::fs::create_dir_all(&cb).unwrap();
        std::fs::create_dir_all(&cc).unwrap();
        let store = open_store(&dir);
        super::set_codebuddy(&store, "codebuddy_conversation", true, &cb).unwrap();
        super::set_codebuddy(&store, "codebuddy_space", true, &cb).unwrap();
        super::set_cursor_cli(&store, "cursor_cli_conversation", true, &cc).unwrap();
        super::set_cursor_cli(&store, "cursor_cli_space", true, &cc).unwrap();
        let grants = store.grants().unwrap();
        assert_eq!(grants.len(), 5, "{grants:?}");
        assert_eq!(grants.iter().filter(|g| g.tool == wanxiang_core::ToolId::CodeBuddy).count(), 2);
        assert_eq!(grants.iter().filter(|g| g.tool == wanxiang_core::ToolId::CursorCli).count(), 3);
        for kind in ["codebuddy_conversation", "codebuddy_space"] {
            super::set_codebuddy(&store, kind, false, &cb).unwrap();
        }
        for kind in ["cursor_cli_conversation", "cursor_cli_space"] {
            super::set_cursor_cli(&store, kind, false, &cc).unwrap();
        }
        assert!(store.grants().unwrap().is_empty());
        assert!(super::set_cursor_cli(&store, "cursor_cli_usage", true, &cc).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn claude_grants_cover_records_plan_and_titles_only() {
        let dir = std::env::temp_dir().join(format!("wanxiang-claude-grant-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = dir.join("dot-claude");
        let support = dir.join("support");
        std::fs::create_dir_all(&config).unwrap();
        let store = open_store(&dir);
        super::set_claude(&store, "claude_usage", true, &config, &support).unwrap();
        let patterns: Vec<_> = store.grants().unwrap().into_iter().map(|g| g.pattern).collect();
        assert_eq!(patterns, vec![wanxiang_adapter_claude::PROJECTS_PATTERN.to_string()]);
        std::fs::create_dir_all(&support).unwrap();
        super::set_claude(&store, "claude_usage", true, &config, &support).unwrap();
        super::set_claude(&store, "claude_conversation", true, &config, &support).unwrap();
        super::set_claude(&store, "claude_space", true, &config, &support).unwrap();
        let grants = store.grants().unwrap();
        assert_eq!(grants.len(), 5, "{grants:?}");
        assert!(grants.iter().all(|g| g.tool == wanxiang_core::ToolId::Claude));
        assert!(grants.iter().any(|g| g.pattern == wanxiang_adapter_claude::PLAN_PATTERN));
        for kind in ["claude_usage", "claude_conversation", "claude_space"] {
            super::set_claude(&store, kind, false, &config, &support).unwrap();
        }
        assert!(store.grants().unwrap().is_empty());
        assert!(super::set_claude(&store, "claude_other", true, &config, &support).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn antigravity_and_workbuddy_grants_stay_in_their_files() {
        let dir = std::env::temp_dir().join(format!("wanxiang-ag-wb-grant-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = dir.join("antigravity");
        let cli = dir.join("antigravity-cli");
        let wb = dir.join("workbuddy");
        for path in [&app, &cli, &wb] {
            std::fs::create_dir_all(path).unwrap();
        }
        let store = open_store(&dir);
        let roots = vec![app.clone(), cli.clone()];
        super::set_antigravity(&store, "antigravity_usage", true, &roots, &app).unwrap();
        super::set_antigravity(&store, "antigravity_conversation", true, &roots, &app).unwrap();
        super::set_antigravity(&store, "antigravity_space", true, &roots, &app).unwrap();
        super::set_workbuddy(&store, "workbuddy_conversation", true, &wb).unwrap();
        super::set_workbuddy(&store, "workbuddy_space", true, &wb).unwrap();
        let grants = store.grants().unwrap();
        // 用量 2 个目录 × 1，对话 2 × 2，空间 1；WorkBuddy 对话 2，空间 1
        assert_eq!(grants.len(), 10, "{grants:?}");
        assert!(grants
            .iter()
            .filter(|g| g.tool == wanxiang_core::ToolId::Antigravity && g.capability != wanxiang_core::Capability::SpaceScan)
            .all(|g| g.pattern != "**"));
        assert!(super::set_antigravity(&store, "antigravity_usage", true, &[], &app).is_err());
        for kind in ["antigravity_usage", "antigravity_conversation", "antigravity_space"] {
            super::set_antigravity(&store, kind, false, &roots, &app).unwrap();
        }
        for kind in ["workbuddy_conversation", "workbuddy_space"] {
            super::set_workbuddy(&store, kind, false, &wb).unwrap();
        }
        assert!(store.grants().unwrap().is_empty());
        assert!(super::set_workbuddy(&store, "workbuddy_usage", true, &wb).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grok_grants_cover_only_the_cache_files() {
        let dir = std::env::temp_dir().join(format!("wanxiang-grok-grant-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("grok");
        std::fs::create_dir_all(&root).unwrap();
        let store = open_store(&dir);
        for kind in ["grok_usage", "grok_conversation", "grok_space"] {
            super::set_grok(&store, kind, true, &root).unwrap();
        }
        let grants = store.grants().unwrap();
        assert_eq!(grants.len(), 3);
        assert_eq!(
            grants.iter().filter(|g| g.pattern == wanxiang_adapter_grokbot::PERSIST_PATTERN).count(),
            2
        );
        for kind in ["grok_usage", "grok_conversation", "grok_space"] {
            super::set_grok(&store, kind, false, &root).unwrap();
        }
        assert!(store.grants().unwrap().is_empty());
        assert!(super::set_grok(&store, "grok_cleanup", true, &root).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        assert_eq!(store.grants().unwrap()[0].root, canon(&dsh));
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
            grant.pattern == "sessions/**/*.jsonl" && grant.root == canon(&codex)
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
                &canon(&dsh.join("cache/job/nested")),
            )
            .unwrap_err();
        let protected = store
            .guard()
            .metadata(
                wanxiang_core::ToolId::DeepSeekHarness,
                wanxiang_core::Capability::Cleanup,
                &canon(&dsh.join("bot/note")),
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
                &canon(&dsh.join("cache/item")),
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
        let canonical = canon(&root);
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
                && grant.root == canon(&codex)
        }));
        assert!(grants.iter().any(|grant| {
            grant.tool == wanxiang_core::ToolId::Cursor
                && grant.capability == wanxiang_core::Capability::SpaceScan
                && grant.pattern == "**"
                && grant.root == canon(&cursor)
        }));
        let guard = store.guard();
        let note = canon(&codex.join("note"));
        let db = canon(&cursor.join("item"));
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
                && grant.root == canon(&dir.join(".codex"))
        }));
        assert!(grants.iter().any(|grant| {
            grant.tool == wanxiang_core::ToolId::Cursor
                && grant.root == canon(&cursor_support(&dir))
        }));
        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
