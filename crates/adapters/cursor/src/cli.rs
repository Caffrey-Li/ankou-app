//! Cursor 命令行（cursor-agent）对话：`~/.cursor/chats/<目录>/<对话编号>/`。
//!
//! 每个对话有 `meta.json`（标题、时间）与 `store.db`（`blobs` 表里存消息）。
//! 只读这两类文件；消息只取用户与助手的文字，跳过系统提示、工具输出与思考过程。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use wanxiang_core::{Capability, Guard, GuardError, ToolId};

/// 授权范围：对话元数据与消息库（含 WAL/SHM）。
pub const CLI_META_PATTERN: &str = "chats/*/*/meta.json";
pub const CLI_STORE_PATTERN: &str = "chats/*/*/store.db*";

pub fn cli_root(home: &Path) -> PathBuf {
    home.join(".cursor")
}

pub fn cli_installed(home: &Path) -> bool {
    home.join(".local").join("bin").join("cursor-agent").exists() || cli_root(home).join("chats").is_dir()
}

#[derive(Clone, Debug, PartialEq)]
pub struct CliChat {
    pub id: String,
    pub title: String,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub messages: usize,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Meta {
    created_at_ms: Option<i64>,
    updated_at_ms: Option<i64>,
    has_conversation: Option<bool>,
    title: Option<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// 列出对话目录（不读内容）：`chats/<目录>/<编号>/`。
fn chat_dirs(root: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let Ok(groups) = std::fs::read_dir(root.join("chats")) else {
        return found;
    };
    for group in groups.flatten() {
        if !group.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(chats) = std::fs::read_dir(group.path()) else { continue };
        for chat in chats.flatten() {
            if !chat.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let id = chat.file_name().to_string_lossy().into_owned();
            if valid_id(&id) {
                found.push((id, chat.path()));
            }
        }
    }
    found.sort();
    found
}

fn read_meta(guard: &Guard, dir: &Path) -> Result<Meta, GuardError> {
    let mut opened = guard.open_read(ToolId::CursorCli, Capability::Conversation, &dir.join("meta.json"))?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = opened.read(&mut chunk)?;
        if read == 0 || bytes.len() > 256 * 1024 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

fn open_store(guard: &Guard, dir: &Path) -> Result<Connection, GuardError> {
    let path = dir.join("store.db");
    let opened = guard.open_read(ToolId::CursorCli, Capability::Conversation, &path)?;
    drop(opened);
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| GuardError::Io(std::io::ErrorKind::Other))?;
    let _ = connection.busy_timeout(std::time::Duration::from_millis(1500));
    Ok(connection)
}

#[derive(Deserialize)]
struct Message {
    role: String,
    #[serde(default)]
    content: serde_json::Value,
}

/// 用户输入在命令行里包成 `<user_query>…</user_query>`（前面可能有 `<timestamp>`）；
/// 取出里面的话。其余以 `<` 开头的是程序附带的上下文（工作区、技能列表、提醒），丢弃。
fn user_part(text: &str) -> Option<&str> {
    let text = text.trim();
    if let Some(start) = text.find("<user_query>") {
        let body = &text[start + "<user_query>".len()..];
        let body = body.find("</user_query>").map_or(body, |end| &body[..end]);
        let body = body.trim();
        return (!body.is_empty()).then_some(body);
    }
    (!text.is_empty() && !text.starts_with('<')).then_some(text)
}

/// 一条消息里的文字：字符串，或 `{type:"text", text}` 列表；其余类型（思考、工具调用）丢弃。
fn message_text(content: &serde_json::Value, user: bool) -> String {
    let pick = |text: &str| -> Option<String> {
        if user {
            user_part(text).map(str::to_string)
        } else {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
    };
    match content {
        serde_json::Value::String(text) => pick(text).unwrap_or_default(),
        serde_json::Value::Array(items) => items
            .iter()
            .filter(|item| item.get("type").and_then(|kind| kind.as_str()) == Some("text"))
            .filter_map(|item| item.get("text").and_then(|text| text.as_str()))
            .filter_map(pick)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// 按写入顺序取出用户与助手的文字。
pub fn parse_messages(rows: impl IntoIterator<Item = Vec<u8>>) -> Vec<(bool, String)> {
    let mut turns = Vec::new();
    for data in rows {
        let Ok(message) = serde_json::from_slice::<Message>(&data) else { continue };
        let user = match message.role.as_str() {
            "user" => true,
            "assistant" => false,
            _ => continue,
        };
        let text = message_text(&message.content, user);
        if text.is_empty() {
            continue;
        }
        turns.push((user, text));
    }
    turns
}

fn turns(connection: &Connection) -> Result<Vec<(bool, String)>, GuardError> {
    // 有的对话库还没有消息表，当作没有消息
    let has_blobs: bool = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'blobs'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);
    if !has_blobs {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare("SELECT data FROM blobs ORDER BY rowid")
        .map_err(|_| GuardError::Io(std::io::ErrorKind::Other))?;
    let rows = statement
        .query_map([], |row| row.get::<_, rusqlite::types::Value>(0))
        .map_err(|_| GuardError::Io(std::io::ErrorKind::Other))?;
    let data = rows.flatten().filter_map(|value| match value {
        rusqlite::types::Value::Blob(bytes) if bytes.first() == Some(&b'{') => Some(bytes),
        rusqlite::types::Value::Text(text) if text.starts_with('{') => Some(text.into_bytes()),
        _ => None,
    });
    Ok(parse_messages(data))
}

fn short_title(text: &str) -> String {
    let line = text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    let mut title: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        title.push('…');
    }
    title
}

/// 列出有文字消息的对话，最新的在前。
pub fn list_cli_chats(guard: &Guard, root: &Path) -> Result<Vec<CliChat>, GuardError> {
    let mut chats = Vec::new();
    let mut denied = None;
    for (id, dir) in chat_dirs(root) {
        let meta = match read_meta(guard, &dir) {
            Ok(meta) => meta,
            Err(GuardError::NotGranted) => return Err(GuardError::NotGranted),
            Err(error) => {
                denied = Some(error);
                continue;
            }
        };
        if meta.has_conversation == Some(false) || !dir.join("store.db").is_file() {
            continue;
        }
        let Ok(connection) = open_store(guard, &dir) else { continue };
        let list = turns(&connection)?;
        if list.is_empty() {
            continue;
        }
        let title = meta
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
            .or_else(|| list.iter().find(|(user, _)| *user).map(|(_, text)| short_title(text)))
            .unwrap_or_default();
        let created_ms = meta.created_at_ms.unwrap_or(0);
        chats.push(CliChat {
            id,
            title,
            created_ms,
            updated_ms: meta.updated_at_ms.unwrap_or(created_ms),
            messages: list.len(),
        });
    }
    if chats.is_empty() {
        if let Some(error) = denied {
            return Err(error);
        }
    }
    chats.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    Ok(chats)
}

/// 交接用的正文：「用户：…」「助手：…」。
pub fn cli_chunks(guard: &Guard, root: &Path, id: &str) -> Result<Option<Vec<String>>, GuardError> {
    if !valid_id(id) {
        return Ok(None);
    }
    let Some((_, dir)) = chat_dirs(root).into_iter().find(|(found, _)| found == id) else {
        return Ok(None);
    };
    let connection = open_store(guard, &dir)?;
    Ok(Some(
        turns(&connection)?
            .into_iter()
            .map(|(user, text)| format!("{}：{text}", if user { "用户" } else { "助手" }))
            .collect(),
    ))
}
