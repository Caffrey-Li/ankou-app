use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::guard::{metadata_denied, node_ids, root_ids};
use crate::{Capability, ConfirmToken, Guard, GuardError, ToolId};

pub const LABEL_CHANGED: &str = "已跳过 · 内容已变化";
pub const LABEL_OUTSIDE: &str = "失败 · 超出授权";
pub const LABEL_PROTECTED: &str = "失败 · 受保护";
pub const LABEL_PERMISSION: &str = "失败 · 权限不足";
pub const LABEL_BUSY: &str = "已跳过 · 正在使用";
pub const LABEL_TRASH: &str = "失败 · 移到废纸篓失败";
pub const LABEL_TRASHED: &str = "已清理";
pub const LABEL_SCAN_CANCEL: &str = "扫描已取消";
pub const LABEL_SCAN_DENIED: &str = "权限不足";
pub const LABEL_REAUTH: &str = "需要重新授权";
pub const LABEL_QUIT_CURSOR: &str = "请先退出 Cursor";
pub const LABEL_QUIT_CODEX: &str = "请先退出 ChatGPT";
pub const LABEL_QUIT_CLAUDE: &str = "请先退出 Claude";
pub const LABEL_QUIT_GEMINI: &str = "请先退出 Gemini";
pub const LABEL_QUIT_ANTIGRAVITY: &str = "请先退出 Antigravity";
pub const LABEL_QUIT_WORKBUDDY: &str = "请先退出 WorkBuddy";
pub const LABEL_QUIT_GROK_BOT: &str = "请先退出 Grok Bot";
pub const LABEL_QUIT_MUSE: &str = "请先退出 Muse";
pub const LABEL_QUIT_CODEBUDDY: &str = "请先退出 CodeBuddy";
pub const LABEL_QUIT_HARNESS: &str = "请先退出 DeepSeek Harness";
pub const LABEL_QUIT_DSH_PROCESS: &str = "请先退出 DeepSeek Harness / DS Bot";
pub const LABEL_UNKNOWN_APP: &str = "无法确认应用是否在运行，已停止清理";
pub const LABEL_UNKNOWN_DSH: &str = "无法确认 DeepSeek Harness / DS Bot 是否在运行，已停止清理";

pub const CLEANUP_PATTERNS: &[&str] = &[
    "cache/*",
    "logs/*",
    "attachments/v1/tmp/*",
    "storages/session_projcache/*",
];

/// Claude 桌面应用目录里浏览器内核的缓存：退出 Claude 后删掉会自动重建。
/// 不含 vm_bundles（电脑任务的运行环境，删掉要重新下载十几 GB）和任何会话、设置。
pub const CLAUDE_CLEANUP_PATTERNS: &[&str] = &[
    "Cache/*",
    "Code Cache/*",
    "GPUCache/*",
    "GraphiteDawnCache/*",
    "DawnGraphiteCache/*",
    "DawnWebGPUCache/*",
];

/// Cursor 应用目录里的编译缓存、浏览器缓存和日志。不含 snapshots（智能体改动的检查点）和 User（对话、设置）。
pub const CURSOR_CLEANUP_PATTERNS: &[&str] = &[
    "CachedData/*",
    "CachedProfilesData/*",
    "CachedExtensionVSIXs/*",
    "Cache/*",
    "Code Cache/*",
    "GPUCache/*",
    "logs/*",
];

/// ~/.codex 里的缓存目录。会话、归档、生成的图片、数据库都不动。
pub const CODEX_CLEANUP_PATTERNS: &[&str] = &["cache/*"];

/// 每个端可以清理的目录（`目录/*` 形式，清理单位是目录下的每一项）。
pub fn cleanup_patterns(tool: ToolId) -> &'static [&'static str] {
    match tool {
        ToolId::DeepSeekHarness | ToolId::DsBot => CLEANUP_PATTERNS,
        ToolId::Claude => CLAUDE_CLEANUP_PATTERNS,
        ToolId::Cursor => CURSOR_CLEANUP_PATTERNS,
        ToolId::Codex => CODEX_CLEANUP_PATTERNS,
        _ => &[],
    }
}

pub fn ui_entry(rel: &str, bytes: u64, files: u64, dev: u64, ino: u64) -> Option<CleanEntry> {
    let parts: Vec<&str> = rel.split(['/', '\\']).collect();
    let relative = !rel.is_empty()
        && !Path::new(rel).is_absolute()
        && !rel.contains(':')
        && parts.iter().all(|part| !part.is_empty() && *part != "..");
    if !relative {
        return None;
    }
    Some(CleanEntry {
        rel: PathBuf::from(rel),
        bytes,
        files,
        dev,
        ino,
        modified: 0,
    })
}

