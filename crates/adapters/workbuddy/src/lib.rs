//! WorkBuddy 接入：只读本机记录。
//!
//! - `~/.workbuddy/projects/<项目>/<会话>.jsonl`：会话记录（与 CodeBuddy 命令行同一格式：
//!   `type: "message"`、`role`、`content: [{type: "input_text" | "output_text", text}]`、
//!   `timestamp`（毫秒））。
//! - `~/.workbuddy/workbuddy.db`：会话表（标题、是否删除）与 `session_usage`（上下文已用与上限）。
//!
//! 不读 `keyblob`、`*.key`、`models.json`、`device-id`、`connectors/` 等任何凭据或账号文件；
//! 守卫按授权范围再挡一次，数据库只读打开。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use wanxiang_core::{Capability, Guard, GuardError, ToolId};

/// 会话记录。
pub const PROJECTS_PATTERN: &str = "projects/*/*.jsonl";
/// 会话表与上下文用量。
pub const DB_PATTERN: &str = "workbuddy.db*";

const MAX_FILE: u64 = 256 * 1024 * 1024;
const TITLE_CHARS: usize = 40;

pub fn config_root(home: &Path) -> PathBuf {
    home.join(".workbuddy")
}

pub fn installed(home: &Path, applications: &Path) -> bool {
    applications.join("WorkBuddy.app").is_dir() || config_root(home).is_dir()
}

/// 会话编号：UUID 形式。
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
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

pub fn find_session(config: &Path, id: &str) -> Option<PathBuf> {
    if !valid_id(id) {
        return None;
    }
    session_paths(config)
        .into_iter()
        .find(|path| path.file_stem().is_some_and(|stem| stem.to_string_lossy() == id))
}

/// 经守卫读取一个会话文件。
pub fn read_file(guard: &Guard, capability: Capability, path: &Path) -> Result<Vec<u8>, GuardError> {
    let mut opened = guard.open_read(ToolId::WorkBuddy, capability, path)?;
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WbSession {
    pub id: String,
    pub first_user: Option<String>,
    pub messages: usize,
    pub updated_ms: i64,
}

impl WbSession {
    pub fn title(&self) -> String {
        self.first_user.as_deref().map(short_title).unwrap_or_default()
    }
}

fn time_ms(value: &Value) -> Option<i64> {
    match value.get("timestamp")? {
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.timestamp_millis()),
        Value::Number(number) => number.as_i64().map(|n| if n < 100_000_000_000 { n * 1000 } else { n }),
        _ => None,
    }
}

fn injected(text: &str) -> bool {
    let text = text.trim_start();
    text.is_empty() || text.starts_with('<') || text.starts_with("[Request interrupted")
}

fn message_text(value: &Value) -> Option<String> {
    let parts: Vec<&str> = match value.get("content")? {
        Value::String(text) => vec![text.as_str()],
        Value::Array(items) => items
            .iter()
            // 实测格式：用户是 input_text，助手是 output_text；也接受 text
            .filter(|item| {
                matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("text" | "input_text" | "output_text")
                )
            })
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect(),
        _ => return None,
    };
    let text = parts.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn spoken(value: &Value) -> Option<(&str, String)> {
    if value.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = value.get("role").and_then(Value::as_str)?;
    if role != "user" && role != "assistant" {
        return None;
    }
    let text = message_text(value)?;
    if role == "user" && injected(&text) {
        return None;
    }
    Some((role, text))
}

/// 解析会话：首句、消息条数、最后时间。不保留其他正文。
pub fn parse_session(id: &str, bytes: &[u8]) -> WbSession {
    let text = String::from_utf8_lossy(bytes);
    let mut session = WbSession {
        id: id.to_string(),
        ..WbSession::default()
    };
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(at) = time_ms(&value) {
            session.updated_ms = session.updated_ms.max(at);
        }
        let Some((role, text)) = spoken(&value) else { continue };
        session.messages += 1;
        if role == "user" && session.first_user.is_none() {
            session.first_user = Some(text.chars().take(400).collect());
        }
    }
    session
}

/// 对话原文：「用户：」「助手：」块。
pub fn transcript(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut chunks: Vec<String> = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        let Some((role, text)) = spoken(&value) else { continue };
        let label = if role == "user" { "用户" } else { "助手" };
        let chunk = format!("{label}：{text}");
        if chunks.last() != Some(&chunk) {
            chunks.push(chunk);
        }
    }
    chunks
}

pub fn short_title(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(TITLE_CHARS).collect();
    if line.chars().count() > TITLE_CHARS {
        out.push('…');
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WbMeta {
    pub title: String,
    pub deleted: bool,
    /// 上下文（已用、上限）
    pub context: Option<(i64, i64)>,
}

fn io(_: rusqlite::Error) -> GuardError {
    GuardError::Io(std::io::ErrorKind::Other)
}

/// 会话表里的标题、删除标记，以及上下文用量。数据库不存在或读不懂时返回空表。
pub fn session_meta(guard: &Guard, config: &Path) -> Result<HashMap<String, WbMeta>, GuardError> {
    let path = config.join("workbuddy.db");
    let opened = guard.open_read(ToolId::WorkBuddy, Capability::Conversation, &path)?;
    drop(opened);
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(io)?;
    let _ = connection.busy_timeout(std::time::Duration::from_millis(1500));
    let mut out: HashMap<String, WbMeta> = HashMap::new();
    if let Ok(mut statement) =
        connection.prepare("SELECT id, title, custom_title, deleted_at FROM sessions")
    {
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, rusqlite::types::Value>(3)?,
            ))
        });
        if let Ok(rows) = rows {
            for (id, title, custom, deleted) in rows.flatten() {
                let Some(id) = id.filter(|id| valid_id(id)) else { continue };
                let title = custom
                    .filter(|text| !text.trim().is_empty())
                    .or(title)
                    .map(|text| short_title(&text))
                    .unwrap_or_default();
                let entry = out.entry(id).or_default();
                entry.title = title;
                entry.deleted = !matches!(deleted, rusqlite::types::Value::Null);
            }
        }
    }
    if let Ok(mut statement) = connection.prepare("SELECT session_id, used, size FROM session_usage")
    {
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        });
        if let Ok(rows) = rows {
            for (id, used, size) in rows.flatten() {
                let (Some(id), Some(used), Some(size)) = (id, used, size) else { continue };
                if valid_id(&id) && size > 0 && used >= 0 {
                    out.entry(id).or_default().context = Some((used, size));
                }
            }
        }
    }
    Ok(out)
}

pub fn percent(used: i64, limit: i64) -> Option<u32> {
    (limit > 0).then(|| ((used.max(0) * 100 + limit / 2) / limit).min(100) as u32)
}
