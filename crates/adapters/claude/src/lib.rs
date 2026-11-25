//! Claude 接入：只读本机记录。
//!
//! - `~/.claude/projects/<项目>/<会话>.jsonl`：Claude Code（含 Claude 桌面应用里的 Code）的会话记录，
//!   每条回复带 usage；用量只读数字，交接时才读所选会话的文字。
//! - 桌面应用数据目录下的 `plan-usage-history.json`：桌面应用记下的订阅额度百分比
//!   （5 小时窗口 `fh`、每周 `sd`），只取最新一条。
//! - 桌面应用数据目录下的 `claude-code-sessions/**/<会话>.json`：桌面应用给会话起的标题。
//!
//! 不读 `~/.claude.json`（账号信息）、登录凭据文件、钥匙串；守卫按授权范围再挡一次。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wanxiang_core::{Capability, Guard, GuardError, ToolId};

/// `~/.claude` 下的会话记录。
pub const PROJECTS_PATTERN: &str = "projects/*/*.jsonl";
/// 桌面应用目录下的额度历史。
pub const PLAN_PATTERN: &str = "plan-usage-history.json";
/// 桌面应用目录下的会话标题。
pub const DESKTOP_SESSIONS_PATTERN: &str = "claude-code-sessions/**/*.json";

const MAX_FILE: u64 = 256 * 1024 * 1024;
const TITLE_CHARS: usize = 40;

pub fn config_root(home: &Path) -> PathBuf {
    home.join(".claude")
}

pub fn support_root(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Claude");
        }
        return home.join("AppData").join("Roaming").join("Claude");
    }
    #[cfg(not(target_os = "windows"))]
    {
        home.join("Library")
            .join(format!("{} {}", "Application", "Support"))
            .join("Claude")
    }
}

pub fn installed(home: &Path, applications: &Path) -> bool {
    config_root(home).is_dir()
        || support_root(home).is_dir()
        || applications.join("Claude.app").is_dir()
}

/// 列出会话记录文件（不读内容）。
pub fn session_paths(config: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(projects) = std::fs::read_dir(config.join("projects")) else {
        return found;
    };
    for project in projects.flatten() {
        let Ok(kind) = project.file_type() else { continue };
        if !kind.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(project.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            let Ok(kind) = file.file_type() else { continue };
            if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// 列出桌面应用的会话标题文件（不读内容）。
pub fn desktop_session_paths(support: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk_json(&support.join("claude-code-sessions"), 0, &mut found);
    found.sort();
    found
}

fn walk_json(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            walk_json(&path, depth + 1, found);
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            found.push(path);
        }
    }
}

/// 经守卫读取一个文件：没有授权或路径不在范围内就拒绝。
pub fn read_file(
    guard: &Guard,
    capability: Capability,
    path: &Path,
) -> Result<Vec<u8>, GuardError> {
    let mut opened = guard.open_read(ToolId::Claude, capability, path)?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = opened.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() as u64 > MAX_FILE {
            break;
        }
    }
    Ok(raw)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeCall {
    pub time_ms: i64,
    pub tokens: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeSession {
    pub id: String,
    /// 会话里 `custom-title` 记下的标题
    pub custom_title: Option<String>,
    /// 第一句用户输入（跳过系统注入的文字），只在没有标题时用
    pub first_user: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// 每次模型调用（按回复编号去重）
    pub calls: Vec<ClaudeCall>,
    /// 最后一次调用的上下文 token（输入 + 缓存 + 输出）
    pub last_context: Option<i64>,
}

impl ClaudeSession {
    pub fn title(&self) -> String {
        if let Some(title) = self.custom_title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            return title.to_string();
        }
        self.first_user.as_deref().map(short_title).unwrap_or_default()
    }
}

pub fn short_title(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    let mut title: String = line.chars().take(TITLE_CHARS).collect();
    if line.chars().count() > TITLE_CHARS {
        title.push('…');
    }
    title
}

fn time_ms(value: &Value) -> Option<i64> {
    let text = value.get("timestamp")?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|at| at.timestamp_millis())
}

fn usage_tokens(usage: &Value) -> i64 {
    [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
    ]
    .iter()
    .filter_map(|key| usage.get(*key).and_then(Value::as_i64))
    .sum()
}