const HARNESS_BUNDLE: &str = "com.deepseek.dsh";
const CURSOR_BUNDLE: &str = "com.todesktop.230313mzl4w4u92";
const CODEX_BUNDLE: &str = "com.openai.codex";
const CLAUDE_BUNDLE: &str = "com.anthropic.claudefordesktop";
const GEMINI_BUNDLE: &str = "com.google.GeminiMacOS";
const ANTIGRAVITY_BUNDLE: &str = "com.google.antigravity";
const WORKBUDDY_BUNDLE: &str = "com.tencent.workbuddy.mac";
const GROK_BOT_BUNDLE: &str = "com.anysphere.sand";
const MUSE_BUNDLE: &str = "com.meta.endo";
const CODEBUDDY_BUNDLE: &str = "com.tencent.codebuddycn";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceClass {
    Protected,
    Cleanable,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanEntry {
    pub rel: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub dev: u64,
    pub ino: u64,
    pub modified: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scan {
    pub protected_bytes: u64,
    pub cleanable_bytes: u64,
    pub unscanned: u32,
    pub cancelled: bool,
    pub permission: bool,
    pub entries: Vec<CleanEntry>,
    /// 受保护部分按第一层目录（或文件）合计，从大到小，最多 8 项
    pub parts: Vec<(String, u64)>,
}

pub fn scan_label(scan: &Scan) -> Option<&'static str> {
    if scan.cancelled {
        Some(LABEL_SCAN_CANCEL)
    } else if scan.permission {
        Some(LABEL_SCAN_DENIED)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceError {
    NotGranted,
    NeedsReauth,
    Io(io::ErrorKind),
}

impl SpaceError {
    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsReauth => LABEL_REAUTH,
            Self::NotGranted => "没有这项授权",
            Self::Io(_) => "文件访问失败",
        }
    }
}

pub fn classify(tool: ToolId, rel: &Path) -> SpaceClass {
    if metadata_denied(rel) {
        return SpaceClass::Skip;
    }
    if is_cleanable_dir(tool, rel) || under_cleanable(tool, rel) {
        SpaceClass::Cleanable
    } else {
        SpaceClass::Protected
    }
}

pub fn scan(guard: &Guard, tool: ToolId, cancel: &AtomicBool) -> Result<Scan, SpaceError> {
    let root = one_root(guard, tool, Capability::SpaceScan)?;
    if let Err(err) = guard.metadata(tool, Capability::SpaceScan, &root) {
        return match err {
            GuardError::SymlinkEscape | GuardError::NeedsReauth => Err(SpaceError::NeedsReauth),
            GuardError::Io(io::ErrorKind::PermissionDenied) => Ok(denied_scan()),
            GuardError::NotGranted | GuardError::UnregisteredRoot | GuardError::GrantRevoked => {
                Err(SpaceError::NotGranted)
            }
            GuardError::Io(kind) => Err(SpaceError::Io(kind)),
            _ => Err(SpaceError::Io(io::ErrorKind::Other)),
        };
    }
    Ok(scan_dir(&root, tool, cancel, &mut real_stat))
}

pub fn canonical_bytes(tool: ToolId, entries: &[CleanEntry]) -> Vec<u8> {
    let mut lines: Vec<String> = entries
        .iter()
        .map(|entry| {
            format!(
                "{}\t{}\t{}\t{}\t{}",
                rel_text(&entry.rel),
                entry.bytes,
                entry.files,
                entry.dev,
                entry.ino
            )
        })
        .collect();
    lines.sort();
    let mut out = format!("{}\n", tool_name(tool)).into_bytes();
    for line in lines {
        out.extend(line.into_bytes());
        out.push(b'\n');
    }
    out
}

pub fn content_changed(entry: &CleanEntry, bytes: u64, files: u64, dev: u64, ino: u64) -> bool {
    entry.bytes != bytes || entry.files != files || entry.dev != dev || entry.ino != ino
}

pub struct RunningView<'a> {
    pub bundle_ids: Option<&'a [String]>,
    pub process_names: Option<&'a [String]>,
    pub harness_executables: Option<&'a [String]>,
}

