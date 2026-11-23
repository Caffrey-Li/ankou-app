use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use wanxiang_core::{
    month_start, present_usage, shift_days, Capability, Clock, GrantStatus, Guard, RecordAccess,
    RefreshGate, SecretStore, Settings, Store, SystemClock, ToolId, UsageFacts, UsagePage,
};

use crate::discover::{self, ToolSnapshot};

#[derive(Default)]
pub struct ScanSlots {
    flags: Mutex<HashMap<ToolId, Arc<AtomicBool>>>,
}

impl ScanSlots {
    pub fn begin(&self, tool: ToolId) -> Arc<AtomicBool> {
        let mut flags = self.flags.lock().expect("scan");
        let flag = flags
            .entry(tool)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        flag.store(false, Ordering::SeqCst);
        flag
    }

    pub fn cancel(&self, tool: ToolId) {
        let mut flags = self.flags.lock().expect("scan");
        flags
            .entry(tool)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::SeqCst);
    }
}

pub struct Hub {
    pub store: Arc<Store>,
    pub secrets: SecretStore,
    pub scans: ScanSlots,
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
        let guard = Arc::new(Guard::new(
            Arc::clone(&gate) as Arc<dyn wanxiang_core::ToolScheduler>,
            Arc::clone(&clock),
        ));
        let store =
            Arc::new(Store::open(&dir.join("wanxiang.sqlite"), guard, clock).map_err(io_err)?);
        let emit_app = app.clone();
        let emit: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |part: &str| {
            let _ = emit_app.emit("usage-item", part.to_string());
        });
        crate::collect::spawn_refresh(Arc::clone(&store), Arc::clone(&gate), Arc::clone(&emit));
        crate::measured::apply_space_fixture(&store).map_err(io_err)?;
        Ok(Self {
            store,
            secrets: SecretStore::new(),
            scans: ScanSlots::default(),
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
fn set_settings<R: tauri::Runtime>(
    app: AppHandle<R>,
    hub: State<'_, Hub>,
    settings: Settings,
) -> Result<(), String> {
    hub.store.set_settings(&settings).map_err(text)?;
    crate::system::apply(&app, &settings)
}

#[tauri::command]
fn send_notice<R: tauri::Runtime>(
    app: AppHandle<R>,
    hub: State<'_, Hub>,
    title: String,
    body: String,
) -> Result<bool, String> {
    let settings = hub.store.settings().map_err(text)?;
    crate::system::notify(&app, &settings, &title, &body)
}

#[tauri::command]
fn save_deepseek_key(hub: State<'_, Hub>, key: String) -> Result<(), String> {
    let result = hub.secrets.save(&key).map_err(text);
    drop(key);
    result?;
    hub.store.set_key_saved_flag(true).map_err(text)
}

#[tauri::command]
fn delete_deepseek_key(hub: State<'_, Hub>) -> Result<(), String> {
    hub.secrets.delete().map_err(text)?;
    hub.store.set_key_saved_flag(false).map_err(text)
}

/// 先看本地记号；只有从没记过（旧版本保存的 Key）才去钥匙串确认一次，并且不在主线程上等。
#[tauri::command]
async fn deepseek_key_saved(hub: State<'_, Hub>) -> Result<bool, String> {
    if crate::measured::space_fixture_active() {
        return Ok(false);
    }
    if let Some(saved) = hub.store.key_saved_flag().map_err(text)? {
        return Ok(saved);
    }
    let saved = tauri::async_runtime::spawn_blocking(|| SecretStore::new().is_saved())
        .await
        .map_err(text)?
        .map_err(text)?;
    hub.store.set_key_saved_flag(saved).map_err(text)?;
    Ok(saved)
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
        key_saved: if crate::measured::space_fixture_active() {
            false
        } else {
            // 本地记号，不在主线程上读钥匙串（新签名的包会弹系统确认框）
            hub.store.key_saved_flag().ok().flatten().unwrap_or(false)
        },
        account_linked: hub
            .store
            .guard()
            .account_link_active(ToolId::DeepSeekHarness),
        codex_app_server: settings.codex_app_server,
        codex_access: record_access(&hub.store.grants().map_err(StoreErrorText)?, ToolId::Codex),
        bot_access: record_access(&hub.store.grants().map_err(StoreErrorText)?, ToolId::DsBot),
        harness_access: record_access(
            &hub.store.grants().map_err(StoreErrorText)?,
            ToolId::DeepSeekHarness,
        ),
        claude_access: record_access(&hub.store.grants().map_err(StoreErrorText)?, ToolId::Claude),
        antigravity_access: record_access(
            &hub.store.grants().map_err(StoreErrorText)?,
            ToolId::Antigravity,
        ),
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
        crate::space::scan_space,
        crate::space::cancel_scan,
        crate::space::prepare_cleanup,
        crate::space::run_cleanup,
        open_privacy_settings,
        send_notice,
        tray_action,
    ])
}

/// 快速面板上的按钮：收起面板，再打开主窗口到对应页面，或退出。
#[tauri::command]
fn tray_action<R: tauri::Runtime>(app: AppHandle<R>, action: String) -> Result<(), String> {
    if let Some(panel) = app.get_webview_window("tray") {
        let _ = panel.hide();
    }
    match action.as_str() {
        "quit" => {
            if let Some(supervisor) = app.try_state::<wanxiang_core::Supervisor>() {
                supervisor.shutdown();
            }
            app.exit(0);
            Ok(())
        }
        "overview" | "settings" | "handoff" | "space" | "usage" => {
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| "找不到主窗口".to_string())?;
            let _ = window.unminimize();
            window.show().map_err(text)?;
            let _ = window.set_focus();
            window.emit("navigate", action).map_err(text)
        }
        _ => Err("没有这个操作".to_string()),
    }
}

/// 只打开「隐私与安全性 › 完全磁盘访问权限」面板，由用户自己决定是否授予。
#[tauri::command]
fn open_privacy_settings<R: tauri::Runtime>(app: AppHandle<R>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
            None::<&str>,
        )
        .map_err(text)
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
            "scan-space",
            "cancel-scan",
            "prepare-cleanup",
            "run-cleanup",
            "open-privacy-settings",
            "send-notice",
            "tray-action",
        ];
        let text = include_str!("../capabilities/default.json");
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let permissions = value["permissions"].as_array().unwrap();
        let allows: Vec<&str> = permissions
            .iter()
            .filter_map(|item| item.as_str().filter(|name| name.starts_with("allow-")))
            .collect();
        assert_eq!(allows.len(), COMMANDS.len());
        for name in COMMANDS {
            let needle = format!("allow-{name}");
            assert_eq!(
                allows.iter().filter(|item| **item == needle).count(),
                1,
                "{needle}"
            );
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
                "core:window:allow-set-size",
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