/// 系统、命令或提醒注入的文字，不算用户说的话。
fn injected(text: &str) -> bool {
    let text = text.trim_start();
    text.is_empty()
        || text.starts_with('<')
        || text.starts_with("Caveat:")
        || text.starts_with("This session is being continued")
        || text.starts_with("[Request interrupted")
        || text.starts_with("[auto-continue]")
}

/// 去掉桌面版包在用户消息外面的标记：只留 <message> 里的正文，去掉开头的 @文件 引用。
/// 不是人发的消息（from 不是 human）返回 None。
fn clean_user(text: &str) -> Option<String> {
    let mut body = text;
    if let Some(start) = text.rfind("<message") {
        let rest = &text[start..];
        let open_end = rest.find('>')?;
        let tag = &rest[..open_end];
        if let Some(from) = tag.find("from=\"") {
            let value = &tag[from + 6..];
            if !value.starts_with("human\"") {
                return None;
            }
        }
        let inner = &rest[open_end + 1..];
        body = match inner.find("</message>") {
            Some(end) => &inner[..end],
            None => inner,
        };
    }
    // 没有 <message> 包装、整段以标记开头的是系统注入（命令输出、提醒等）
    if injected(body) {
        return None;
    }
    // 附件等只有标记的行换成占位
    let lines: Vec<String> = body
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("<attachment") {
                "[附件]".to_string()
            } else if trimmed.starts_with('<') && trimmed.ends_with('>') {
                String::new()
            } else {
                line.to_string()
            }
        })
        .collect();
    let joined = lines.join("\n");
    let body = joined.as_str();
    let kept: Vec<&str> = body
        .split_whitespace()
        .filter(|word| !(word.starts_with("@/") || word.starts_with("@~")))
        .collect();
    let cleaned = if kept.len() == body.split_whitespace().count() {
        body.trim().to_string()
    } else {
        // 有 @ 引用时按行去掉，保留其他行原样
        body.lines()
            .map(|line| {
                line.split(' ')
                    .filter(|word| !(word.starts_with("@/") || word.starts_with("@~")))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    };
    (!injected(&cleaned)).then_some(cleaned)
}

/// 用户消息里的文字（字符串或 text 块；工具结果不算）。
fn user_text(message: &Value) -> Option<String> {
    match message.get("content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts: Vec<&str> = items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        _ => None,
    }
}

/// 助手消息里的文字块（不含思考过程和工具调用）。
fn assistant_text(message: &Value) -> Option<String> {
    let items = message.get("content")?.as_array()?;
    let parts: Vec<&str> = items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn is_true(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}

/// 解析一个会话文件：用量、时间、标题。不保留对话正文（第一句用户输入除外，只用于标题）。
pub fn parse_session(id: &str, bytes: &[u8]) -> ClaudeSession {
    let text = String::from_utf8_lossy(bytes);
    let mut session = ClaudeSession {
        id: id.to_string(),
        ..ClaudeSession::default()
    };
    let mut calls: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut last: Option<(i64, i64)> = None;
    let mut unnamed = 0_u64;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        let at = time_ms(&value);
        if let Some(at) = at {
            if session.created_ms == 0 || at < session.created_ms {
                session.created_ms = at;
            }
            session.updated_ms = session.updated_ms.max(at);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("custom-title") => {
                if let Some(title) = value.get("customTitle").and_then(Value::as_str) {
                    session.custom_title = Some(title.to_string());
                }
            }
            Some("user") if session.first_user.is_none() => {
                if is_true(&value, "isMeta") || is_true(&value, "isSidechain") {
                    continue;
                }
                if let Some(text) = value.get("message").and_then(user_text).as_deref().and_then(clean_user) {
                    session.first_user = Some(text.chars().take(400).collect());
                }
            }
            Some("assistant") => {
                let Some(message) = value.get("message") else { continue };
                if message.get("model").and_then(Value::as_str) == Some("<synthetic>") {
                    continue;
                }
                let Some(usage) = message.get("usage") else { continue };
                let tokens = usage_tokens(usage);
                let at = at.unwrap_or(session.updated_ms);
                let key = match message.get("id").and_then(Value::as_str) {
                    Some(id) => id.to_string(),
                    None => {
                        unnamed += 1;
                        format!("#{unnamed}")
                    }
                };
                // 同一条回复按内容块拆成多行，usage 相同：按编号只算一次，取最后一行
                calls.insert(key, (at, tokens));
                if !is_true(&value, "isSidechain") {
                    last = Some((at, tokens));
                }
            }
            _ => {}
        }
    }
    session.calls = calls
        .into_values()
        .filter(|(_, tokens)| *tokens > 0)
        .map(|(time_ms, tokens)| ClaudeCall { time_ms, tokens })
        .collect();
    session.calls.sort_by_key(|call| call.time_ms);
    session.last_context = last.map(|(_, tokens)| tokens);
    session
}

/// 交接用的正文：「用户：…」「助手：…」，只含文字；不含思考、工具调用和工具结果。
pub fn transcript(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut chunks: Vec<String> = Vec::new();
    let mut last_reply: Option<String> = None;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        if is_true(&value, "isSidechain") || is_true(&value, "isMeta") {
            continue;
        }
        let Some(message) = value.get("message") else { continue };
        match value.get("type").and_then(Value::as_str) {
            Some("user") => {
                if let Some(text) = user_text(message).as_deref().and_then(clean_user) {
                    chunks.push(format!("用户：{text}"));
                    last_reply = None;
                }
            }
            Some("assistant") => {
                if message.get("model").and_then(Value::as_str) == Some("<synthetic>") {
                    continue;
                }
                let Some(text) = assistant_text(message) else { continue };
                // 同一条回复拆成多行时，文字块会重复出现，跳过完全相同的
                if last_reply.as_deref() == Some(text.as_str()) {
                    continue;
                }
                chunks.push(format!("助手：{text}"));
                last_reply = Some(text);
            }
            _ => {}
        }
    }
    chunks
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanSample {
    pub at_ms: i64,
    pub five_hour: Option<i32>,
    pub seven_day: Option<i32>,
}

/// 额度历史里最新一条有数据的记录。
pub fn latest_plan(bytes: &[u8]) -> Option<PlanSample> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let samples = value.get("samples")?.as_array()?;
    samples
        .iter()
        .filter_map(|sample| {
            let at_ms = sample.get("t")?.as_i64()?;
            let usage = sample.get("u")?.as_object()?;
            let percent = |key: &str| {
                usage
                    .get(key)
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value.clamp(0, 100)).ok())
            };
            let five_hour = percent("fh");
            let seven_day = percent("sd");
            (five_hour.is_some() || seven_day.is_some()).then_some(PlanSample {
                at_ms,
                five_hour,
                seven_day,
            })
        })
        .max_by_key(|sample| sample.at_ms)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSession {
    /// 对应 `projects/*/<cli_session_id>.jsonl`
    pub cli_session_id: String,
    pub title: String,
    pub archived: bool,
}