pub fn cleanup_block(tool: ToolId, view: &RunningView<'_>) -> Option<&'static str> {
    match tool {
        ToolId::DeepSeekHarness | ToolId::DsBot => dsh_block(view),
        ToolId::Cursor => app_block(view.bundle_ids, CURSOR_BUNDLE, LABEL_QUIT_CURSOR),
        ToolId::Codex => app_block(view.bundle_ids, CODEX_BUNDLE, LABEL_QUIT_CODEX),
        ToolId::Claude => app_block(view.bundle_ids, CLAUDE_BUNDLE, LABEL_QUIT_CLAUDE),
        ToolId::Gemini => app_block(view.bundle_ids, GEMINI_BUNDLE, LABEL_QUIT_GEMINI),
        ToolId::Antigravity => app_block(view.bundle_ids, ANTIGRAVITY_BUNDLE, LABEL_QUIT_ANTIGRAVITY),
        ToolId::WorkBuddy => app_block(view.bundle_ids, WORKBUDDY_BUNDLE, LABEL_QUIT_WORKBUDDY),
        ToolId::GrokBot => app_block(view.bundle_ids, GROK_BOT_BUNDLE, LABEL_QUIT_GROK_BOT),
        ToolId::Muse => app_block(view.bundle_ids, MUSE_BUNDLE, LABEL_QUIT_MUSE),
        ToolId::CodeBuddy => app_block(view.bundle_ids, CODEBUDDY_BUNDLE, LABEL_QUIT_CODEBUDDY),
        ToolId::CursorCli => app_block(view.bundle_ids, CURSOR_BUNDLE, LABEL_QUIT_CURSOR),
    }
}

/// 只取应用自己的两处文件名：`Contents/MacOS`，以及 `Contents/Resources/runtime/cli/bin`。
/// 后者是本机 Harness 包里 `dsh` 脚本的位置，脚本会 exec 到 MacOS 里的可执行文件。
/// 不把包内 python、node 等依赖算进进程名。
pub fn harness_executable_names(bundle: &Path) -> Result<Vec<String>, ()> {
    let mut names = executable_names(&bundle.join("Contents/MacOS"))?;
    if names.is_empty() {
        return Err(());
    }
    let cli = bundle.join("Contents/Resources/runtime/cli/bin");
    if cli.exists() {
        names.extend(executable_names(&cli)?);
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub trait Trasher {
    fn trash(&mut self, path: &Path) -> Result<(), TrashFault>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashFault {
    Permission,
    Busy,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupRecord {
    pub tool: ToolId,
    pub entry_count: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemReport {
    pub rel: String,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupReport {
    pub blocked: Option<&'static str>,
    pub record: CleanupRecord,
    pub items: Vec<ItemReport>,
}

pub fn cleanup(
    guard: &Guard,
    tool: ToolId,
    entries: &[CleanEntry],
    token: &ConfirmToken,
    running: &RunningView<'_>,
    trash: &mut dyn Trasher,
) -> Result<CleanupReport, GuardError> {
    if let Some(message) = cleanup_block(tool, running) {
        return Ok(blocked_report(tool, message));
    }
    let root = match cleanup_root(guard, tool) {
        Ok(root) => root,
        Err(SpaceError::NotGranted) => return Err(GuardError::NotGranted),
        Err(SpaceError::NeedsReauth) => return Err(GuardError::NeedsReauth),
        Err(SpaceError::Io(kind)) => return Err(GuardError::Io(kind)),
    };
    let bytes = canonical_bytes(tool, entries);
    guard.redeem_confirmation(token, tool, Capability::Cleanup, &bytes)?;
    let mut allow = |path: &Path| match guard.metadata(tool, Capability::Cleanup, path) {
        Ok(_) => None,
        Err(GuardError::Io(io::ErrorKind::PermissionDenied)) => Some(LABEL_PERMISSION),
        Err(GuardError::Io(io::ErrorKind::NotFound)) | Err(GuardError::SymlinkEscape) => {
            Some(LABEL_CHANGED)
        }
        Err(_) => Some(LABEL_PROTECTED),
    };
    Ok(cleanup_at(
        &root,
        tool,
        entries,
        &mut real_stat,
        trash,
        &mut allow,
    ))
}

fn blocked_report(tool: ToolId, message: &'static str) -> CleanupReport {
    CleanupReport {
        blocked: Some(message),
        record: CleanupRecord {
            tool,
            entry_count: 0,
            total_bytes: 0,
        },
        items: Vec::new(),
    }
}

fn dsh_block(view: &RunningView<'_>) -> Option<&'static str> {
    let (Some(bundles), Some(processes), Some(names)) = (
        view.bundle_ids,
        view.process_names,
        view.harness_executables,
    ) else {
        return Some(LABEL_UNKNOWN_DSH);
    };
    if names.is_empty() {
        return Some(LABEL_UNKNOWN_DSH);
    }
    if bundles.iter().any(|id| id == HARNESS_BUNDLE) {
        return Some(LABEL_QUIT_HARNESS);
    }
    if processes
        .iter()
        .any(|name| names.iter().any(|want| want == name))
    {
        return Some(LABEL_QUIT_DSH_PROCESS);
    }
    None
}

fn app_block(ids: Option<&[String]>, want: &str, quit: &'static str) -> Option<&'static str> {
    match ids {
        None => Some(LABEL_UNKNOWN_APP),
        Some(ids) if ids.iter().any(|id| id == want) => Some(quit),
        Some(_) => None,
    }
}

fn executable_names(dir: &Path) -> Result<Vec<String>, ()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| ())? {
        let entry = entry.map_err(|_| ())?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|_| ())?;
        if !meta.is_file() || !is_executable(&meta) {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        names.push(name);
    }
    Ok(names)
}

fn is_executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        true
    }
}

fn cleanup_root(guard: &Guard, tool: ToolId) -> Result<PathBuf, SpaceError> {
    let grants = guard.grants_for(tool);
    let mut root = None;
    let mut sample = None;
    let patterns = cleanup_patterns(tool);
    if patterns.is_empty() {
        return Err(SpaceError::NotGranted);
    }
    for pattern in patterns {
        let matched: Vec<_> = grants
            .iter()
            .filter(|grant| {
                grant.capability == Capability::Cleanup && grant.scope.pattern == *pattern
            })
            .collect();
        if matched.len() != 1 {
            return Err(SpaceError::NotGranted);
        }
        let this = matched[0].scope.root.clone();
        if let Some(previous) = &root {
            if previous != &this {
                return Err(SpaceError::NotGranted);
            }
        } else {
            root = Some(this);
            sample = Some(matched[0].id);
        }
    }
    let root = root.ok_or(SpaceError::NotGranted)?;
    let stamp = guard
        .stamp(sample.ok_or(SpaceError::NotGranted)?)
        .map_err(|_| SpaceError::NeedsReauth)?;
    let meta = fs::symlink_metadata(&root).map_err(|err| SpaceError::Io(err.kind()))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(SpaceError::NeedsReauth);
    }
    let (dev, ino) = root_ids(&root).map_err(|err| SpaceError::Io(err.kind()))?;
    if dev != stamp.dev || ino != stamp.ino {
        return Err(SpaceError::NeedsReauth);
    }
    Ok(root)
}

fn one_root(guard: &Guard, tool: ToolId, capability: Capability) -> Result<PathBuf, SpaceError> {
    let mut roots = Vec::new();
    for grant in guard.grants_for(tool) {
        if grant.capability == capability && grant.scope.pattern == "**" {
            roots.push(grant.scope.root);
        }
    }
    if roots.len() == 1 {
        Ok(roots.remove(0))
    } else {
        Err(SpaceError::NotGranted)
    }
}

fn denied_scan() -> Scan {
    Scan {
        protected_bytes: 0,
        cleanable_bytes: 0,
        unscanned: 1,
        cancelled: false,
        permission: true,
        entries: Vec::new(),
        parts: Vec::new(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
    Symlink,
    Other,
}

struct Meta {
    dev: u64,
    ino: u64,
    nlink: u64,
    len: u64,
    kind: Kind,
}

struct Content {
    bytes: u64,
    files: u64,
    holds_secret: bool,
}

struct Acc {
    protected_bytes: u64,
    cleanable_bytes: u64,
    unscanned: u32,
    cancelled: bool,
    permission: bool,
    entries: Vec<CleanEntry>,
    seen_protected: HashSet<(u64, u64)>,
    seen_clean: HashSet<(u64, u64)>,
    parts: std::collections::HashMap<String, u64>,
}

/// 受保护的字节记到第一层目录名下
fn add_protected(acc: &mut Acc, rel: &Path, bytes: u64) {
    acc.protected_bytes += bytes;
    let top = rel
        .components()
        .next()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    *acc.parts.entry(top).or_insert(0) += bytes;
}

fn scan_dir(
    root: &Path,
    tool: ToolId,
    cancel: &AtomicBool,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
) -> Scan {
    let mut acc = Acc {
        protected_bytes: 0,
        cleanable_bytes: 0,
        unscanned: 0,
        cancelled: false,
        permission: false,
        entries: Vec::new(),
        seen_protected: HashSet::new(),
        seen_clean: HashSet::new(),
        parts: std::collections::HashMap::new(),
    };
    let root_meta = match stat(root) {
        Ok(meta) if meta.kind == Kind::Dir => meta,
        Ok(_) => {
            acc.unscanned += 1;
            return finish(acc);
        }
        Err(err) => {
            note_stat_err(&mut acc, &err);
            return finish(acc);
        }
    };
    walk(
        root,
        Path::new(""),
        root_meta.dev,
        tool,
        cancel,
        stat,
        &mut acc,
    );
    finish(acc)
}

fn finish(mut acc: Acc) -> Scan {
    acc.entries
        .sort_by(|a, b| rel_text(&a.rel).cmp(&rel_text(&b.rel)));
    let mut parts: Vec<(String, u64)> = acc.parts.drain().filter(|(_, bytes)| *bytes > 0).collect();
    parts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    parts.truncate(8);
    Scan {
        protected_bytes: acc.protected_bytes,
        cleanable_bytes: acc.cleanable_bytes,
        unscanned: acc.unscanned,
        cancelled: acc.cancelled,
        permission: acc.permission,
        entries: acc.entries,
        parts,
    }
}

fn walk(
    dir: &Path,
    rel: &Path,
    root_dev: u64,
    tool: ToolId,
    cancel: &AtomicBool,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
    acc: &mut Acc,
) {
    if cancel.load(Ordering::Relaxed) {
        acc.cancelled = true;
        acc.unscanned += 1;
        return;
    }
    let names = match list_names(dir, acc) {
        Some(names) => names,
        None => return,
    };
    for (index, name) in names.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            acc.cancelled = true;
            acc.unscanned += (names.len() - index) as u32;
            return;
        }
        let child_rel = join_rel(rel, name);
        if metadata_denied(&child_rel) {
            continue;
        }
        let path = dir.join(name);
        let meta = match stat(&path) {
            Ok(meta) => meta,
            Err(err) => {
                note_stat_err(acc, &err);
                continue;
            }
        };
        if meta.kind != Kind::File && meta.kind != Kind::Dir {
            continue;
        }
        if meta.dev != root_dev {
            acc.unscanned += 1;
            continue;
        }
        if meta.kind == Kind::Dir {
            if is_cleanable_dir(tool, &child_rel) {
                list_units(&path, &child_rel, root_dev, cancel, stat, acc);
            } else {
                walk(&path, &child_rel, root_dev, tool, cancel, stat, acc);
            }
        } else if meta.ino == 0 || acc.seen_protected.insert((meta.dev, meta.ino)) {
            add_protected(acc, &child_rel, meta.len);
        }
    }
}

fn list_units(
    dir: &Path,
    rel: &Path,
    root_dev: u64,
    cancel: &AtomicBool,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
    acc: &mut Acc,
) {
    if cancel.load(Ordering::Relaxed) {
        acc.cancelled = true;
        acc.unscanned += 1;
        return;
    }
    let names = match list_names(dir, acc) {
        Some(names) => names,
        None => return,
    };
    for (index, name) in names.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            acc.cancelled = true;
            acc.unscanned += (names.len() - index) as u32;
            return;
        }
        let child_rel = join_rel(rel, name);
        if metadata_denied(&child_rel) {
            continue;
        }
        let path = dir.join(name);
        let meta = match stat(&path) {
            Ok(meta) => meta,
            Err(err) => {
                note_stat_err(acc, &err);
                continue;
            }
        };
        if meta.kind != Kind::File && meta.kind != Kind::Dir {
            continue;
        }
        if meta.dev != root_dev {
            acc.unscanned += 1;
            continue;
        }
        if meta.kind == Kind::File && meta.nlink > 1 {
            if meta.ino == 0 || acc.seen_clean.insert((meta.dev, meta.ino)) {
                acc.cleanable_bytes += meta.len;
            }
            continue;
        }
        match measure(&path, &child_rel, &meta, root_dev, stat) {
            Ok(content) if content.holds_secret => add_protected(acc, &child_rel, content.bytes),
            Ok(content) => {
                acc.cleanable_bytes += content.bytes;
                acc.entries.push(CleanEntry {
                    rel: child_rel,
                    bytes: content.bytes,
                    files: content.files,
                    dev: meta.dev,
                    ino: meta.ino,
                    modified: modified_secs(&path),
                });
            }
            Err(err) => note_stat_err(acc, &err),
        }
    }
}

fn measure(
    path: &Path,
    rel: &Path,
    meta: &Meta,
    root_dev: u64,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
) -> io::Result<Content> {
    if meta.kind == Kind::File {
        return Ok(Content {
            bytes: meta.len,
            files: 1,
            holds_secret: false,
        });
    }
    let mut content = Content {
        bytes: 0,
        files: 0,
        holds_secret: false,
    };
    let mut seen = HashSet::new();
    measure_dir(path, rel, root_dev, stat, &mut seen, &mut content)?;
    Ok(content)
}

fn measure_dir(
    dir: &Path,
    rel: &Path,
    root_dev: u64,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
    seen: &mut HashSet<(u64, u64)>,
    content: &mut Content,
) -> io::Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        names.push(entry?.file_name());
    }
    names.sort();
    for name in names {
        let path = dir.join(&name);
        let child_rel = join_rel(rel, &name);
        if metadata_denied(&child_rel) {
            content.holds_secret = true;
            continue;
        }
        let meta = stat(&path)?;
        if meta.kind != Kind::File && meta.kind != Kind::Dir {
            continue;
        }
        if meta.dev != root_dev {
            continue;
        }
        if meta.kind == Kind::Dir {
            measure_dir(&path, &child_rel, root_dev, stat, seen, content)?;
        } else if meta.ino == 0 || seen.insert((meta.dev, meta.ino)) {
            content.bytes += meta.len;
            content.files += 1;
        } else {
            content.files += 1;
        }
    }
    Ok(())
}

