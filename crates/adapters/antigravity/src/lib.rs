//! Antigravity 接入：只读本机记录。
//!
//! - `~/.gemini/antigravity/conversation_summaries.db`：对话列表（标题、步数、最后修改时间、
//!   是否子代理），以及搜索用的分段原文（`conversations_fts_content`，用户与助手的文字）。
//! - `~/.gemini/antigravity/conversations/<对话>.db`：每一步的元数据（protobuf），
//!   其中带时间与 token 用量；最后一条生成记录里有上下文已用与上限。
//! - 命令行版 `~/.gemini/antigravity-cli/` 结构相同，一并读取。
//!
//! 不读登录令牌（`antigravity-oauth-token` 等）、`~/.gemini` 下其他目录；
//! 守卫按授权范围再挡一次，数据库一律只读打开。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use wanxiang_core::{Capability, Guard, GuardError, ToolId};

/// 对话列表与分段原文。
pub const SUMMARY_PATTERN: &str = "conversation_summaries.db*";
/// 每个对话的步骤库（用量与上下文）。
pub const CONVERSATIONS_PATTERN: &str = "conversations/*.db*";

const TITLE_CHARS: usize = 40;

/// 桌面应用与命令行版的数据目录（存在的才返回）。
pub fn data_roots(home: &Path) -> Vec<PathBuf> {
    [".gemini/antigravity", ".gemini/antigravity-cli"]
        .iter()
        .map(|rel| home.join(rel))
        .filter(|path| path.is_dir())
        .collect()
}

/// 空间统计看的目录：桌面应用的数据目录。
pub fn space_root(home: &Path) -> PathBuf {
    home.join(".gemini").join("antigravity")
}

pub fn installed(home: &Path, applications: &Path) -> bool {
    applications.join("Antigravity.app").is_dir() || !data_roots(home).is_empty()
}

/// 对话编号只允许 UUID 形式，防止拼出别的路径。
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn io(_: rusqlite::Error) -> GuardError {
    GuardError::Io(std::io::ErrorKind::Other)
}

/// 先过守卫，再只读打开数据库。正在被应用写入时退回到不加锁的只读快照。
fn open(guard: &Guard, capability: Capability, path: &Path) -> Result<Connection, GuardError> {
    let opened = guard.open_read(ToolId::Antigravity, capability, path)?;
    drop(opened);
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if let Ok(connection) = Connection::open_with_flags(path, flags) {
        let _ = connection.busy_timeout(std::time::Duration::from_millis(1500));
        if connection
            .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))
            .is_ok()
        {
            return Ok(connection);
        }
    }
    let uri = format!("file:{}?mode=ro&immutable=1", uri_path(path));
    Connection::open_with_flags(uri, flags | OpenFlags::SQLITE_OPEN_URI).map_err(io)
}

fn uri_path(path: &Path) -> String {
    let mut out = String::new();
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'%' | b'?' | b'#' | b' ' => out.push_str(&format!("%{byte:02X}")),
            _ => out.push(byte as char),
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgConversation {
    pub id: String,
    pub title: String,
    pub updated_ms: i64,
    pub steps: i64,
    /// 子代理（被主对话调起）的对话：列表里不单独显示
    pub nested: bool,
}

/// 读对话列表（只取标题、时间、步数、层级；没有标题时取预览的第一行当标题）。
pub fn list_conversations(
    guard: &Guard,
    capability: Capability,
    root: &Path,
) -> Result<Vec<AgConversation>, GuardError> {
    let connection = open(guard, capability, &root.join("conversation_summaries.db"))?;
    let mut statement = connection
        .prepare(
            "SELECT conversation_id, title, step_count, last_modified_time, nesting_depth, preview \
             FROM conversation_summaries",
        )
        .map_err(io)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, rusqlite::types::Value>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(io)?;
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (Some(id), title, steps, modified, depth, preview) = row else { continue };
        if !valid_id(&id) {
            continue;
        }
        out.push(AgConversation {
            id,
            // 没有标题时用预览（用户的第一句话）
            title: short_title(
                title
                    .as_deref()
                    .filter(|text| !text.trim().is_empty())
                    .or(preview.as_deref())
                    .unwrap_or(""),
            ),
            updated_ms: time_value_ms(&modified).unwrap_or(0),
            steps: steps.unwrap_or(0),
            nested: depth.unwrap_or(0) > 0,
        });
    }
    Ok(out)
}

