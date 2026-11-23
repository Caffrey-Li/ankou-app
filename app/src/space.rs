use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use wanxiang_core::{
    canonical_bytes, cleanup, cleanup_block, scan, ui_entry, Capability, CleanEntry, CleanupReport,
    ConfirmToken, RunningView, Store, ToolId, Trasher,
};

use crate::commands::Hub;
use crate::runtime::{observe, trash_supported, SystemTrash};

#[derive(Clone, Debug, serde::Deserialize)]
pub struct EntryInput {
    rel: String,
    bytes: u64,
    files: u64,
    dev: u64,
    ino: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SpaceView {
    pub protected_bytes: u64,
    pub cleanable_bytes: u64,
    pub unscanned: u32,
    pub cancelled: bool,
    pub quit: Option<String>,
    pub entries: Vec<EntryView>,
    pub trash_supported: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntryView {
    pub rel: String,
    pub bytes: u64,
    pub files: u64,
    pub dev: u64,
    pub ino: u64,
    pub modified: u64,
    pub weak: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunView {
    pub blocked: Option<String>,
    pub entry_count: u64,
    pub total_bytes: u64,
    pub items: Vec<ItemView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemView {
    pub name: String,
    pub label: String,
}

#[tauri::command]
pub async fn scan_space(hub: State<'_, Hub>, tool: String) -> Result<SpaceView, String> {
    let store = Arc::clone(&hub.store);
    let flag = hub.scans.begin(tool_id(&tool)?);
    tauri::async_runtime::spawn_blocking(move || scan_tool(&store, &tool, &flag))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn cancel_scan(hub: State<'_, Hub>, tool: String) -> Result<(), String> {
    hub.scans.cancel(tool_id(&tool)?);
    Ok(())
}

#[tauri::command]
pub fn prepare_cleanup(
    hub: State<'_, Hub>,
    tool: String,
    entries: Vec<EntryInput>,
) -> Result<String, String> {
    prepare(&hub.store, &tool, &entries)
}

#[tauri::command]
pub async fn run_cleanup(
    hub: State<'_, Hub>,
    tool: String,
    token: String,
    entries: Vec<EntryInput>,
) -> Result<RunView, String> {
    if !trash_supported() {
        return Ok(unsupported());
    }
    let store = Arc::clone(&hub.store);
    tauri::async_runtime::spawn_blocking(move || {
        let seen = observe();
        let bundles = seen.bundle_ids;
        let names = seen.process_names;
        let executables = seen.harness_executables;
        let view = RunningView {
            bundle_ids: bundles.as_deref(),
            process_names: names.as_deref(),
            harness_executables: executables.as_deref(),
        };
        let mut trash = SystemTrash;
        execute(&store, &tool, &token, &entries, &view, &mut trash)
    })
    .await
    .map_err(|err| err.to_string())?
}

pub fn scan_tool(store: &Store, tool: &str, cancel: &AtomicBool) -> Result<SpaceView, String> {
    let tool = tool_id(tool)?;
    let scanned = scan(store.guard(), tool, cancel).map_err(scan_text)?;
    let seen = observe();
    let bundles = seen.bundle_ids;
    let names = seen.process_names;
    let executables = seen.harness_executables;
    let running = RunningView {
        bundle_ids: bundles.as_deref(),
        process_names: names.as_deref(),
        harness_executables: executables.as_deref(),
    };
    Ok(SpaceView {
        protected_bytes: scanned.protected_bytes,
        cleanable_bytes: scanned.cleanable_bytes,
        unscanned: scanned.unscanned,
        cancelled: scanned.cancelled,
        quit: cleanup_block(tool, &running).map(str::to_string),
        entries: scanned
            .entries
            .into_iter()
            .map(|entry| {
                let rel = entry.rel.to_string_lossy().to_string();
                let weak = rel.starts_with("storages/session_projcache/");
                EntryView {
                    rel,
                    bytes: entry.bytes,
                    files: entry.files,
                    dev: entry.dev,
                    ino: entry.ino,
                    modified: entry.modified,
                    weak,
                }
            })
            .collect(),
        trash_supported: trash_supported(),
    })
}

pub fn prepare(store: &Store, tool: &str, entries: &[EntryInput]) -> Result<String, String> {
    let tool = tool_id(tool)?;
    let entries = checked(entries)?;
    let bytes = canonical_bytes(tool, &entries);
    store
        .guard()
        .issue_confirmation(tool, Capability::Cleanup, &bytes)
        .map(|token| token.to_hex())
        .map_err(|err| err.to_string())
}

pub fn execute(
    store: &Store,
    tool: &str,
    token: &str,
    entries: &[EntryInput],
    running: &RunningView<'_>,
    trash: &mut dyn Trasher,
) -> Result<RunView, String> {
    let tool = tool_id(tool)?;
    let entries = checked(entries)?;
    let token = ConfirmToken::from_hex(token).ok_or_else(|| "确认已失效".to_string())?;
    let report = cleanup(store.guard(), tool, &entries, &token, running, trash)
        .map_err(|err| err.to_string())?;
    if report.blocked.is_none() {
        store
            .record_cleanup(tool, report.record.entry_count, report.record.total_bytes)
            .map_err(|err| err.to_string())?;
    }
    Ok(view_of(report))
}

fn checked(entries: &[EntryInput]) -> Result<Vec<CleanEntry>, String> {
    entries
        .iter()
        .map(|entry| {
            ui_entry(&entry.rel, entry.bytes, entry.files, entry.dev, entry.ino)
                .ok_or_else(|| "只能使用相对路径".to_string())
        })
        .collect()
}

fn view_of(report: CleanupReport) -> RunView {
    RunView {
        blocked: report.blocked.map(str::to_string),
        entry_count: report.record.entry_count,
        total_bytes: report.record.total_bytes,
        items: report
            .items
            .into_iter()
            .map(|item| ItemView {
                name: display_name(&item.rel),
                label: item.label.to_string(),
            })
            .collect(),
    }
}

fn unsupported() -> RunView {
    RunView {
        blocked: Some("Windows 暂不支持".to_string()),
        entry_count: 0,
        total_bytes: 0,
        items: Vec::new(),
    }
}

fn display_name(rel: &str) -> String {
    if rel.starts_with("cache/") {
        "运行缓存".to_string()
    } else if rel.starts_with("logs/") {
        "日志".to_string()
    } else if rel.starts_with("attachments/v1/tmp/") {
        "附件临时文件".to_string()
    } else if rel.starts_with("storages/session_projcache/") {
        "会话项目缓存".to_string()
    } else {
        rel.rsplit(['/', '\\']).next().unwrap_or(rel).to_string()
    }
}

fn tool_id(name: &str) -> Result<ToolId, String> {
    match name {
        "deepseek_harness" => Ok(ToolId::DeepSeekHarness),
        "ds_bot" => Ok(ToolId::DsBot),
        "cursor" => Ok(ToolId::Cursor),
        "codex" => Ok(ToolId::Codex),
        "claude" => Ok(ToolId::Claude),
        _ => Err("没有这项工具".to_string()),
    }
}

fn scan_text(err: wanxiang_core::SpaceError) -> String {
    match err {
        wanxiang_core::SpaceError::NotGranted => "还没有空间扫描授权".to_string(),
        wanxiang_core::SpaceError::NeedsReauth => wanxiang_core::LABEL_REAUTH.to_string(),
        wanxiang_core::SpaceError::Io(_) => "扫描没有完成".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use wanxiang_core::{
        Clock, Guard, RefreshGate, RunningView, Store, Supervisor, SystemClock, ToolId,
        ToolScheduler, TrashFault, Trasher,
    };

    use crate::commands::ScanSlots;

    use super::{execute, prepare, scan_tool, EntryInput};

    struct MemTrash {
        paths: Vec<PathBuf>,
    }

    impl Trasher for MemTrash {
        fn trash(&mut self, path: &Path) -> Result<(), TrashFault> {
            self.paths.push(path.to_path_buf());
            Ok(())
        }
    }

    fn open(dir: &Path) -> Store {
        let supervisor = Supervisor::new();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(Guard::new(
            Arc::clone(&gate) as Arc<dyn ToolScheduler>,
            Arc::clone(&clock),
        ));
        Store::open(&dir.join("wanxiang.sqlite"), guard, clock).unwrap()
    }

    fn idle() -> (Vec<String>, Vec<String>, Vec<String>) {
        (
            Vec::new(),
            Vec::new(),
            vec!["DeepSeek Harness".to_string(), "dsh".to_string()],
        )
    }

    #[test]
    fn cancel_scan_sets_the_flag_a_running_scan_reads() {
        let slots = ScanSlots::default();
        let flag = slots.begin(ToolId::DeepSeekHarness);
        assert!(!flag.load(Ordering::SeqCst));
        slots.cancel(ToolId::DeepSeekHarness);
        assert!(flag.load(Ordering::SeqCst));
        let again = slots.begin(ToolId::DeepSeekHarness);
        assert!(!again.load(Ordering::SeqCst));
        assert!(Arc::ptr_eq(&flag, &again));
    }

    #[test]
    fn commands_scan_prepare_and_record_without_paths() {
        let dir = std::env::temp_dir().join(format!("wanxiang-m4b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dsh = dir.join("dsh");
        std::fs::create_dir_all(dsh.join("cache")).unwrap();
        std::fs::write(dsh.join("cache/item"), b"abcd").unwrap();
        let store = open(&dir);
        crate::measured::set_at(
            &store,
            "harness_space",
            true,
            &dsh,
            &dir.join("codex"),
            &dir.join("cursor"),
        )
        .unwrap();
        crate::measured::set_at(
            &store,
            "harness_cleanup",
            true,
            &dsh,
            &dir.join("codex"),
            &dir.join("cursor"),
        )
        .unwrap();
        let view = scan_tool(&store, "deepseek_harness", &AtomicBool::new(false)).unwrap();
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].rel, "cache/item");
        let absolute = EntryInput {
            rel: dsh.join("cache/item").to_string_lossy().to_string(),
            bytes: view.entries[0].bytes,
            files: 1,
            dev: view.entries[0].dev,
            ino: view.entries[0].ino,
        };
        assert!(prepare(&store, "deepseek_harness", &[absolute]).is_err());
        let input = vec![EntryInput {
            rel: view.entries[0].rel.clone(),
            bytes: view.entries[0].bytes,
            files: view.entries[0].files,
            dev: view.entries[0].dev,
            ino: view.entries[0].ino,
        }];
        let token = prepare(&store, "deepseek_harness", &input).unwrap();
        let (bundles, names, executables) = idle();
        let running = RunningView {
            bundle_ids: Some(&bundles),
            process_names: Some(&names),
            harness_executables: Some(&executables),
        };
        let mut trash = MemTrash { paths: Vec::new() };
        let report = execute(
            &store,
            "deepseek_harness",
            &token,
            &input,
            &running,
            &mut trash,
        )
        .unwrap();
        assert!(report.blocked.is_none());
        assert_eq!(report.entry_count, 1);
        assert_eq!(trash.paths.len(), 1);
        assert_eq!(
            store.cleanup_history().unwrap(),
            vec![("deepseek_harness".to_string(), 1, 4)]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