fn cleanup_at(
    root: &Path,
    tool: ToolId,
    entries: &[CleanEntry],
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
    trash: &mut dyn Trasher,
    allow: &mut dyn FnMut(&Path) -> Option<&'static str>,
) -> CleanupReport {
    let mut items = Vec::new();
    let mut record = CleanupRecord {
        tool,
        entry_count: 0,
        total_bytes: 0,
    };
    for entry in entries {
        let rel = rel_text(&entry.rel);
        let label = decide(root, tool, entry, stat, trash, allow);
        if label == LABEL_TRASHED {
            record.entry_count += 1;
            record.total_bytes += entry.bytes;
        }
        items.push(ItemReport { rel, label });
    }
    CleanupReport {
        blocked: None,
        record,
        items,
    }
}

fn decide(
    root: &Path,
    tool: ToolId,
    entry: &CleanEntry,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
    trash: &mut dyn Trasher,
    allow: &mut dyn FnMut(&Path) -> Option<&'static str>,
) -> &'static str {
    if path_outside(&entry.rel) {
        return LABEL_OUTSIDE;
    }
    if metadata_denied(&entry.rel) || !is_cleanup_unit(tool, &entry.rel) {
        return LABEL_PROTECTED;
    }
    let Ok(full) = crate::lexical_within(root, &entry.rel) else {
        return LABEL_OUTSIDE;
    };
    let meta = match confirm_chain(root, &full, stat) {
        Ok(meta) => meta,
        Err(ChainFail::Denied) => return LABEL_PERMISSION,
        Err(ChainFail::Changed) => return LABEL_CHANGED,
    };
    if let Some(label) = allow(&full) {
        return label;
    }
    if meta.kind == Kind::File && meta.nlink > 1 {
        return LABEL_PROTECTED;
    }
    if meta.kind != Kind::File && meta.kind != Kind::Dir {
        return LABEL_CHANGED;
    }
    let content = match measure(&full, &entry.rel, &meta, meta.dev, stat) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => return LABEL_PERMISSION,
        Err(_) => return LABEL_CHANGED,
    };
    if content.holds_secret {
        return LABEL_PROTECTED;
    }
    if content_changed(entry, content.bytes, content.files, meta.dev, meta.ino) {
        return LABEL_CHANGED;
    }
    match trash.trash(&full) {
        Ok(()) => LABEL_TRASHED,
        Err(TrashFault::Permission) => LABEL_PERMISSION,
        Err(TrashFault::Busy) => LABEL_BUSY,
        Err(TrashFault::Other) => LABEL_TRASH,
    }
}

