use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use wanxiang_core::{
    month_start, present_usage, shift_days, Capability, Clock, GrantStatus, Guard, RecordAccess,
    RefreshGate, SecretStore, Settings, Store, SystemClock, ToolId, UsageFacts, UsagePage,
};

use crate::discover::{self, ToolSnapshot};

pub struct Hub {
    pub store: Arc<Store>,
    pub secrets: SecretStore,
    pub gate: Arc<RefreshGate>,
    pub emit: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Hub {
    pub fn open(app: &AppHandle) -> tauri::Result<Self> {
        let dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&dir)?;
        let supervisor = app.state::<wanxiang_core::Supervisor>();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(Guard::new(Arc::clone(&gate) as Arc<dyn wanxiang_core::ToolScheduler>, Arc::clone(&clock)));
        let store = Arc::new(Store::open(&dir.join("wanxiang.sqlite"), guard, clock).map_err(io_err)?);
        let emit_app = app.clone();
        let emit: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |part: &str| {
            let _ = emit_app.emit("usage-item", part.to_string());
        });
        crate::collect::spawn_refresh(Arc::clone(&store), Arc::clone(&gate), Arc::clone(&emit));
        Ok(Self {
            store,
            secrets: SecretStore::new(),
            gate,
            emit,
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
    let mut grants = hub.store.grants().map_err(text)?;
    if hub.store.account_link_status().map_err(text)?.as_deref() == Some("active") {
        for tool in [ToolId::DeepSeekHarness, ToolId::DsBot] {
            grants.push(wanxiang_core::StoredGrant {
                id: 0,
                tool,
                capability: Capability::AccountLink,
                root: PathBuf::new(),
                pattern: String::new(),
                status: GrantStatus::Active,
            });
        }
    }
    Ok(grants)
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

#[tauri::command]
fn get_usage(
    hub: State<'_, Hub>,
    range: Option<String>,
    start: Option<String>,
    end: Option<String>,
) -> Result<UsagePage, String> {
    usage_page(&hub, range.as_deref(), start.as_deref(), end.as_deref()).map_err(text)
}

#[tauri::command]
fn refresh_usage(hub: State<'_, Hub>) -> Result<bool, String> {
    Ok(crate::collect::start_manual(
        Arc::clone(&hub.store),
        Arc::clone(&hub.gate),
        Arc::clone(&hub.emit),
    ))
}

#[tauri::command]
fn set_capability(hub: State<'_, Hub>, kind: String, on: bool) -> Result<(), String> {
    crate::measured::set(&hub.store, &kind, on)
}

fn usage_page(
    hub: &Hub,
    range: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<UsagePage, StoreErrorText> {
    let settings = hub.store.settings().map_err(StoreErrorText)?;
    let today = hub.store.local_today();
    let (range_start, range_end) = match range.unwrap_or("month") {
        "today" => (today.clone(), today.clone()),
        "7" => (shift_days(&today, -6), today.clone()),
        "30" => (shift_days(&today, -29), today.clone()),
        "custom" if valid_day(start) && valid_day(end) && start.unwrap() <= end.unwrap() => {
            (start.unwrap().to_string(), end.unwrap().to_string())
        }
        _ => (month_start(&today), today.clone()),
    };
    Ok(present_usage(&UsageFacts {
        now: hub.store.now_unix(),
        offset_secs: hub.store.offset_secs(),
        today,
        range_start,
        range_end,
        rows: hub.store.usage_rows().map_err(StoreErrorText)?,
        reports: hub.store.usage_reports().map_err(StoreErrorText)?,
        key_saved: hub.secrets.is_saved().unwrap_or(false),
        account_linked: hub.store.guard().account_link_active(ToolId::DeepSeekHarness),
        codex_app_server: settings.codex_app_server,
        codex_access: record_access(&hub.store.grants().map_err(StoreErrorText)?, ToolId::Codex),
        bot_access: record_access(&hub.store.grants().map_err(StoreErrorText)?, ToolId::DsBot),
    }))
}

fn record_access(grants: &[wanxiang_core::StoredGrant], tool: ToolId) -> RecordAccess {
    let mut authorized = false;
    let mut reauth = false;
    for grant in grants {
        if grant.tool == tool && grant.capability == Capability::Usage {
            if grant.status == GrantStatus::Active {
                authorized = true;
            } else if grant.status == GrantStatus::NeedsReauth {
                reauth = true;
            }
        }
    }
    if authorized {
        RecordAccess::Authorized
    } else if reauth {
        RecordAccess::NeedsReauth
    } else {
        RecordAccess::Unauthorized
    }
}

struct StoreErrorText(wanxiang_core::StoreError);

impl std::fmt::Display for StoreErrorText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn valid_day(day: Option<&str>) -> bool {
    day.is_some_and(|day| {
        day.len() == 10
            && day.as_bytes()[4] == b'-'
            && day.as_bytes()[7] == b'-'
            && day.bytes().filter(|byte| byte.is_ascii_digit()).count() == 8
    })
}

pub fn attach<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        list_grants,
        revoke,
        get_settings,
        set_settings,
        save_deepseek_key,
        delete_deepseek_key,
        deepseek_key_saved,
        clear_history,
        discover_tools,
        complete_setup,
        get_usage,
        refresh_usage,
        set_capability,
        crate::handoff::list_conversations,
        crate::handoff::prepare_handoff,
        crate::handoff::begin_summary,
        crate::handoff::send_summary,
        crate::handoff::prepare_deeplink,
        crate::handoff::place_deeplink,
        crate::handoff::export_handoff,
        crate::handoff::organize_handoff,
        tool_icons,
    ])
}

#[tauri::command]
fn tool_icons<R: tauri::Runtime>(app: AppHandle<R>) -> Result<Vec<crate::icons::ToolIcon>, String> {
    let cache = app.path().app_data_dir().map_err(text)?.join("icon-cache");
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = tx.send(crate::icons::tool_icons(&cache));
        })
        .map_err(text)?;
        return rx.recv().map_err(text);
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(crate::icons::tool_icons(&cache))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn grant_parses_the_ui_tool_name() {
        #[derive(serde::Deserialize)]
        struct SetCapabilityArgs {
            kind: String,
            on: bool,
        }
        let args: SetCapabilityArgs =
            serde_json::from_str(r#"{"kind":"ds_bot_usage","on":true}"#).unwrap();
        assert_eq!(args.kind, "ds_bot_usage");
        assert!(args.on);
        let off: SetCapabilityArgs =
            serde_json::from_str(r#"{"kind":"codex_usage","on":false}"#).unwrap();
        assert_eq!(off.kind, "codex_usage");
        assert!(!off.on);
    }

    #[test]
    fn capability_lists_each_command_once() {
        const COMMANDS: &[&str] = &[
            "list-grants",
            "revoke",
            "get-settings",
            "set-settings",
            "save-deepseek-key",
            "delete-deepseek-key",
            "deepseek-key-saved",
            "clear-history",
            "discover-tools",
            "complete-setup",
            "get-usage",
            "refresh-usage",
            "set-capability",
            "list-conversations",
            "prepare-handoff",
            "begin-summary",
            "send-summary",
            "prepare-deeplink",
            "place-deeplink",
            "export-handoff",
            "organize-handoff",
            "tool-icons",
        ];
        let text = include_str!("../capabilities/default.json");
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let permissions = value["permissions"].as_array().unwrap();
        let allows: Vec<&str> = permissions.iter().filter_map(|item| item.as_str().filter(|name| name.starts_with("allow-"))).collect();
        assert_eq!(allows.len(), COMMANDS.len());
        for name in COMMANDS {
            let needle = format!("allow-{name}");
            assert_eq!(allows.iter().filter(|item| **item == needle).count(), 1, "{needle}");
        }
        assert!(!allows.contains(&"allow-grant"));
        assert!(!text.contains("open-deeplink"));
        assert!(!text.contains("allow-open-path"));
        assert!(!text.contains("allow-default-urls"));
        assert!(!text.contains("opener:default"));
    }

    #[test]
    fn capability_has_no_opener_dialog_or_plugin_defaults() {
        let text = include_str!("../capabilities/default.json");
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let permissions = value["permissions"].as_array().unwrap();
        assert!(!permissions.is_empty());
        for item in permissions {
            let name = item
                .as_str()
                .or_else(|| item.get("identifier").and_then(|id| id.as_str()))
                .unwrap_or("");
            assert!(!name.starts_with("opener:"), "{name}");
            assert!(!name.starts_with("dialog:"), "{name}");
            assert!(!name.split(':').any(|part| part == "default"), "{name}");
        }
    }

    #[test]
    fn capability_core_permissions_are_listen_and_drag() {
        let text = include_str!("../capabilities/default.json");
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let core: Vec<&str> = value["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.as_str().filter(|name| name.starts_with("core:")))
            .collect();
        assert_eq!(
            core,
            [
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "core:window:allow-start-dragging",
            ]
        );
    }

    #[test]
    fn tool_icons_takes_no_caller_path() {
        let source = include_str!("commands.rs");
        let start = source.find("fn tool_icons").expect("tool_icons");
        let end = source[start..].find('{').unwrap() + start;
        let signature = &source[start..end];
        assert!(signature.contains("AppHandle"), "{signature}");
        assert!(!signature.contains("path"), "{signature}");
    }
}