/// 时间列可能是 `2026-11-11 04:46:38.13321`（UTC）、RFC 3339，或数字（秒、毫秒、纳秒）。
pub fn time_value_ms(value: &rusqlite::types::Value) -> Option<i64> {
    match value {
        rusqlite::types::Value::Integer(n) => Some(number_ms(*n)),
        rusqlite::types::Value::Real(n) => Some(number_ms(*n as i64)),
        rusqlite::types::Value::Text(text) => text_ms(text),
        _ => None,
    }
}

fn number_ms(n: i64) -> i64 {
    if n > 100_000_000_000_000 {
        n / 1_000_000
    } else if n > 100_000_000_000 {
        n
    } else {
        n * 1000
    }
}

pub fn text_ms(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(at.timestamp_millis());
    }
    // 去掉可能跟在后面的时区（如 " +0000 UTC"），按 UTC 解析
    let head: String = text.split(['+', 'Z']).next().unwrap_or(text).trim().replace('T', " ");
    let head = head.split(" UTC").next().unwrap_or(&head).trim().to_string();
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(at) = chrono::NaiveDateTime::parse_from_str(&head, format) {
            return Some(at.and_utc().timestamp_millis());
        }
    }
    None
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

/// 对话的分段原文：按段落顺序给出「用户：」「助手：」块。只读所选对话。
pub fn transcript(
    guard: &Guard,
    root: &Path,
    id: &str,
) -> Result<Option<Vec<String>>, GuardError> {
    if !valid_id(id) {
        return Ok(None);
    }
    let connection = open(guard, Capability::Conversation, &root.join("conversation_summaries.db"))?;
    // fts5 的内容表：c0 对话编号、c1 段号（-1 是摘要行）、c13 用户原文、c14 助手原文
    let mut statement = connection
        .prepare(
            "SELECT c1, c13, c14 FROM conversations_fts_content \
             WHERE c0 = ?1 AND c1 >= 0 ORDER BY c1",
        )
        .map_err(io)?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, rusqlite::types::Value>(1)?,
                row.get::<_, rusqlite::types::Value>(2)?,
            ))
        })
        .map_err(io)?;
    let mut chunks = Vec::new();
    let mut found = false;
    for (user, agent) in rows.flatten() {
        found = true;
        if let Some(text) = text_of(user).filter(|text| !text.trim().is_empty()) {
            chunks.push(format!("用户：{}", text.trim()));
        }
        if let Some(text) = text_of(agent).filter(|text| !text.trim().is_empty()) {
            chunks.push(format!("助手：{}", text.trim()));
        }
    }
    Ok(found.then_some(chunks))
}

fn text_of(value: rusqlite::types::Value) -> Option<String> {
    match value {
        rusqlite::types::Value::Text(text) => Some(text),
        rusqlite::types::Value::Blob(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        _ => None,
    }
}

/// 列出每个对话的步骤库（不读内容）。
pub fn conversation_paths(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("conversations")) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        let id_ok = path
            .file_stem()
            .map(|stem| valid_id(&stem.to_string_lossy()))
            .unwrap_or(false);
        if kind.is_file() && id_ok && path.extension().is_some_and(|ext| ext == "db") {
            found.push(path);
        }
    }
    found.sort();
    found
}

pub fn conversation_db(root: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| root.join("conversations").join(format!("{id}.db")))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgCall {
    pub time_ms: i64,
    pub tokens: i64,
}

/// 一个对话里每次模型调用的时间与 token（输入 + 缓存读取 + 输出）。
pub fn calls(guard: &Guard, capability: Capability, db: &Path) -> Result<Vec<AgCall>, GuardError> {
    let connection = open(guard, capability, db)?;
    let mut statement = connection.prepare("SELECT metadata FROM steps").map_err(io)?;
    let rows = statement
        .query_map([], |row| row.get::<_, Option<Vec<u8>>>(0))
        .map_err(io)?;
    let mut out = Vec::new();
    for metadata in rows.flatten().flatten() {
        if let Some(call) = step_call(&metadata) {
            out.push(call);
        }
    }
    Ok(out)
}

