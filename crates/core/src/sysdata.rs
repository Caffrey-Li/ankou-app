//! 本机空间：AI 工具以外的部分（系统、开发工具、本地模型）。
//!
//! 只统计大小，不读文件内容。清理只把条目移到废纸篓；需要命令或管理员权限的位置只列出并说明，
//! 安扣不替用户执行。目录清单是安扣自己整理的，按 macOS 的公开约定写成，不依赖任何第三方工具。

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::guard::node_ids;
use crate::space::{TrashFault, Trasher};

/// 这一项属于哪一边：AI 相关的放在「AI 工具」，其余放在「系统」。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Ai,
    System,
}

/// 能做什么：可以直接清理、清理前要想清楚、只能按说明手动处理。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Safe,
    Review,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// 用户目录下的相对路径
    Home,
    /// 绝对路径（只统计，不清理）
    Absolute,
}

/// 目录里哪些子项属于这一项（按名字前缀）。
#[derive(Clone, Copy, Debug)]
pub enum Filter {
    All,
    Only(&'static [&'static str]),
    Except(&'static [&'static str]),
    /// 名字以前缀开头、以后缀结尾（例如 /Applications 里的 macOS 安装器）
    Named(&'static str, &'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub id: &'static str,
    pub side: Side,
    pub group: &'static str,
    pub title: &'static str,
    pub action: Action,
    pub base: Base,
    pub roots: &'static [&'static str],
    pub filter: Filter,
    /// 清理会带来什么，或者该怎么手动处理
    pub note: &'static str,
    /// 只有装了这个应用才显示（手动项用）
    pub needs_app: Option<&'static str>,
}

/// AI 应用在 ~/Library/Caches 和 ~/Library/Logs 里的目录名前缀：归到「AI 工具」，不算进系统部分。
pub const AI_CACHE_PREFIXES: &[&str] = &[
    "com.openai.",
    "com.anthropic.",
    "com.todesktop.230313mzl4w4u92",
    "com.google.antigravity",
    "com.google.GeminiMacOS",
    "com.tencent.workbuddy",
    "com.tencent.codebuddy",
    "com.anysphere.",
    "com.meta.endo",
    "com.deepseek.",
    "Cursor",
    "Claude",
    "ChatGPT",
    "Antigravity",
    "Gemini",
    "WorkBuddy",
    "CodeBuddy",
    "DeepSeek",
];

