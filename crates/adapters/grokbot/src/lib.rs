//! Grok Bot 接入：只读应用自己的本地缓存 `sand-client-persistence/*.blob`。
//!
//! 每个缓存文件名是键名的 base32（小写、不补等号），内容是 JSON `{schemaVersion, value}`。
//! 只打开三类键，其余（草稿、连接的应用、账号设置等）一律不打开：
//! - `….roster.last-roster`：会话列表（编号、名字、最后活动时间）。
//! - `….transcript.replicas.<编号>`：该会话最近的消息（应用只缓存最近一部分）。
//! - `….weekly-usage.cache`：每周额度百分比与重置时间。
//!
//! 键名里带账号编号，只用来匹配，不显示、不记录。守卫按授权范围再挡一次。

use std::path::{Path, PathBuf};

use serde_json::Value;
use wanxiang_core::{Capability, Guard, GuardError, ToolId};

/// 本地缓存文件。
pub const PERSIST_PATTERN: &str = "sand-client-persistence/*.blob";

const MAX_FILE: u64 = 32 * 1024 * 1024;
const TITLE_CHARS: usize = 40;

pub fn support_root(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Grok Bot");
        }
        return home.join("AppData").join("Roaming").join("Grok Bot");
    }
    #[cfg(not(target_os = "windows"))]
    {
        home.join("Library")
            .join(format!("{} {}", "Application", "Support"))
            .join("Grok Bot")
    }
}

pub fn installed(home: &Path, applications: &Path) -> bool {
    applications.join("Grok Bot.app").is_dir() || support_root(home).is_dir()
}

/// 会话编号：UUID 形式。
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// 文件名（去掉 .blob）按小写 base32 解成键名；不是合法 base32 或不是 UTF-8 时返回 None。
pub fn decode_key(stem: &str) -> Option<String> {
    let mut bits = 0u32;
    let mut count = 0u32;
    let mut out = Vec::new();
    for byte in stem.bytes() {
        let value = match byte {
            b'a'..=b'z' => byte - b'a',
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            b'=' => break,
            _ => return None,
        };
        bits = (bits << 5) | u32::from(value);
        count += 5;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 键名的种类：只有这三类会被打开。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slice {
    Roster,
    Transcript(String),
    Usage,
}

pub fn classify(key: &str) -> Option<Slice> {
    if !key.starts_with("sand.client.slice.") {
        return None;
    }
    if key.ends_with(".roster.last-roster") {
        return Some(Slice::Roster);
    }
    if key.ends_with(".weekly-usage.cache") {
        return Some(Slice::Usage);
    }
    let (_, id) = key.rsplit_once(".transcript.replicas.")?;
    valid_id(id).then(|| Slice::Transcript(id.to_string()))
}

/// 缓存目录里每个认得的文件（不读内容）。
pub fn slices(root: &Path) -> Vec<(Slice, PathBuf)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("sand-client-persistence")) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if !kind.is_file() || !path.extension().is_some_and(|ext| ext == "blob") {
            continue;
        }
        let Some(stem) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else {
            continue;
        };
        if let Some(slice) = decode_key(&stem).as_deref().and_then(classify) {
            found.push((slice, path));
        }
    }
    found.sort_by(|left, right| left.1.cmp(&right.1));
    found
}

fn read_value(guard: &Guard, capability: Capability, path: &Path) -> Result<Value, GuardError> {
    let mut opened = guard.open_read(ToolId::GrokBot, capability, path)?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = opened.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() as u64 > MAX_FILE {
            return Err(GuardError::Io(std::io::ErrorKind::InvalidData));
        }
    }
    let parsed: Value = serde_json::from_slice(&raw)
        .map_err(|_| GuardError::Io(std::io::ErrorKind::InvalidData))?;
    Ok(parsed.get("value").cloned().unwrap_or(Value::Null))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrokAgent {
    pub id: String,
    pub name: String,
    pub updated_ms: i64,
}

