//! 交接放入：默认回到原应用开新对话。链接只用 core 里写死的前缀；打开应用只按 bundle id。

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use wanxiang_core::place::{place_bytes, plan, target, Method, Target};
use wanxiang_core::sysdata::SysConfirm;

#[derive(Default)]
pub struct PlaceState {
    confirm: SysConfirm,
}

#[derive(Serialize)]
pub struct Prepared {
    token: String,
    method: Method,
    too_long: bool,
}

#[tauri::command]
pub fn prepare_place(
    place: State<'_, PlaceState>,
    target: String,
    text: String,
    folder: Option<String>,
) -> Result<Prepared, String> {
    let found = target_of(&target)?;
    let planned = plan(found, &text, folder.as_deref());
    let token = place
        .confirm
        .issue(&place_bytes(found, &text, folder.as_deref()))
        .map_err(|err| err.to_string())?;
    Ok(Prepared {
        token,
        method: planned.method,
        too_long: planned.too_long,
    })
}

#[tauri::command]
pub fn place_handoff<R: tauri::Runtime>(
    app: AppHandle<R>,
    place: State<'_, PlaceState>,
    target: String,
    token: String,
    text: String,
    folder: Option<String>,
) -> Result<Method, String> {
    let found = target_of(&target)?;
    if !place.confirm.redeem(&token, &place_bytes(found, &text, folder.as_deref())) {
        return Err("确认已失效".to_string());
    }
    let planned = plan(found, &text, folder.as_deref());
    if let Some(url) = planned.url.as_deref() {
        let fixed = found.prefix.is_some_and(|prefix| url.starts_with(prefix));
        if fixed && app.opener().open_url(url, None::<&str>).is_ok() {
            return Ok(Method::Prefill);
        }
    }
    match found.bundle {
        Some(bundle) if open_app(bundle) => Ok(Method::Open),
        Some(_) => Err("没能打开应用".to_string()),
        None => Ok(Method::Copy),
    }
}

fn target_of(id: &str) -> Result<&'static Target, String> {
    target(id).ok_or_else(|| "没有这个目标".to_string())
}

#[cfg(target_os = "macos")]
fn open_app(bundle: &str) -> bool {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    objc2::rc::autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        match workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle)) {
            Some(url) => workspace.openURL(&url),
            None => false,
        }
    })
}

#[cfg(target_os = "windows")]
fn open_app(bundle: &str) -> bool {
    use std::path::PathBuf;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
    let candidates: Vec<PathBuf> = match bundle {
        "com.todesktop.230313mzl4w4u92" => local
            .iter()
            .map(|d| d.join("Programs").join("cursor").join("Cursor.exe"))
            .collect(),
        "com.anthropic.claudefordesktop" => local
            .iter()
            .map(|d| d.join("AnthropicClaude").join("Claude.exe"))
            .collect(),
        "com.deepseek.dsh" => local
            .iter()
            .map(|d| d.join("Programs").join("DeepSeek Harness").join("DeepSeek Harness.exe"))
            .collect(),
        "com.google.antigravity" => local
            .iter()
            .map(|d| d.join("Programs").join("Antigravity").join("Antigravity.exe"))
            .collect(),
        "com.tencent.workbuddy.mac" => local
            .iter()
            .map(|d| d.join("Programs").join("WorkBuddy").join("WorkBuddy.exe"))
            .chain(
                program_files
                    .iter()
                    .map(|d| d.join("WorkBuddy").join("WorkBuddy.exe")),
            )
            .collect(),
        _ => Vec::new(),
    };
    for exe in candidates {
        if exe.is_file() && std::process::Command::new(&exe).spawn().is_ok() {
            return true;
        }
    }
    false
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_app(bundle: &str) -> bool {
    let _ = bundle;
    false
}
