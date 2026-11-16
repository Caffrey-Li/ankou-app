//! Cursor 对话：只读 globalStorage/state.vscdb 里与对话有关的三类键。
//!
//! 白名单：`composerData:<id>`、`composerConvHeaders:<id>`、`bubbleId:<id>:<id>`。
//! 不碰 ItemTable（登录信息在那里），也不读 agentKv 等其他键。
//! 解析时只取下面结构体里列出的字段，其余字段（包括加密密钥类字段）一律丢弃。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use wanxiang_core::{classify_cursor_global_key, Capability, Guard, GuardError, ToolId};

/// 授权范围：数据库本体与它的 WAL/SHM。
pub const STATE_PATTERN: &str = "User/globalStorage/state.vscdb*";

pub fn state_db(root: &Path) -> PathBuf {
    root.join("User").join("globalStorage").join("state.vscdb")
}

#[derive(Clone, Debug, PartialEq)]
pub struct CursorChat {
    pub id: String,
    pub title: String,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub percent: Option<u32>,
    /// 消息条数（有消息头时只算有文字的，否则是气泡总数，含工具调用）
    pub messages: usize,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ComposerMeta {
    composer_id: String,
    name: Option<String>,
    created_at: Option<i64>,
    last_updated_at: Option<i64>,
    context_usage_percent: Option<f64>,
    context_tokens_used: Option<i64>,
    context_token_limit: Option<i64>,
    subagent_info: Option<serde::de::IgnoredAny>,
    is_draft: Option<bool>,
}

#[derive(Deserialize)]
struct Header {
    #[serde(rename = "bubbleId")]
    bubble_id: String,
    #[serde(rename = "type")]
    kind: i64,
    #[serde(default)]
    grouping: Option<Grouping>,
}

#[derive(Deserialize)]
struct Grouping {
    #[serde(rename = "hasText", default)]
    has_text: Option<bool>,
}

impl Header {
    fn spoken(&self) -> bool {
        (self.kind == 1 || self.kind == 2)
            && self
                .grouping
                .as_ref()
                .and_then(|grouping| grouping.has_text)
                .unwrap_or(true)
    }
}

/// 对话的消息头（顺序与是否有文字），没有或读不懂时返回 None。
fn headers(connection: &Connection, id: &str) -> Result<Option<Vec<Header>>, GuardError> {
    let Some(raw) = value(connection, &format!("composerConvHeaders:{id}"))? else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<Vec<Header>>(&raw)
        .ok()
        .filter(|list| !list.is_empty()))
}