enum ChainFail {
    Changed,
    Denied,
}

fn confirm_chain(
    root: &Path,
    full: &Path,
    stat: &mut impl FnMut(&Path) -> io::Result<Meta>,
) -> Result<Meta, ChainFail> {
    let root_meta = stat(root).map_err(chain_fail)?;
    if root_meta.kind != Kind::Dir {
        return Err(ChainFail::Changed);
    }
    let Ok(rel) = full.strip_prefix(root) else {
        return Err(ChainFail::Changed);
    };
    let parts: Vec<_> = rel.components().collect();
    if parts.is_empty() {
        return Err(ChainFail::Changed);
    }
    let mut cursor = root.to_path_buf();
    let mut entry_meta = None;
    for (index, part) in parts.iter().enumerate() {
        cursor.push(part);
        let meta = stat(&cursor).map_err(chain_fail)?;
        let last = index + 1 == parts.len();
        if meta.dev != root_meta.dev
            || meta.kind == Kind::Symlink
            || (!last && meta.kind != Kind::Dir)
        {
            return Err(ChainFail::Changed);
        }
        if last && meta.kind != Kind::File && meta.kind != Kind::Dir {
            return Err(ChainFail::Changed);
        }
        if last {
            entry_meta = Some(meta);
        }
    }
    entry_meta.ok_or(ChainFail::Changed)
}

