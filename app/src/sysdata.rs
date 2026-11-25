//! 清理页「系统」部分与 AI 本地模型：命令入口。只统计大小；清理只移到废纸篓。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::State;
use wanxiang_core::sysdata::{
    checked_picks, cleanup, count_snapshots, parse_df, pick_bytes, scan_all, Pick, Probe,
    SysCleanup, SysConfirm, SysScan,
};

use crate::commands::Hub;
use crate::runtime::{observe, trash_supported, SystemTrash};

#[derive(Default)]
pub struct SysState {
    cancel: Arc<AtomicBool>,
    confirm: SysConfirm,
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| "找不到本机目录".to_string())
}

fn enabled(hub: &Hub) -> Result<(), String> {
    let settings = hub.store.settings().map_err(|err| err.to_string())?;
    if settings.system_scan {
        Ok(())
    } else {
        Err("还没有开启系统空间统计".to_string())
    }
}

#[tauri::command]
pub async fn scan_system(hub: State<'_, Hub>, sys: State<'_, SysState>) -> Result<SysScan, String> {
    enabled(&hub)?;
    let home = home()?;
    let cancel = Arc::clone(&sys.cancel);
    cancel.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        let has_app = |name: &str| {
            #[cfg(target_os = "macos")]
            {
                Path::new("/Applications").join(name).exists()
            }
            #[cfg(target_os = "windows")]
            {
                if name == "Docker.app" {
                    Path::new(r"C:\Program Files\Docker\Docker\Docker Desktop.exe").exists()
                } else {
                    false
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = name;
                false
            }
        };
        let snapshots = || {
            #[cfg(target_os = "macos")]
            {
                Command::new("/usr/bin/tmutil")
                    .args(["listlocalsnapshots", "/"])
                    .output()
                    .ok()
                    .filter(|out| out.status.success())
                    .map(|out| count_snapshots(&String::from_utf8_lossy(&out.stdout)))
            }
            #[cfg(not(target_os = "macos"))]
            {
                None
            }
        };
        let probe = Probe {
            home: &home,
            has_app: &has_app,
            snapshots: &snapshots,
        };
        let mut scan = scan_all(&probe, &cancel);
        scan.disk = disk_usage(&home);
        scan
    })
    .await
    .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
fn disk_usage(_home: &Path) -> Option<wanxiang_core::sysdata::Disk> {
    Command::new("/bin/df")
        .args(["-k", "/"])
        .output()
        .ok()
        .and_then(|out| parse_df(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(target_os = "windows")]
fn disk_usage(home: &Path) -> Option<wanxiang_core::sysdata::Disk> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let mut wide: Vec<u16> = home.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free)
    };
    (ok != 0 && total > 0).then_some(wanxiang_core::sysdata::Disk { total, available })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn disk_usage(_home: &Path) -> Option<wanxiang_core::sysdata::Disk> {
    None
}

#[tauri::command]
pub fn cancel_system_scan(sys: State<'_, SysState>) {
    sys.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn prepare_system_cleanup(
    hub: State<'_, Hub>,
    sys: State<'_, SysState>,
    picks: Vec<Pick>,
) -> Result<String, String> {
    enabled(&hub)?;
    checked_picks(&picks).map_err(str::to_string)?;
    sys.confirm
        .issue(&pick_bytes(&picks))
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn run_system_cleanup(
    hub: State<'_, Hub>,
    sys: State<'_, SysState>,
    token: String,
    picks: Vec<Pick>,
) -> Result<SysCleanup, String> {
    enabled(&hub)?;
    if !trash_supported() {
        return Err("这台电脑暂不支持".to_string());
    }
    let checked = checked_picks(&picks).map_err(str::to_string)?;
    if !sys.confirm.redeem(&token, &pick_bytes(&picks)) {
        return Err("确认已失效，请重新选择".to_string());
    }
    let home = home()?;
    tauri::async_runtime::spawn_blocking(move || {
        let running = observe().bundle_ids;
        let mut trash = SystemTrash;
        cleanup(&home, &checked, running.as_deref(), &mut trash)
    })
    .await
    .map_err(|err| err.to_string())
}
