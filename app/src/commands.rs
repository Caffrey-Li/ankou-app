use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use wanxiang_core::{
    Capability, Clock, Guard, PathScope, SecretStore, Settings, Store, SystemClock, ToolId,
    ToolScheduler,
};

use crate::discover::{self, ToolSnapshot};

pub struct Hub {
    pub store: Store,
    pub secrets: SecretStore,
}

struct Nop;

impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

impl Hub {
    pub fn open(app: &AppHandle) -> tauri::Result<Self> {
        let dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&dir)?;
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(Guard::new(Arc::new(Nop), Arc::clone(&clock)));
        let store = Store::open(&dir.join("wanxiang.sqlite"), guard, clock).map_err(io_err)?;
        Ok(Self {
            store,
            secrets: SecretStore::new(),
        })
    }
}

fn io_err(err: impl std::fmt::Display) -> tauri::Error {
    tauri::Error::Io(std::io::Error::other(err.to_string()))
}

fn text(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[tauri::command]
fn list_grants(hub: State<'_, Hub>) -> Result<Vec<wanxiang_core::StoredGrant>, String> {
    hub.store.grants().map_err(text)
}

#[tauri::command]
fn grant(
    hub: State<'_, Hub>,
    tool: ToolId,
    capability: Capability,
    root: String,
    pattern: String,
) -> Result<wanxiang_core::StoredGrant, String> {
    hub.store
        .grant(
            tool,
            capability,
            PathScope {
                root: root.into(),
                pattern,
            },
        )
        .map_err(text)
}

#[tauri::command]
fn revoke(hub: State<'_, Hub>, id: i64) -> Result<(), String> {
    hub.store.revoke(id).map_err(text)
}

#[tauri::command]
fn get_settings(hub: State<'_, Hub>) -> Result<Settings, String> {
    hub.store.settings().map_err(text)
}

#[tauri::command]
fn set_settings(hub: State<'_, Hub>, settings: Settings) -> Result<(), String> {
    hub.store.set_settings(&settings).map_err(text)
}

#[tauri::command]
fn save_deepseek_key(hub: State<'_, Hub>, key: String) -> Result<(), String> {
    let result = hub.secrets.save(&key).map_err(text);
    drop(key);
    result
}

#[tauri::command]
fn delete_deepseek_key(hub: State<'_, Hub>) -> Result<(), String> {
    hub.secrets.delete().map_err(text)
}

#[tauri::command]
fn deepseek_key_saved(hub: State<'_, Hub>) -> Result<bool, String> {
    hub.secrets.is_saved().map_err(text)
}

#[tauri::command]
fn clear_history(hub: State<'_, Hub>) -> Result<(), String> {
    hub.store.clear_history().map_err(text)
}

#[tauri::command]
fn discover_tools(hub: State<'_, Hub>) -> Result<Vec<ToolSnapshot>, String> {
    let grants = hub.store.grants().map_err(text)?;
    Ok(discover::snapshot(&discover::production_roots(), &grants))
}

#[tauri::command]
fn complete_setup(hub: State<'_, Hub>) -> Result<(), String> {
    let mut settings = hub.store.settings().map_err(text)?;
    settings.setup_complete = true;
    hub.store.set_settings(&settings).map_err(text)
}

pub fn attach<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        list_grants,
        grant,
        revoke,
        get_settings,
        set_settings,
        save_deepseek_key,
        delete_deepseek_key,
        deepseek_key_saved,
        clear_history,
        discover_tools,
        complete_setup,
    ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn grant_parses_the_ui_tool_name() {
        #[derive(serde::Deserialize)]
        struct GrantArgs {
            tool: wanxiang_core::ToolId,
            capability: wanxiang_core::Capability,
            root: String,
            pattern: String,
        }
        let args: GrantArgs = serde_json::from_str(
            r#"{"tool":"deepseek_harness","capability":"usage","root":"/tmp/wanxiang","pattern":"**"}"#,
        )
        .unwrap();
        assert_eq!(args.tool, wanxiang_core::ToolId::DeepSeekHarness);
        assert_eq!(args.capability, wanxiang_core::Capability::Usage);
        assert_eq!(args.root, "/tmp/wanxiang");
        assert_eq!(args.pattern, "**");
    }

    #[test]
    fn capability_lists_each_command_once() {
        const COMMANDS: &[&str] = &[
            "list-grants",
            "grant",
            "revoke",
            "get-settings",
            "set-settings",
            "save-deepseek-key",
            "delete-deepseek-key",
            "deepseek-key-saved",
            "clear-history",
            "discover-tools",
            "complete-setup",
        ];
        let text = include_str!("../capabilities/default.json");
        for name in COMMANDS {
            let needle = format!("\"allow-{name}\"");
            assert!(text.contains(&needle), "{needle}");
        }
    }
}