fn chain_fail(err: io::Error) -> ChainFail {
    if err.kind() == io::ErrorKind::PermissionDenied {
        ChainFail::Denied
    } else {
        ChainFail::Changed
    }
}

fn path_outside(rel: &Path) -> bool {
    rel.is_absolute()
        || rel.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
}

fn list_names(dir: &Path, acc: &mut Acc) -> Option<Vec<std::ffi::OsString>> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(err) => {
            note_stat_err(acc, &err);
            return None;
        }
    };
    let mut names = Vec::new();
    for entry in read {
        match entry {
            Ok(entry) => names.push(entry.file_name()),
            Err(err) => note_stat_err(acc, &err),
        }
    }
    names.sort();
    Some(names)
}

fn note_stat_err(acc: &mut Acc, err: &io::Error) {
    if err.kind() == io::ErrorKind::PermissionDenied {
        acc.permission = true;
    }
    acc.unscanned += 1;
}

fn join_rel(rel: &Path, name: &std::ffi::OsStr) -> PathBuf {
    if rel.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        rel.join(name)
    }
}

fn real_stat(path: &Path) -> io::Result<Meta> {
    let meta = fs::symlink_metadata(path)?;
    let kind = if meta.file_type().is_symlink() {
        Kind::Symlink
    } else if meta.is_dir() {
        Kind::Dir
    } else if meta.is_file() {
        Kind::File
    } else {
        Kind::Other
    };
    let (dev, ino, nlink) = node_ids(path, &meta);
    Ok(Meta {
        dev,
        ino,
        nlink,
        len: meta.len(),
        kind,
    })
}