/// 会话列表（只取编号、名字、最后活动时间）。
pub fn agents(guard: &Guard, root: &Path) -> Result<Vec<GrokAgent>, GuardError> {
    let Some((_, path)) = slices(root).into_iter().find(|(slice, _)| *slice == Slice::Roster) else {
        return Ok(Vec::new());
    };
    let value = read_value(guard, Capability::Conversation, &path)?;
    Ok(parse_roster(&value))
}

pub fn parse_roster(value: &Value) -> Vec<GrokAgent> {
    let Some(rows) = value.get("rows").and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let id = row.get("id").and_then(Value::as_str).filter(|id| valid_id(id))?;
            let name = row
                .get("title")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .or_else(|| row.get("name").and_then(Value::as_str))
                .unwrap_or("");
            let updated_ms = ["lastActivityAt", "localActivityAt", "updatedAt", "createdAt"]
                .iter()
                .filter_map(|key| row.get(*key).and_then(Value::as_i64))
                .max()
                .unwrap_or(0);
            Some(GrokAgent {
                id: id.to_string(),
                name: short_title(name),
                updated_ms,
            })
        })
        .collect()
}

/// 所选会话缓存里的消息：「用户：」「助手：」块；没有缓存时返回 None。
pub fn transcript(guard: &Guard, root: &Path, id: &str) -> Result<Option<Vec<String>>, GuardError> {
    if !valid_id(id) {
        return Ok(None);
    }
    let want = Slice::Transcript(id.to_string());
    let Some((_, path)) = slices(root).into_iter().find(|(slice, _)| *slice == want) else {
        return Ok(None);
    };
    let value = read_value(guard, Capability::Conversation, &path)?;
    Ok(Some(parse_transcript(&value)))
}

pub fn parse_transcript(value: &Value) -> Vec<String> {
    let Some(entries) = value.get("entries").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut spoken: Vec<(i64, i64, &str, &str)> = entries
        .iter()
        .filter_map(|entry| {
            let kind = entry.get("kind").and_then(Value::as_str)?;
            let (role, text) = match kind {
                "message" => {
                    let role = entry.get("role").and_then(Value::as_str)?;
                    let label = match role {
                        "user" => "用户",
                        "assistant" => "助手",
                        _ => return None,
                    };
                    (label, entry.get("content").and_then(Value::as_str)?)
                }
                // Bot 主动发出的消息
                "send-message" => {
                    let message = entry.get("message")?;
                    if message.get("type").and_then(Value::as_str) != Some("text") {
                        return None;
                    }
                    ("助手", message.get("content").and_then(Value::as_str)?)
                }
                _ => return None,
            };
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let at = entry.get("timestampMs").and_then(Value::as_i64).unwrap_or(0);
            let seq = entry.get("seq").and_then(Value::as_i64).unwrap_or(0);
            Some((at, seq, role, text))
        })
        .collect();
    spoken.sort_by_key(|item| (item.0, item.1));
    let mut chunks: Vec<String> = Vec::new();
    for (_, _, role, text) in spoken {
        let chunk = format!("{role}：{text}");
        if chunks.last() != Some(&chunk) {
            chunks.push(chunk);
        }
    }
    chunks
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrokUsage {
    /// 已用百分比（0–100）
    pub percent: i32,
    pub resets_ms: Option<i64>,
    pub read_ms: Option<i64>,
}

/// 应用缓存的每周额度。
pub fn weekly_usage(guard: &Guard, root: &Path) -> Result<Option<GrokUsage>, GuardError> {
    let Some((_, path)) = slices(root).into_iter().find(|(slice, _)| *slice == Slice::Usage) else {
        return Ok(None);
    };
    let value = read_value(guard, Capability::Usage, &path)?;
    Ok(parse_usage(&value))
}

pub fn parse_usage(value: &Value) -> Option<GrokUsage> {
    let reading = value.get("reading")?;
    let usage = reading.get("usage")?;
    let percent = usage.get("percentUsed")?.as_f64()?;
    if !percent.is_finite() {
        return None;
    }
    Some(GrokUsage {
        percent: percent.round().clamp(0.0, 100.0) as i32,
        resets_ms: usage.get("nextResetMs").and_then(Value::as_i64),
        read_ms: reading.get("readAtMs").and_then(Value::as_i64),
    })
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