fn bubble_text(connection: &Connection, id: &str, bubble: &str) -> Result<Option<(i64, String)>, GuardError> {
    if !valid_id(bubble) {
        return Ok(None);
    }
    let Some(raw) = value(connection, &format!("bubbleId:{id}:{bubble}"))? else {
        return Ok(None);
    };
    let Ok(parsed) = serde_json::from_str::<Bubble>(&raw) else {
        return Ok(None);
    };
    let text = parsed.text();
    Ok((!text.is_empty()).then(|| (parsed.kind(), text.to_string())))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Bubble {
    #[serde(rename = "type")]
    kind: Option<i64>,
    text: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: serde_json::Value,
}

impl Bubble {
    fn kind(&self) -> i64 {
        self.kind.unwrap_or(0)
    }
    fn text(&self) -> &str {
        self.text.as_deref().unwrap_or("").trim()
    }
    /// 新版是 ISO 时间字符串，旧版可能是毫秒数；统一成可排序的键。
    fn order_key(&self) -> (i64, String) {
        match &self.created_at {
            serde_json::Value::Number(n) => (n.as_i64().unwrap_or(0), String::new()),
            serde_json::Value::String(s) => (0, s.clone()),
            _ => (0, String::new()),
        }
    }
}

/// 数据库里的值可能是文本也可能是字节，统一转成文本。
fn text_of(value: rusqlite::types::Value) -> Option<String> {
    match value {
        rusqlite::types::Value::Text(text) => Some(text),
        rusqlite::types::Value::Blob(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        _ => None,
    }
}

/// 只允许 Cursor 自己生成的编号格式，防止拼出白名单外的键。
pub fn valid_id(id: &str) -> bool {
    let body = id.strip_prefix("bc-").unwrap_or(id);
    !body.is_empty()
        && body.len() <= 64
        && body.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn open(guard: &Guard, root: &Path) -> Result<Connection, GuardError> {
    let path = state_db(root);
    // 先过守卫：没有授权或路径不在范围内就不打开
    let opened = guard.open_read(ToolId::Cursor, Capability::Conversation, &path)?;
    drop(opened);
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| GuardError::Io(std::io::ErrorKind::Other))?;
    let _ = connection.busy_timeout(std::time::Duration::from_millis(1500));
    Ok(connection)
}

fn io(_: rusqlite::Error) -> GuardError {
    GuardError::Io(std::io::ErrorKind::Other)
}

/// 只放行三类对话键；凭据键由核心的分类再挡一次。
fn allowed(key: &str) -> Result<(), GuardError> {
    classify_cursor_global_key(key)?;
    if ["composerData:", "composerConvHeaders:", "bubbleId:"]
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        Ok(())
    } else {
        Err(GuardError::NotGranted)
    }
}

fn value(connection: &Connection, key: &str) -> Result<Option<String>, GuardError> {
    allowed(key)?;
    connection
        .query_row("SELECT value FROM cursorDiskKV WHERE key = ?1", [key], |row| {
            row.get::<_, rusqlite::types::Value>(0)
        })
        .optional()
        .map(|found| found.and_then(text_of))
        .map_err(io)
}

fn range(connection: &Connection, prefix: &str, with_values: bool) -> Result<Vec<(String, Option<String>)>, GuardError> {
    allowed(prefix)?;
    let mut end = prefix.to_string();
    end.pop();
    end.push(';'); // ':' 的下一个字符，键前缀用区间查询，走索引
    let sql = if with_values {
        "SELECT key, value FROM cursorDiskKV WHERE key >= ?1 AND key < ?2"
    } else {
        "SELECT key, NULL FROM cursorDiskKV WHERE key >= ?1 AND key < ?2"
    };
    let mut statement = connection.prepare(sql).map_err(io)?;
    let rows = statement
        .query_map([prefix, end.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, rusqlite::types::Value>(1)?,
            ))
        })
        .map_err(io)?;
    let out: Vec<(String, Option<String>)> = rows
        .filter_map(Result::ok)
        .map(|(key, value)| (key, text_of(value)))
        .collect();
    Ok(out)
}

fn percent(meta: &ComposerMeta) -> Option<u32> {
    if let (Some(used), Some(limit)) = (meta.context_tokens_used, meta.context_token_limit) {
        if used >= 0 && limit > 0 {
            let value = (used.saturating_mul(100) + limit / 2) / limit;
            return u32::try_from(value).ok().map(|v| v.min(100));
        }
    }
    let raw = meta.context_usage_percent?;
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    Some(raw.round().min(100.0) as u32)
}

/// 一条对话的消息顺序：优先用 composerConvHeaders，没有时按气泡创建时间排。
fn ordered(connection: &Connection, id: &str) -> Result<Vec<(i64, String)>, GuardError> {
    if let Some(list) = headers(connection, id)? {
        let mut out = Vec::new();
        for header in list.iter().filter(|header| header.kind == 1 || header.kind == 2) {
            if let Some((_, text)) = bubble_text(connection, id, &header.bubble_id)? {
                out.push((header.kind, text));
            }
        }
        return Ok(out);
    }
    let mut bubbles: Vec<Bubble> = range(connection, &format!("bubbleId:{id}:"), true)?
        .into_iter()
        .filter_map(|(_, raw)| raw)
        .filter_map(|raw| serde_json::from_str::<Bubble>(&raw).ok())
        .filter(|bubble| (bubble.kind() == 1 || bubble.kind() == 2) && !bubble.text().is_empty())
        .collect();
    bubbles.sort_by_key(Bubble::order_key);
    Ok(bubbles
        .iter()
        .map(|bubble| (bubble.kind(), bubble.text().to_string()))
        .collect())
}

/// 只为列表用：在数据库里数文字消息，不把整段气泡读进来。
fn bubble_end(prefix: &str) -> String {
    let mut end = prefix.to_string();
    end.pop();
    end.push(';');
    end
}