fn modified_secs(path: &Path) -> u64 {
    fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn is_cleanable_dir(tool: ToolId, rel: &Path) -> bool {
    let Some(rel) = strict_rel(rel) else {
        return false;
    };
    cleanup_patterns(tool)
        .iter()
        .any(|pattern| pattern.strip_suffix("/*") == Some(rel.as_str()))
}

fn under_cleanable(tool: ToolId, rel: &Path) -> bool {
    let mut prefix = PathBuf::new();
    let mut saw = false;
    for component in rel.components() {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        if saw {
            prefix.push(name);
        } else {
            prefix = PathBuf::from(name);
            saw = true;
        }
        if is_cleanable_dir(tool, &prefix) && prefix != rel {
            return true;
        }
    }
    false
}

fn is_cleanup_unit(tool: ToolId, rel: &Path) -> bool {
    let Some(parent) = rel.parent() else {
        return false;
    };
    is_cleanable_dir(tool, parent)
}

fn rel_text(path: &Path) -> String {
    strict_rel(path).unwrap_or_else(|| format!("outside:{}", path.display()))
}

fn strict_rel(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return Some(String::new());
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(text) => parts.push(text.to_string_lossy().into_owned()),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

fn tool_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::DeepSeekHarness => "deepseek_harness",
        ToolId::DsBot => "ds_bot",
        ToolId::Cursor => "cursor",
        ToolId::Codex => "codex",
        ToolId::Claude => "claude",
        ToolId::Gemini => "gemini",
        ToolId::Antigravity => "antigravity",
        ToolId::WorkBuddy => "workbuddy",
        ToolId::GrokBot => "grok_bot",
        ToolId::Muse => "muse",
        ToolId::CodeBuddy => "codebuddy",
        ToolId::CursorCli => "cursor_cli",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn touch(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn credential_names_are_not_stat() {
        let root = std::env::temp_dir().join(format!("wanxiang-space-skip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cursorAuth")).unwrap();
        touch(&root.join("note.txt"), b"abc");
        touch(&root.join("auth.json"), vec![1; 1000].as_slice());
        touch(&root.join(".credentials.yaml"), vec![2; 1000].as_slice());
        touch(
            &root.join("cursorAuth").join("key"),
            vec![3; 1000].as_slice(),
        );
        let seen = Mutex::new(Vec::new());
        let mut stat = |path: &Path| {
            let text = path.to_string_lossy();
            assert!(!text.contains("auth.json"), "{text}");
            assert!(!text.contains(".credentials.yaml"), "{text}");
            assert!(!text.contains("cursorAuth"), "{text}");
            seen.lock().unwrap().push(path.to_path_buf());
            real_stat(path)
        };
        let scan = scan_dir(&root, ToolId::Cursor, &AtomicBool::new(false), &mut stat);
        assert_eq!(scan.protected_bytes, 3);
        assert!(seen
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.ends_with("note.txt")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_after_one_directory_does_not_stat_the_next() {
        let root =
            std::env::temp_dir().join(format!("wanxiang-space-cancel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        touch(&root.join("a").join("z"), b"z");
        touch(&root.join("b").join("y"), b"y");
        let cancel = AtomicBool::new(false);
        let seen = Mutex::new(Vec::new());
        let mut stat = |path: &Path| {
            seen.lock().unwrap().push(path.to_path_buf());
            let meta = real_stat(path)?;
            if path.file_name() == Some(std::ffi::OsStr::new("z")) {
                cancel.store(true, Ordering::SeqCst);
            }
            Ok(meta)
        };
        let scan = scan_dir(&root, ToolId::Codex, &cancel, &mut stat);
        assert!(scan.cancelled);
        let names: Vec<_> = seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|path| path.file_name().map(|name| name.to_os_string()))
            .collect();
        assert!(names.iter().any(|name| name == "z"));
        assert!(names.iter().all(|name| name != "b" && name != "y"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_set_after_start_stops_before_the_next_directory() {
        let root =
            std::env::temp_dir().join(format!("wanxiang-space-cancel-next-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        touch(&root.join("a").join("z"), b"z");
        touch(&root.join("b").join("y"), b"y");
        let cancel = AtomicBool::new(false);
        let seen = Mutex::new(Vec::new());
        let mut stat = |path: &Path| {
            seen.lock().unwrap().push(path.to_path_buf());
            let meta = real_stat(path)?;
            if path.file_name() == Some(std::ffi::OsStr::new("a")) {
                cancel.store(true, Ordering::SeqCst);
            }
            Ok(meta)
        };
        let scan = scan_dir(&root, ToolId::Codex, &cancel, &mut stat);
        assert!(scan.cancelled);
        let names: Vec<_> = seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|path| path.file_name().map(|name| name.to_os_string()))
            .collect();
        assert!(names.iter().any(|name| name == "a"));
        assert!(names
            .iter()
            .all(|name| name != "z" && name != "b" && name != "y"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn other_device_is_unscanned_and_not_entered() {
        let root = std::env::temp_dir().join(format!("wanxiang-space-dev-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        touch(&root.join("local").join("keep"), b"1234");
        touch(&root.join("foreign").join("secret"), vec![9; 50].as_slice());
        let seen = Mutex::new(Vec::new());
        let mut stat = |path: &Path| {
            seen.lock().unwrap().push(path.to_path_buf());
            let mut meta = real_stat(path)?;
            if path
                .components()
                .any(|component| component.as_os_str() == "foreign")
            {
                meta.dev = meta.dev.wrapping_add(1);
            }
            Ok(meta)
        };
        let scan = scan_dir(&root, ToolId::Codex, &AtomicBool::new(false), &mut stat);
        assert_eq!(scan.protected_bytes, 4);
        assert!(scan.unscanned >= 1);
        assert!(seen
            .lock()
            .unwrap()
            .iter()
            .all(|path| !path.ends_with("secret")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn secret_inside_a_listed_entry_is_not_stat_and_not_trashed() {
        let root = std::env::temp_dir().join(format!("wanxiang-space-nest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cache").join("job")).unwrap();
        touch(&root.join("cache").join("job").join("auth.json"), b"secret");
        let entry = CleanEntry {
            rel: PathBuf::from("cache/job"),
            bytes: 0,
            files: 0,
            dev: 1,
            ino: 1,
            modified: 0,
        };
        let mut stat = |path: &Path| {
            assert!(
                !path.to_string_lossy().contains("auth.json"),
                "{}",
                path.display()
            );
            real_stat(path)
        };
        let mut trash = RecTrash::default();
        let report = cleanup_at(
            &root,
            ToolId::DeepSeekHarness,
            &[entry],
            &mut stat,
            &mut trash,
            &mut |_| None,
        );
        assert_eq!(report.items[0].label, LABEL_PROTECTED);
        assert!(trash.paths.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[derive(Default)]
    struct RecTrash {
        paths: Vec<PathBuf>,
    }

    impl Trasher for RecTrash {
        fn trash(&mut self, path: &Path) -> Result<(), TrashFault> {
            self.paths.push(path.to_path_buf());
            Ok(())
        }
    }
}