/// 步骤元数据：字段 1 是创建时间（秒、纳秒），字段 9 是这一步的模型用量
/// （2 输入、3 输出、5 缓存读取）。没有用量的步骤（用户输入、工具调用）返回 None。
pub fn step_call(metadata: &[u8]) -> Option<AgCall> {
    let fields = proto_fields(metadata)?;
    let usage = fields.iter().find_map(|(number, value)| match value {
        Proto::Bytes(bytes) if *number == 9 => proto_fields(bytes),
        _ => None,
    })?;
    let count = |want: u32| {
        usage
            .iter()
            .find_map(|(number, value)| match value {
                Proto::Varint(n) if *number == want => i64::try_from(*n).ok(),
                _ => None,
            })
            .unwrap_or(0)
    };
    let tokens = count(2) + count(3) + count(5);
    if tokens <= 0 {
        return None;
    }
    let created = fields.iter().find_map(|(number, value)| match value {
        Proto::Bytes(bytes) if *number == 1 => proto_fields(bytes),
        _ => None,
    })?;
    let seconds = created.iter().find_map(|(number, value)| match value {
        Proto::Varint(n) if *number == 1 => i64::try_from(*n).ok(),
        _ => None,
    })?;
    Some(AgCall {
        time_ms: seconds * 1000,
        tokens,
    })
}

/// 对话当前的上下文：最后一条生成记录里的（已用、上限）。
pub fn context(guard: &Guard, db: &Path) -> Result<Option<(i64, i64)>, GuardError> {
    let connection = open(guard, Capability::Conversation, db)?;
    let data = connection
        .query_row(
            "SELECT data FROM gen_metadata ORDER BY idx DESC LIMIT 1",
            [],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .ok()
        .flatten();
    Ok(data.as_deref().and_then(context_of))
}

/// 生成记录：1 → 9 → 10 里，字段 1 是已用 token，字段 4 是上下文上限。
pub fn context_of(data: &[u8]) -> Option<(i64, i64)> {
    let first = child(&proto_fields(data)?, 1)?;
    let nine = child(&first, 9)?;
    let window = child(&nine, 10)?;
    let number = |want: u32| {
        window.iter().find_map(|(number, value)| match value {
            Proto::Varint(n) if *number == want => i64::try_from(*n).ok(),
            _ => None,
        })
    };
    let used = number(1)?;
    let limit = number(4)?;
    (limit > 0 && used >= 0).then_some((used, limit))
}

fn child(fields: &[(u32, Proto<'_>)], want: u32) -> Option<Vec<(u32, Proto<'static>)>> {
    fields.iter().find_map(|(number, value)| {
        if *number != want {
            return None;
        }
        let bytes: &[u8] = match value {
            Proto::Bytes(bytes) => bytes,
            Proto::Owned(bytes) => bytes.as_slice(),
            Proto::Varint(_) => return None,
        };
        proto_fields(bytes).map(owned)
    })
}

fn owned(fields: Vec<(u32, Proto<'_>)>) -> Vec<(u32, Proto<'static>)> {
    fields
        .into_iter()
        .map(|(number, value)| {
            let value = match value {
                Proto::Varint(n) => Proto::Varint(n),
                Proto::Bytes(bytes) => Proto::Owned(bytes.to_vec()),
                Proto::Owned(bytes) => Proto::Owned(bytes),
            };
            (number, value)
        })
        .collect()
}

pub fn percent(used: i64, limit: i64) -> Option<u32> {
    (limit > 0).then(|| ((used.max(0) * 100 + limit / 2) / limit).min(100) as u32)
}

/// protobuf 的一个字段值：只区分数字与字节串（嵌套消息也是字节串）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proto<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Owned(Vec<u8>),
}

/// 按 protobuf 线格式拆出一层字段；格式不对返回 None。
pub fn proto_fields(buf: &[u8]) -> Option<Vec<(u32, Proto<'_>)>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < buf.len() {
        let (key, next) = varint(buf, at)?;
        at = next;
        let number = u32::try_from(key >> 3).ok()?;
        if number == 0 {
            return None;
        }
        match key & 7 {
            0 => {
                let (value, next) = varint(buf, at)?;
                at = next;
                out.push((number, Proto::Varint(value)));
            }
            1 => at = at.checked_add(8).filter(|end| *end <= buf.len())?,
            2 => {
                let (len, next) = varint(buf, at)?;
                let end = next.checked_add(usize::try_from(len).ok()?)?;
                if end > buf.len() {
                    return None;
                }
                out.push((number, Proto::Bytes(&buf[next..end])));
                at = end;
            }
            5 => at = at.checked_add(4).filter(|end| *end <= buf.len())?,
            _ => return None,
        }
    }
    Some(out)
}

fn varint(buf: &[u8], mut at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = *buf.get(at)?;
        at += 1;
        if shift >= 64 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Some((value, at));
        }
        shift += 7;
    }
}