/// 桌面应用的会话标题文件：只取会话编号、标题和是否归档。
pub fn desktop_session(bytes: &[u8]) -> Option<DesktopSession> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let cli_session_id = value.get("cliSessionId")?.as_str()?.to_string();
    Some(DesktopSession {
        cli_session_id,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        archived: value.get("isArchived").and_then(Value::as_bool) == Some(true),
    })
}

/// 会话编号只允许十六进制与连字符，防止拼出别的路径。
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// 按编号找会话文件（只在授权范围内的会话记录里找）。
pub fn find_session(config: &Path, id: &str) -> Option<PathBuf> {
    if !valid_id(id) {
        return None;
    }
    let name = format!("{id}.jsonl");
    session_paths(config)
        .into_iter()
        .find(|path| path.file_name().is_some_and(|file| file == name.as_str()))
}

/// 官方深链：在新终端窗口打开 Claude Code，提示词预填、不发送。最多 5000 字。
pub const DEEPLINK_PREFIX: &str = "claude-cli://open?q=";
pub const DEEPLINK_MAX_CHARS: usize = 5000;

pub fn deeplink(text: &str) -> Option<String> {
    if text.chars().count() > DEEPLINK_MAX_CHARS {
        return None;
    }
    let mut out = String::from(DEEPLINK_PREFIX);
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    Some(out)
}