pub const CATALOG: &[Item] = &[
    // —— AI 工具：各厂商以外的本地模型与缓存 ——
    Item {
        id: "ai_app_caches",
        side: Side::Ai,
        group: "AI 应用缓存与日志",
        title: "AI 应用的缓存与日志",
        action: Action::Safe,
        base: Base::Home,
        roots: &["Library/Caches", "Library/Logs"],
        filter: Filter::Only(AI_CACHE_PREFIXES),
        note: "各 AI 应用放在系统缓存和日志目录里的文件，应用会按需重建；正在运行的应用的缓存会跳过",
        needs_app: None,
    },
    Item {
        id: "ollama_models",
        side: Side::Ai,
        group: "本地模型",
        title: "Ollama 模型",
        action: Action::Review,
        base: Base::Home,
        roots: &[".ollama/models"],
        filter: Filter::All,
        note: "删除后要用模型时需重新下载",
        needs_app: None,
    },
    Item {
        id: "lmstudio_models",
        side: Side::Ai,
        group: "本地模型",
        title: "LM Studio 模型",
        action: Action::Review,
        base: Base::Home,
        roots: &[".lmstudio/models", ".cache/lm-studio/models"],
        filter: Filter::All,
        note: "删除后要用模型时需重新下载",
        needs_app: None,
    },
    Item {
        id: "huggingface",
        side: Side::Ai,
        group: "本地模型",
        title: "Hugging Face 缓存",
        action: Action::Review,
        base: Base::Home,
        roots: &[".cache/huggingface"],
        filter: Filter::All,
        note: "下载过的模型和数据集，用到时会重新下载",
        needs_app: None,
    },
    Item {
        id: "torch_hub",
        side: Side::Ai,
        group: "本地模型",
        title: "PyTorch 模型缓存",
        action: Action::Review,
        base: Base::Home,
        roots: &[".cache/torch"],
        filter: Filter::All,
        note: "用到时会重新下载",
        needs_app: None,
    },
    Item {
        id: "whisper_models",
        side: Side::Ai,
        group: "本地模型",
        title: "Whisper 语音模型",
        action: Action::Review,
        base: Base::Home,
        roots: &[".cache/whisper"],
        filter: Filter::All,
        note: "用到时会重新下载",
        needs_app: None,
    },
    Item {
        id: "easyocr_models",
        side: Side::Ai,
        group: "本地模型",
        title: "EasyOCR 模型",
        action: Action::Review,
        base: Base::Home,
        roots: &[".EasyOCR/model"],
        filter: Filter::All,
        note: "用到时会重新下载",
        needs_app: None,
    },
    // —— 系统：缓存与日志 ——
    Item {
        id: "app_caches",
        side: Side::System,
        group: "缓存与日志",
        title: "应用缓存",
        action: Action::Safe,
        base: Base::Home,
        roots: &["Library/Caches"],
        filter: Filter::Except(AI_CACHE_PREFIXES),
        note: "各应用的临时缓存，会按需重建；正在运行的应用会跳过",
        needs_app: None,
    },
    Item {
        id: "user_logs",
        side: Side::System,
        group: "缓存与日志",
        title: "日志与诊断报告",
        action: Action::Safe,
        base: Base::Home,
        roots: &["Library/Logs"],
        filter: Filter::Except(AI_CACHE_PREFIXES),
        note: "应用日志和崩溃报告，排查问题时才用得到",
        needs_app: None,
    },
    Item {
        id: "win_temp",
        side: Side::System,
        group: "缓存与日志",
        title: "Windows 临时文件",
        action: Action::Safe,
        base: Base::Home,
        roots: &["AppData/Local/Temp"],
        filter: Filter::All,
        note: "系统与应用的临时文件，正在占用的文件会自动跳过",
        needs_app: None,
    },
    // —— 系统：开发工具 ——
    Item {
        id: "xcode_derived",
        side: Side::System,
        group: "开发工具",
        title: "Xcode 编译缓存",
        action: Action::Safe,
        base: Base::Home,
        roots: &["Library/Developer/Xcode/DerivedData"],
        filter: Filter::All,
        note: "下次编译时自动重建，首次编译会慢一些",
        needs_app: None,
    },
    Item {
        id: "xcode_device_support",
        side: Side::System,
        group: "开发工具",
        title: "设备调试支持文件",
        action: Action::Safe,
        base: Base::Home,
        roots: &[
            "Library/Developer/Xcode/iOS DeviceSupport",
            "Library/Developer/Xcode/watchOS DeviceSupport",
            "Library/Developer/Xcode/tvOS DeviceSupport",
            "Library/Developer/Xcode/visionOS DeviceSupport",
        ],
        filter: Filter::All,
        note: "连接设备调试时会重新准备",
        needs_app: None,
    },
    Item {
        id: "xcode_archives",
        side: Side::System,
        group: "开发工具",
        title: "Xcode 归档",
        action: Action::Review,
        base: Base::Home,
        roots: &["Library/Developer/Xcode/Archives"],
        filter: Filter::All,
        note: "删除后无法再用这些归档解析旧版本的崩溃日志",
        needs_app: None,
    },
    Item {
        id: "simulator_caches",
        side: Side::System,
        group: "开发工具",
        title: "模拟器缓存",
        action: Action::Safe,
        base: Base::Home,
        roots: &["Library/Developer/CoreSimulator/Caches"],
        filter: Filter::All,
        note: "模拟器启动时会重建",
        needs_app: None,
    },
    Item {
        id: "simulator_devices",
        side: Side::System,
        group: "开发工具",
        title: "模拟器设备",
        action: Action::Manual,
        base: Base::Home,
        roots: &["Library/Developer/CoreSimulator/Devices"],
        filter: Filter::All,
        note: "直接删除会弄乱 Xcode 的设备列表。请在 Xcode「设备与模拟器」里删除，或在终端运行 xcrun simctl delete unavailable",
        needs_app: None,
    },
    Item {
        id: "npm_cache",
        side: Side::System,
        group: "开发工具",
        title: "npm 缓存",
        action: Action::Safe,
        base: Base::Home,
        roots: &[".npm/_cacache"],
        filter: Filter::All,
        note: "安装依赖时会重新下载",
        needs_app: None,
    },
    Item {
        id: "pnpm_store",
        side: Side::System,
        group: "开发工具",
        title: "pnpm 依赖仓库",
        action: Action::Review,
        base: Base::Home,
        roots: &["Library/pnpm/store", "AppData/Local/pnpm/store"],
        filter: Filter::All,
        note: "项目依赖的共享存放处，删除后安装依赖时会重新下载",
        needs_app: None,
    },
    Item {
        id: "gradle_caches",
        side: Side::System,
        group: "开发工具",
        title: "Gradle 缓存",
        action: Action::Safe,
        base: Base::Home,
        roots: &[".gradle/caches"],
        filter: Filter::All,
        note: "下次构建时重新下载",
        needs_app: None,
    },
    Item {
        id: "maven_repo",
        side: Side::System,
        group: "开发工具",
        title: "Maven 本地仓库",
        action: Action::Review,
        base: Base::Home,
        roots: &[".m2/repository"],
        filter: Filter::All,
        note: "下次构建时重新下载；离线时无法构建",
        needs_app: None,
    },
    Item {
        id: "cargo_registry",
        side: Side::System,
        group: "开发工具",
        title: "Cargo 依赖缓存",
        action: Action::Review,
        base: Base::Home,
        roots: &[".cargo/registry"],
        filter: Filter::All,
        note: "下次编译时重新下载；离线时无法编译",
        needs_app: None,
    },
    Item {
        id: "go_modules",
        side: Side::System,
        group: "开发工具",
        title: "Go 模块缓存",
        action: Action::Review,
        base: Base::Home,
        roots: &["go/pkg/mod"],
        filter: Filter::All,
        note: "下次编译时重新下载",
        needs_app: None,
    },
    Item {
        id: "docker_disk",
        side: Side::System,
        group: "开发工具",
        title: "Docker 磁盘映像",
        action: Action::Manual,
        base: Base::Home,
        roots: &[],
        filter: Filter::All,
        note: "容器和镜像都在 Docker 的磁盘映像里，安扣不打开它。请在 Docker Desktop 里清理，或在终端运行 docker system prune",
        needs_app: Some("Docker.app"),
    },
    // —— 系统：设备与备份 ——
    Item {
        id: "ios_backups",
        side: Side::System,
        group: "设备与备份",
        title: "iPhone / iPad 备份",
        action: Action::Review,
        base: Base::Home,
        roots: &["Library/Application Support/MobileSync/Backup"],
        filter: Filter::All,
        note: "每个文件夹是一台设备的备份。删除前确认设备已有 iCloud 备份或不再需要",
        needs_app: None,
    },
    Item {
        id: "macos_installers",
        side: Side::System,
        group: "设备与备份",
        title: "macOS 安装器",
        action: Action::Manual,
        base: Base::Absolute,
        roots: &["/Applications"],
        filter: Filter::Named("Install macOS", ".app"),
        note: "升级完成后通常用不到。请在「应用程序」里把它拖到废纸篓",
        needs_app: None,
    },
    // —— 系统：系统自己管理的部分 ——
    Item {
        id: "trash",
        side: Side::System,
        group: "系统管理",
        title: "废纸篓",
        action: Action::Manual,
        base: Base::Home,
        roots: &[".Trash"],
        filter: Filter::All,
        note: "安扣清理的东西都在这里。确认不再需要后，在访达里清倒废纸篓",
        needs_app: None,
    },
    Item {
        id: "tm_snapshots",
        side: Side::System,
        group: "系统管理",
        title: "时间机器本地快照",
        action: Action::Manual,
        base: Base::Absolute,
        roots: &[],
        filter: Filter::All,
        note: "系统会在空间紧张时自动删除。也可在终端运行 tmutil deletelocalsnapshots / 手动删除",
        needs_app: None,
    },
    Item {
        id: "swap",
        side: Side::System,
        group: "系统管理",
        title: "虚拟内存与睡眠镜像",
        action: Action::Manual,
        base: Base::Absolute,
        roots: &["/private/var/vm"],
        filter: Filter::All,
        note: "由系统管理，不能删除。重启后通常会变小",
        needs_app: None,
    },
];

