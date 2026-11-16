//! 把设置里的「开机启动」「菜单栏图标」「系统通知」落到系统上。
//! 只在 Rust 端调用插件，网页端没有这些插件的权限。

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;
use wanxiang_core::Settings;

pub fn apply<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> Result<(), String> {
    let launcher = app.autolaunch();
    let enabled = launcher.is_enabled().unwrap_or(false);
    if settings.launch_at_login && !enabled {
        launcher.enable().map_err(|err| format!("开机启动没有打开：{err}"))?;
    } else if !settings.launch_at_login && enabled {
        launcher.disable().map_err(|err| format!("开机启动没有关闭：{err}"))?;
    }
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_visible(settings.menu_bar_icon)
            .map_err(|err| format!("菜单栏图标没有更新：{err}"))?;
    }
    Ok(())
}

/// 只有用户在设置里打开了系统通知才发；标题和正文由界面给出，不含对话内容。
pub fn notify<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    title: &str,
    body: &str,
) -> Result<bool, String> {
    if !settings.notifications {
        return Ok(false);
    }
    let title: String = title.chars().take(60).collect();
    let body: String = body.chars().take(120).collect();
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|err| format!("系统通知没有发出：{err}"))?;
    Ok(true)
}
