//! 版本与更新：启动时和在设置里手动检查；有新版本时在应用内下载、校验签名、替换并重启。
//!
//! 更新清单（latest.json）是 Tauri 更新插件的格式，另外多一个字段 `minimum_version`：
//! 当前版本低于它时是「强制更新」，界面挡住整个应用，只能更新或退出。
//! 检查失败（没网等）不挡，下次再查。

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct UpdateInfo {
    /// 正在运行的版本
    pub current: String,
    /// 查到的最新版本；没有更新时为空
    pub latest: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
    /// 清单里要求的最低版本
    pub minimum: Option<String>,
    /// 当前版本低于最低版本，必须更新
    pub mandatory: bool,
    /// 这次检查失败的原因（不含地址、不含个人信息）
    pub error: Option<String>,
}

fn parse(version: &str) -> Option<semver::Version> {
    semver::Version::parse(version.trim().trim_start_matches('v')).ok()
}

/// 当前版本是否低于清单要求的最低版本。版本号读不懂时不强制。
pub fn below_minimum(current: &str, minimum: Option<&str>) -> bool {
    match (parse(current), minimum.and_then(parse)) {
        (Some(current), Some(minimum)) => current < minimum,
        _ => false,
    }
}

/// 从清单原文里取最低版本。
pub fn minimum_from(raw: &serde_json::Value) -> Option<String> {
    raw.get("minimum_version")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| parse(value).is_some())
        .map(str::to_string)
}

#[tauri::command]
pub async fn check_update<R: tauri::Runtime>(
    app: AppHandle<R>,
    pending: State<'_, PendingUpdate>,
) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    let mut info = UpdateInfo {
        current: current.clone(),
        ..UpdateInfo::default()
    };
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(_) => {
            info.error = Some("更新服务没有配置".to_string());
            return Ok(info);
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            info.minimum = minimum_from(&update.raw_json);
            info.mandatory = below_minimum(&current, info.minimum.as_deref());
            info.latest = Some(update.version.clone());
            info.notes = update.body.clone().filter(|notes| !notes.trim().is_empty());
            info.date = update.date.map(|date| date.to_string());
            *pending.0.lock().map_err(|_| "更新状态出错".to_string())? = Some(update);
        }
        Ok(None) => {
            *pending.0.lock().map_err(|_| "更新状态出错".to_string())? = None;
        }
        Err(error) => {
            info.error = Some(describe(&error.to_string()));
        }
    }
    Ok(info)
}

/// 只说清是哪一类问题，不把地址、路径带到界面上。
fn describe(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("signature") {
        "更新包签名校验失败，已停止".to_string()
    } else if lower.contains("network") || lower.contains("request") || lower.contains("connect") || lower.contains("dns") {
        "连不上更新服务，请检查网络".to_string()
    } else {
        "检查更新失败".to_string()
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
    stage: &'static str,
}

/// 下载、校验签名、替换应用，然后重启。进度通过 `update-progress` 事件发给界面。
#[tauri::command]
pub async fn install_update<R: tauri::Runtime>(
    app: AppHandle<R>,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|_| "更新状态出错".to_string())?
        .take()
        .ok_or_else(|| "没有可安装的更新，请先检查更新".to_string())?;
    let mut downloaded = 0_u64;
    let progress = app.clone();
    let done = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(
                    "update-progress",
                    Progress {
                        downloaded,
                        total,
                        stage: "download",
                    },
                );
            },
            move || {
                let _ = done.emit(
                    "update-progress",
                    Progress {
                        downloaded: 0,
                        total: None,
                        stage: "install",
                    },
                );
            },
        )
        .await
        .map_err(|error| describe(&error.to_string()).replace("检查更新失败", "安装更新失败"))?;
    if let Some(supervisor) = app.try_state::<wanxiang_core::Supervisor>() {
        supervisor.shutdown();
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_version_decides_mandatory() {
        assert!(below_minimum("0.1.0", Some("0.2.0")));
        assert!(below_minimum("0.1.9", Some("v0.2.0")));
        assert!(!below_minimum("0.2.0", Some("0.2.0")));
        assert!(!below_minimum("0.3.0", Some("0.2.0")));
        assert!(!below_minimum("0.1.0", None));
        assert!(!below_minimum("0.1.0", Some("不是版本号")));
    }

    #[test]
    fn minimum_is_read_from_manifest() {
        let raw = serde_json::json!({ "version": "0.2.0", "minimum_version": " 0.2.0 " });
        assert_eq!(minimum_from(&raw).as_deref(), Some("0.2.0"));
        assert_eq!(minimum_from(&serde_json::json!({ "minimum_version": "abc" })), None);
        assert_eq!(minimum_from(&serde_json::json!({})), None);
    }

    #[test]
    fn errors_do_not_leak_addresses() {
        let text = describe("error sending request for url (https://example.r2.dev/latest.json)");
        assert_eq!(text, "连不上更新服务，请检查网络");
        assert!(!describe("weird failure at /Users/someone/x").contains("/Users"));
    }
}