pub fn item(id: &str) -> Option<&'static Item> {
    CATALOG.iter().find(|item| item.id == id)
}

/// 子项名字必须是目录里的一层名字，不能带路径。
pub fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

impl Filter {
    pub fn keeps(self, name: &str) -> bool {
        match self {
            Filter::All => true,
            Filter::Only(prefixes) => prefixes.iter().any(|prefix| name.starts_with(prefix)),
            Filter::Except(prefixes) => !prefixes.iter().any(|prefix| name.starts_with(prefix)),
            Filter::Named(prefix, suffix) => name.starts_with(prefix) && name.ends_with(suffix),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// 统计完成
    Ok,
    /// 本机没有这个位置
    Missing,
    /// 有的位置读不到（通常要「完全磁盘访问权限」）
    Denied,
    /// 只说明，不统计大小
    Info,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChildSize {
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemScan {
    pub id: &'static str,
    pub side: Side,
    pub group: &'static str,
    pub title: &'static str,
    pub action: Action,
    pub note: &'static str,
    /// 给人看的位置，用户目录写成 ~
    pub paths: Vec<String>,
    pub state: ItemState,
    pub bytes: u64,
    pub count: u64,
    /// 最大的若干子项，供逐个勾选
    pub children: Vec<ChildSize>,
    /// 只对时间机器快照：快照个数
    pub snapshots: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Disk {
    pub total: u64,
    pub available: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SysScan {
    pub items: Vec<ItemScan>,
    pub disk: Option<Disk>,
    pub cancelled: bool,
}

pub const CHILD_LIMIT: usize = 40;

/// 统计一个目录（或文件）实际占用的磁盘空间。不跟随链接，不跨卷，同一文件只算一次。
pub fn measure(path: &Path, seen: &mut HashSet<(u64, u64)>, cancel: &AtomicBool) -> io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    let dev = node_ids(path, &meta).0;
    let mut total = 0;
    let mut stack = vec![(path.to_path_buf(), meta)];
    while let Some((at, meta)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let (this_dev, ino, _) = node_ids(&at, &meta);
        if this_dev != dev || meta.file_type().is_symlink() {
            continue;
        }
        if ino != 0 && !seen.insert((this_dev, ino)) {
            continue;
        }
        total += allocated(&meta);
        if meta.is_dir() {
            let Ok(entries) = fs::read_dir(&at) else {
                continue;
            };
            for entry in entries.flatten() {
                if let Ok(child) = entry.metadata() {
                    stack.push((entry.path(), child));
                }
            }
        }
    }
    Ok(total)
}

fn allocated(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

pub fn root_path(home: &Path, item: &Item, root: &str) -> PathBuf {
    match item.base {
        Base::Home => home.join(root),
        Base::Absolute => PathBuf::from(root),
    }
}

fn shown(item: &Item, root: &str) -> String {
    match item.base {
        Base::Home => format!("~/{root}"),
        Base::Absolute => root.to_string(),
    }
}

/// 根目录必须是真实目录，不能是链接，也不能借链接跳出用户目录。
pub fn real_dir(home: &Path, item: &Item, path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "不是目录"));
    }
    if item.base == Base::Home {
        let home = fs::canonicalize(home)?;
        let real = fs::canonicalize(path)?;
        if !real.starts_with(&home) || real == home {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "超出用户目录"));
        }
    }
    Ok(())
}

/// 列出一个根目录里属于这一项的子项名字。
pub fn child_names(path: &Path, filter: Filter) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name == ".DS_Store" || !filter.keeps(&name) {
            continue;
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

pub struct Probe<'a> {
    pub home: &'a Path,
    /// /Applications 里有哪些应用（只看名字）
    pub has_app: &'a dyn Fn(&str) -> bool,
    /// 时间机器本地快照个数；拿不到时为 None
    pub snapshots: &'a dyn Fn() -> Option<u32>,
}

pub fn scan_all(probe: &Probe<'_>, cancel: &AtomicBool) -> SysScan {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for item in CATALOG {
        if cancel.load(Ordering::Relaxed) {
            return SysScan {
                items,
                disk: None,
                cancelled: true,
            };
        }
        if let Some(app) = item.needs_app {
            if !(probe.has_app)(app) {
                continue;
            }
        }
        items.push(scan_item(probe, item, &mut seen, cancel));
    }
    SysScan {
        items,
        disk: None,
        cancelled: cancel.load(Ordering::Relaxed),
    }
}

