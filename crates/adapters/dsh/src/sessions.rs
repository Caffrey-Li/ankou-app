//! DeepSeek Harness 本机会话：`~/.dsh/sessions/<工作区>/session-<id>/session.v4.jsonl.zstd`。
//! 每行一条记录；`assistant/message` 带单次调用的用量，`request/context` 带上下文窗口。
//! Bot 工作区（cwd 在 `~/.dsh/bot/workspace` 下）的会话属于 DS Bot，Harness 不计入，避免重复。
//! 只读：经守卫打开文件后在内存里解压，不写回、不缓存正文。

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wanxiang_core::{Capability, Guard, GuardError, ToolId};

pub const SESSION_FILE: &str = "session.v4.jsonl.zstd";
/// Harness 用量与对话授权的文件范围。
pub const HARNESS_PATTERN: &str = "sessions/**/session.v4.jsonl.zstd";
/// DS Bot 对话授权只覆盖 Bot 工作区的会话。
pub const BOT_SESSION_PATTERN: &str = "sessions/*-.dsh-bot-workspace--/**/session.v4.jsonl.zstd";
/// 解压后的上限，防止异常文件占满内存。
const MAX_DECODED: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DshCall {
    pub time_ms: i64,
    pub tokens: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DshSession {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// 最后一次调用的总 token，近似当前上下文占用。
    pub last_tokens: Option<i64>,
    pub context_window: Option<i64>,
    pub calls: Vec<DshCall>,
    /// 同一条助手消息被重复写入的次数（按消息编号去重后丢弃的条数）。
    pub duplicates: usize,
}

impl DshSession {
    pub fn is_bot(&self) -> bool {
        is_bot_workspace(&self.cwd)
    }

    pub fn context_percent(&self) -> Option<u32> {
        let used = self.last_tokens?;
        let window = self.context_window?;
        if used < 0 || window <= 0 {
            return None;
        }
        let percent = (used.saturating_mul(100) + window / 2) / window;
        u32::try_from(percent).ok().map(|value| value.min(100))
    }
}

pub fn is_bot_workspace(cwd: &str) -> bool {
    let normalized = cwd.replace('\\', "/");
    normalized.contains("/.dsh/bot/workspace")
}

/// 列出 `root/sessions/*/*/session.v4.jsonl.zstd`，按路径排序；只看目录结构，不打开文件。
pub fn session_paths(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(workspaces) = std::fs::read_dir(root.join("sessions")) else {
        return found;
    };
    for workspace in workspaces.flatten() {
        let Ok(kind) = workspace.file_type() else { continue };
        if !kind.is_dir() {
            continue;
        }
        let Ok(sessions) = std::fs::read_dir(workspace.path()) else { continue };
        for session in sessions.flatten() {
            let Ok(kind) = session.file_type() else { continue };
            if !kind.is_dir() {
                continue;
            }
            let file = session.path().join(SESSION_FILE);
            if file.is_file() {
                found.push(file);
            }
        }
    }
    found.sort();
    found
}

/// 经守卫读取并解压。写到一半的会话可能缺最后一帧，已解出的部分照常返回。
pub fn read_session_bytes(
    guard: &Guard,
    tool: ToolId,
    capability: Capability,
    path: &Path,
) -> Result<Vec<u8>, GuardError> {
    let mut opened = guard.open_read(tool, capability, path)?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = opened.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
    }
    Ok(decode_zstd(&raw))
}

pub fn decode_zstd(raw: &[u8]) -> Vec<u8> {
    // 会话文件按追加写入，可能由多个 zstd 帧首尾相接；逐帧解开，遇到坏帧就停在已解出的部分。
    let mut out = Vec::new();
    let mut rest: &[u8] = raw;
    let mut chunk = [0u8; 64 * 1024];
    while !rest.is_empty() && out.len() <= MAX_DECODED {
        let before = rest.len();
        let Ok(mut decoder) = ruzstd::decoding::StreamingDecoder::new(&mut rest) else {
            break;
        };
        let mut failed = false;
        loop {
            match decoder.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    out.extend_from_slice(&chunk[..read]);
                    if out.len() > MAX_DECODED {
                        break;
                    }
                }
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        drop(decoder);
        if failed || rest.len() == before {
            break;
        }
    }
    out
}

/// 只取元数据与用量，不保留任何对话正文。
pub fn parse_session(decoded: &[u8]) -> Option<DshSession> {
    let text = String::from_utf8_lossy(decoded);
    let mut session = DshSession::default();
    let mut seen_head = false;
    let mut by_message = std::collections::HashMap::<String, usize>::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let time = value.get("time").and_then(Value::as_i64);
        if let Some(time) = time {
            session.updated_ms = session.updated_ms.max(time);
        }
        match kind {
            "session" => {
                seen_head = true;
                session.id = text_of(value.get("id"));
                session.cwd = text_of(value.get("cwd"));
                session.created_ms = value.get("createdAt").and_then(Value::as_i64).unwrap_or(0);
                session.updated_ms = session.updated_ms.max(session.created_ms);
            }
            "session/title" => {
                let title = text_of(value.pointer("/data/title"));
                if !title.trim().is_empty() {
                    session.title = title.trim().to_string();
                }
            }
            "request/context" => {
                if let Some(window) = value.pointer("/data/contextWindow").and_then(Value::as_i64) {
                    session.context_window = Some(window);
                }
            }
            "assistant/message" | "compaction/summary" => {
                let Some(usage) = value.pointer("/data/usage") else { continue };
                let tokens = usage_total(usage);
                if tokens <= 0 {
                    continue;
                }
                let call = DshCall {
                    time_ms: time.unwrap_or(session.updated_ms),
                    tokens,
                };
                let message = value
                    .pointer("/data/message/id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .map(|id| format!("{kind}:{id}"));
                match message.as_ref().and_then(|id| by_message.get(id)) {
                    Some(&index) => {
                        session.calls[index] = call;
                        session.duplicates += 1;
                    }
                    None => {
                        if let Some(id) = message {
                            by_message.insert(id, session.calls.len());
                        }
                        session.calls.push(call);
                    }
                }
                if kind == "assistant/message" {
                    session.last_tokens = Some(tokens);
                }
            }
            _ => {}
        }
    }
    (seen_head && !session.id.is_empty()).then_some(session)
}

fn usage_total(usage: &Value) -> i64 {
    let field = |name: &str| usage.get(name).and_then(Value::as_i64).unwrap_or(0).max(0);
    let parts = field("inputTokens")
        + field("cacheReadTokens")
        + field("cacheWriteTokens")
        + field("outputTokens");
    let total = field("totalTokens");
    if parts > 0 {
        parts
    } else {
        total
    }
}

fn text_of(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_string()
}

/// 交接用的正文：用户与助手的文字段落，跳过推理、工具调用与工具结果。
pub fn session_chunks(decoded: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(decoded);
    let mut chunks = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let content = match kind {
            "user/message" => value.pointer("/data/content"),
            "assistant/message" => value.pointer("/data/message/content"),
            _ => None,
        };
        let Some(content) = content else { continue };
        let mut parts = Vec::new();
        match content {
            Value::String(text) => parts.push(text.clone()),
            Value::Array(blocks) => {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        let joined = parts.join("\n").trim().to_string();
        if joined.is_empty() {
            continue;
        }
        let who = if kind == "user/message" { "用户" } else { "助手" };
        chunks.push(format!("{who}：{joined}"));
    }
    chunks
}