const BUBBLE_ROWS: &str = "SELECT v, CASE WHEN json_valid(v) THEN json_extract(v, '$.type') END AS kind,
        CASE WHEN json_valid(v) THEN trim(coalesce(json_extract(v, '$.text'), '')) ELSE '' END AS body,
        CASE WHEN json_valid(v) THEN json_extract(v, '$.createdAt') END AS created
    FROM (SELECT CAST(value AS TEXT) AS v FROM cursorDiskKV WHERE key >= ?1 AND key < ?2)";

/// 只数键，不读值（走索引，很快）；用来判断对话里有没有消息。
fn count_bubbles(connection: &Connection, id: &str) -> Result<usize, GuardError> {
    let prefix = format!("bubbleId:{id}:");
    allowed(&prefix)?;
    connection
        .query_row(
            "SELECT COUNT(key) FROM cursorDiskKV WHERE key >= ?1 AND key < ?2",
            [prefix.as_str(), bubble_end(&prefix).as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| usize::try_from(count).unwrap_or(0))
        .map_err(io)
}

fn first_user_text(connection: &Connection, id: &str) -> Result<Option<String>, GuardError> {
    let prefix = format!("bubbleId:{id}:");
    allowed(&prefix)?;
    let sql = format!(
        "SELECT body FROM ({BUBBLE_ROWS}) WHERE kind = 1 AND length(body) > 0 ORDER BY created LIMIT 1"
    );
    connection
        .query_row(&sql, [prefix.as_str(), bubble_end(&prefix).as_str()], |row| row.get::<_, String>(0))
        .optional()
        .map_err(io)
}

fn short_title(text: &str) -> String {
    let line = text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    let mut title: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        title.push('…');
    }
    title
}

/// 列出有文字消息的对话，最新的在前。只读元数据与消息条数。
pub fn list_chats(guard: &Guard, root: &Path) -> Result<Vec<CursorChat>, GuardError> {
    let connection = open(guard, root)?;
    let mut chats = Vec::new();
    for (key, raw) in range(&connection, "composerData:", true)? {
        let id = key.trim_start_matches("composerData:");
        if !valid_id(id) {
            continue;
        }
        let Some(raw) = raw else { continue };
        let Ok(meta) = serde_json::from_str::<ComposerMeta>(&raw) else { continue };
        drop(raw);
        if meta.subagent_info.is_some() || meta.is_draft == Some(true) {
            continue;
        }
        // 有消息头时只看消息头（很小）；没有时才在数据库里数气泡
        let list = headers(&connection, id)?;
        let messages = match &list {
            Some(list) => list.iter().filter(|header| header.spoken()).count(),
            None => count_bubbles(&connection, id)?,
        };
        if messages == 0 {
            continue;
        }
        let name = meta.name.as_deref().map(str::trim).unwrap_or("");
        let title = if !name.is_empty() {
            name.to_string()
        } else {
            let first = match &list {
                Some(list) => {
                    let mut found = None;
                    for header in list.iter().filter(|header| header.kind == 1) {
                        if let Some((_, text)) = bubble_text(&connection, id, &header.bubble_id)? {
                            found = Some(text);
                            break;
                        }
                    }
                    found
                }
                None => first_user_text(&connection, id)?,
            };
            first.map(|text| short_title(&text)).unwrap_or_default()
        };
        let created_ms = meta.created_at.unwrap_or(0);
        chats.push(CursorChat {
            id: if meta.composer_id.is_empty() { id.to_string() } else { meta.composer_id.clone() },
            title,
            created_ms,
            updated_ms: meta.last_updated_at.unwrap_or(created_ms),
            percent: percent(&meta),
            messages,
        });
    }
    chats.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    Ok(chats)
}

/// 交接用的正文：「用户：…」「助手：…」，只含文字，不含工具输出与思考过程。
pub fn chat_chunks(guard: &Guard, root: &Path, id: &str) -> Result<Option<Vec<String>>, GuardError> {
    if !valid_id(id) {
        return Ok(None);
    }
    let connection = open(guard, root)?;
    if value(&connection, &format!("composerData:{id}"))?.is_none() {
        return Ok(None);
    }
    let turns = ordered(&connection, id)?;
    Ok(Some(
        turns
            .into_iter()
            .map(|(kind, text)| format!("{}：{text}", if kind == 1 { "用户" } else { "助手" }))
            .collect(),
    ))
}