fn scan_item(probe: &Probe<'_>, item: &'static Item, seen: &mut HashSet<(u64, u64)>, cancel: &AtomicBool) -> ItemScan {
    let mut out = ItemScan {
        id: item.id,
        side: item.side,
        group: item.group,
        title: item.title,
        action: item.action,
        note: item.note,
        paths: item.roots.iter().map(|root| shown(item, root)).collect(),
        state: ItemState::Missing,
        bytes: 0,
        count: 0,
        children: Vec::new(),
        snapshots: None,
    };
    if item.id == "tm_snapshots" {
        out.state = ItemState::Info;
        out.snapshots = (probe.snapshots)();
        return out;
    }
    if item.roots.is_empty() {
        out.state = ItemState::Info;
        return out;
    }
    let mut found = false;
    let mut denied = false;
    let mut children = Vec::new();
    for root in item.roots {
        let path = root_path(probe.home, item, root);
        match fs::symlink_metadata(&path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                denied = true;
                continue;
            }
            Ok(_) => {}
        }
        if real_dir(probe.home, item, &path).is_err() {
            continue;
        }
        found = true;
        let names = match child_names(&path, item.filter) {
            Ok(names) => names,
            Err(_) => {
                denied = true;
                continue;
            }
        };
        for name in names {
            let bytes = measure(&path.join(&name), seen, cancel).unwrap_or(0);
            out.bytes += bytes;
            out.count += 1;
            children.push(ChildSize { name, bytes });
        }
    }
    children.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    children.truncate(CHILD_LIMIT);
    out.children = children;
    out.state = if denied {
        ItemState::Denied
    } else if found {
        ItemState::Ok
    } else {
        ItemState::Missing
    };
    out
}

/// 读 `tmutil listlocalsnapshots /` 的输出，数快照个数。
pub fn count_snapshots(text: &str) -> u32 {
    text.lines()
        .filter(|line| line.contains("com.apple.TimeMachine."))
        .count() as u32
}

/// 读 `df -k /` 的输出：总量和可用（字节）。
pub fn parse_df(text: &str) -> Option<Disk> {
    let line = text.lines().nth(1)?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    let total: u64 = fields.get(1)?.parse().ok()?;
    let available: u64 = fields.get(3)?.parse().ok()?;
    Some(Disk {
        total: total * 1024,
        available: available * 1024,
    })
}

// —— 清理 ——

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Pick {
    pub id: String,
    /// 只清理这些子项；不给就是这一项里的全部
    #[serde(default)]
    pub children: Option<Vec<String>>,
}

pub const LABEL_DONE: &str = "已清理";
pub const LABEL_RUNNING: &str = "已跳过 · 应用正在运行";
pub const LABEL_DENIED: &str = "失败 · 权限不足";
pub const LABEL_FAILED: &str = "失败 · 移到废纸篓失败";
pub const LABEL_GONE: &str = "已跳过 · 已不存在";
pub const LABEL_MANUAL: &str = "需要手动处理";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PickReport {
    pub name: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SysCleanup {
    pub entry_count: u64,
    pub total_bytes: u64,
    pub items: Vec<PickReport>,
}

/// 校验所选内容：只认清单里的项，手动项不能清理，子项名字不能带路径。
pub fn checked_picks(picks: &[Pick]) -> Result<Vec<(&'static Item, Option<Vec<String>>)>, &'static str> {
    let mut out = Vec::new();
    let mut ids = HashSet::new();
    for pick in picks {
        let item = item(&pick.id).ok_or("没有这一项")?;
        if item.action == Action::Manual || item.base != Base::Home {
            return Err(LABEL_MANUAL);
        }
        if !ids.insert(item.id) {
            return Err("重复的项");
        }
        if let Some(children) = &pick.children {
            if children.is_empty() || !children.iter().all(|name| plain_name(name)) {
                return Err("子项名字不合法");
            }
        }
        out.push((item, pick.children.clone()));
    }
    if out.is_empty() {
        return Err("没有选择要清理的内容");
    }
    Ok(out)
}

/// 用于确认的规范化内容：同样的选择得到同样的字节。
pub fn pick_bytes(picks: &[Pick]) -> Vec<u8> {
    let mut lines: Vec<String> = picks
        .iter()
        .map(|pick| {
            let mut children = pick.children.clone().unwrap_or_default();
            children.sort();
            format!("{}\t{}", pick.id, children.join("\u{1f}"))
        })
        .collect();
    lines.sort();
    lines.join("\n").into_bytes()
}

/// 这一子项是不是正在运行的应用的缓存（目录名以应用的 bundle id 开头）。
pub fn belongs_to_running(name: &str, running: &[String]) -> bool {
    running
        .iter()
        .any(|id| !id.is_empty() && (name == id || name.starts_with(&format!("{id}."))))
}

pub fn cleanup(
    home: &Path,
    picks: &[(&'static Item, Option<Vec<String>>)],
    running: Option<&[String]>,
    trash: &mut dyn Trasher,
) -> SysCleanup {
    let mut report = SysCleanup {
        entry_count: 0,
        total_bytes: 0,
        items: Vec::new(),
    };
    let never = AtomicBool::new(false);
    for (item, wanted) in picks {
        let mut done = 0u64;
        let mut bytes_done = 0u64;
        let mut skipped_running = 0u64;
        let mut failed: Option<&'static str> = None;
        let mut any_root = false;
        for root in item.roots {
            let path = root_path(home, item, root);
            if real_dir(home, item, &path).is_err() {
                continue;
            }
            any_root = true;
            let names = match child_names(&path, item.filter) {
                Ok(names) => names,
                Err(_) => {
                    failed = Some(LABEL_DENIED);
                    continue;
                }
            };
            for name in names {
                if let Some(wanted) = wanted {
                    if !wanted.iter().any(|want| want == &name) {
                        continue;
                    }
                }
                // 缓存类：应用还开着就不动它的目录；看不到运行列表时一律跳过
                if *root == "Library/Caches" {
                    match running {
                        Some(ids) if !belongs_to_running(&name, ids) => {}
                        _ => {
                            skipped_running += 1;
                            continue;
                        }
                    }
                }
                let target = path.join(&name);
                let bytes = measure(&target, &mut HashSet::new(), &never).unwrap_or(0);
                match trash.trash(&target) {
                    Ok(()) => {
                        done += 1;
                        bytes_done += bytes;
                    }
                    Err(TrashFault::Permission) => failed = Some(LABEL_DENIED),
                    Err(_) => failed = failed.or(Some(LABEL_FAILED)),
                }
            }
        }
        report.entry_count += done;
        report.total_bytes += bytes_done;
        let label = if !any_root {
            LABEL_GONE.to_string()
        } else if let Some(fault) = failed {
            if done > 0 {
                format!("部分完成 · {fault}")
            } else {
                fault.to_string()
            }
        } else if done == 0 && skipped_running > 0 {
            LABEL_RUNNING.to_string()
        } else if skipped_running > 0 {
            format!("{LABEL_DONE} {} 项 · {skipped_running} 项应用正在运行已跳过", done)
        } else {
            format!("{LABEL_DONE} {done} 项")
        };
        report.items.push(PickReport {
            name: item.title.to_string(),
            label,
        });
    }
    report
}

// —— 确认：先预览、再清理，同一份选择只能用一次 ——

pub const SYS_CONFIRM_TTL: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct SysConfirm {
    pending: Mutex<Option<(String, Vec<u8>, Instant)>>,
}

impl SysConfirm {
    pub fn issue(&self, bytes: &[u8]) -> io::Result<String> {
        let mut raw = [0u8; 16];
        getrandom::fill(&mut raw).map_err(|_| io::Error::other("随机数失败"))?;
        let token: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
        *self.pending.lock().map_err(|_| io::Error::other("锁失败"))? =
            Some((token.clone(), bytes.to_vec(), Instant::now()));
        Ok(token)
    }

    pub fn redeem(&self, token: &str, bytes: &[u8]) -> bool {
        let Ok(mut slot) = self.pending.lock() else {
            return false;
        };
        let Some((want, payload, at)) = slot.take() else {
            return false;
        };
        want == token && payload == bytes && at.elapsed() <= SYS_CONFIRM_TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("wanxiang-sys-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct MemTrash(Vec<PathBuf>);
    impl Trasher for MemTrash {
        fn trash(&mut self, path: &Path) -> Result<(), TrashFault> {
            self.0.push(path.to_path_buf());
            Ok(())
        }
    }

    fn write(path: &Path, len: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; len]).unwrap();
    }

    #[test]
    fn catalog_ids_are_unique_and_manual_items_are_not_cleanable() {
        let mut ids = HashSet::new();
        for item in CATALOG {
            assert!(ids.insert(item.id), "{}", item.id);
            if item.base == Base::Absolute {
                assert_eq!(item.action, Action::Manual, "{}", item.id);
            }
            for root in item.roots {
                if item.base == Base::Home {
                    assert!(!root.starts_with('/') && !root.contains(".."), "{root}");
                }
            }
        }
        for id in ["trash", "simulator_devices", "macos_installers", "swap", "tm_snapshots"] {
            let picks = [Pick { id: id.to_string(), children: None }];
            assert!(checked_picks(&picks).is_err(), "{id}");
        }
    }

    #[test]
    fn ai_and_system_caches_split_without_overlap() {
        let ai = item("ai_app_caches").unwrap();
        let sys = item("app_caches").unwrap();
        for name in ["com.anthropic.claudefordesktop", "com.openai.codex", "com.todesktop.230313mzl4w4u92.ShipIt"] {
            assert!(ai.filter.keeps(name) && !sys.filter.keeps(name), "{name}");
        }
        let logs = item("user_logs").unwrap();
        for name in ["Claude", "com.openai.codex", "Antigravity"] {
            assert!(ai.filter.keeps(name) && !logs.filter.keeps(name), "{name}");
        }
        assert!(logs.filter.keeps("DiagnosticReports"));
        for name in ["com.apple.Safari", "Homebrew", "pip"] {
            assert!(!ai.filter.keeps(name) && sys.filter.keeps(name), "{name}");
        }
    }

    #[test]
    fn picks_reject_paths_and_unknown_ids() {
        let bad = [Pick { id: "app_caches".into(), children: Some(vec!["../x".into()]) }];
        assert!(checked_picks(&bad).is_err());
        let bad = [Pick { id: "nope".into(), children: None }];
        assert!(checked_picks(&bad).is_err());
        let twice = [
            Pick { id: "user_logs".into(), children: None },
            Pick { id: "user_logs".into(), children: None },
        ];
        assert!(checked_picks(&twice).is_err());
        let a = pick_bytes(&[
            Pick { id: "user_logs".into(), children: None },
            Pick { id: "app_caches".into(), children: Some(vec!["b".into(), "a".into()]) },
        ]);
        let b = pick_bytes(&[
            Pick { id: "app_caches".into(), children: Some(vec!["a".into(), "b".into()]) },
            Pick { id: "user_logs".into(), children: None },
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn scan_measures_children_and_cleanup_moves_only_picked_ones() {
        let tmp = Tmp::new("scan");
        let home = &tmp.0;
        write(&home.join("Library/Logs/a/one.log"), 5000);
        write(&home.join("Library/Logs/b.log"), 3000);
        write(&home.join("Library/Caches/com.example.app/x"), 4000);
        write(&home.join("Library/Caches/com.openai.codex/y"), 4000);
        let probe = Probe {
            home,
            has_app: &|_| false,
            snapshots: &|| Some(2),
        };
        let scan = scan_all(&probe, &AtomicBool::new(false));
        assert!(scan.items.iter().all(|item| item.id != "docker_disk"));
        let logs = scan.items.iter().find(|item| item.id == "user_logs").unwrap();
        assert_eq!(logs.state, ItemState::Ok);
        assert_eq!(logs.count, 2);
        assert!(logs.bytes >= 8000);
        let caches = scan.items.iter().find(|item| item.id == "app_caches").unwrap();
        assert_eq!(caches.count, 1);
        assert_eq!(caches.children[0].name, "com.example.app");
        let ai = scan.items.iter().find(|item| item.id == "ai_app_caches").unwrap();
        assert_eq!(ai.children[0].name, "com.openai.codex");
        let snapshots = scan.items.iter().find(|item| item.id == "tm_snapshots").unwrap();
        assert_eq!(snapshots.snapshots, Some(2));
        let npm = scan.items.iter().find(|item| item.id == "npm_cache").unwrap();
        assert_eq!(npm.state, ItemState::Missing);

        let picks = checked_picks(&[Pick { id: "user_logs".into(), children: Some(vec!["b.log".into()]) }]).unwrap();
        let mut trash = MemTrash(Vec::new());
        let report = cleanup(home, &picks, Some(&[] as &[String]), &mut trash);
        assert_eq!(trash.0, vec![home.join("Library/Logs/b.log")]);
        assert_eq!(report.entry_count, 1);

        // 应用开着：它的缓存不动
        let picks = checked_picks(&[Pick { id: "app_caches".into(), children: None }]).unwrap();
        let mut trash = MemTrash(Vec::new());
        let running = vec!["com.example.app".to_string()];
        let report = cleanup(home, &picks, Some(running.as_slice()), &mut trash);
        assert!(trash.0.is_empty());
        assert_eq!(report.items[0].label, LABEL_RUNNING);
        // 看不到运行列表：缓存一律不动
        let report = cleanup(home, &picks, None, &mut trash);
        assert!(trash.0.is_empty());
        assert_eq!(report.items[0].label, LABEL_RUNNING);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_root_is_not_followed() {
        let tmp = Tmp::new("link");
        let home = tmp.0.join("home");
        let outside = tmp.0.join("outside");
        write(&outside.join("keep.txt"), 100);
        fs::create_dir_all(home.join("Library")).unwrap();
        std::os::unix::fs::symlink(&outside, home.join("Library/Logs")).unwrap();
        let picks = checked_picks(&[Pick { id: "user_logs".into(), children: None }]).unwrap();
        let mut trash = MemTrash(Vec::new());
        let report = cleanup(&home, &picks, Some(&[] as &[String]), &mut trash);
        assert!(trash.0.is_empty());
        assert_eq!(report.items[0].label, LABEL_GONE);
    }

    #[test]
    fn confirm_is_single_use_and_bound_to_the_picks() {
        let confirm = SysConfirm::default();
        let token = confirm.issue(b"a").unwrap();
        assert!(!confirm.redeem(&token, b"b"));
        let token = confirm.issue(b"a").unwrap();
        assert!(confirm.redeem(&token, b"a"));
        assert!(!confirm.redeem(&token, b"a"));
    }

    #[test]
    fn parses_df_and_tmutil() {
        let df = "Filesystem 1024-blocks Used Available Capacity iused ifree %iused Mounted on\n/dev/disk3s1s1 971350180 11000000 400000000 3% 1 2 0% /\n";
        let disk = parse_df(df).unwrap();
        assert_eq!(disk.total, 971350180 * 1024);
        assert_eq!(disk.available, 400000000 * 1024);
        let tm = "Snapshots for disk /:\ncom.apple.TimeMachine.2026-11-23-101010.local\ncom.apple.TimeMachine.2026-11-23-111010.local\n";
        assert_eq!(count_snapshots(tm), 2);
        assert_eq!(count_snapshots(""), 0);
    }

    #[test]
    fn running_match_is_by_bundle_prefix() {
        let running = vec!["com.example.app".to_string()];
        assert!(belongs_to_running("com.example.app", &running));
        assert!(belongs_to_running("com.example.app.ShipIt", &running));
        assert!(!belongs_to_running("com.example.application", &running));
    }
}
