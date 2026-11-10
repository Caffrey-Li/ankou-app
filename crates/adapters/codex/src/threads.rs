use std::io::BufRead;

use serde_json::{json, Value};
use wanxiang_core::UsageError;

use crate::server::{next_id, write_line};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadHead {
    pub id: String,
    pub name: Option<String>,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub cwd: Option<String>,
    pub used_tokens: Option<i64>,
    pub window_tokens: Option<i64>,
    pub from_jsonl: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadText {
    pub head: ThreadHead,
    pub text: String,
    pub truncated: bool,
}

pub fn thread_list_line(id: i64) -> String {
    json!({
        "method": "thread/list",
        "id": id,
        "params": {"useStateDbOnly": true, "limit": 100}
    })
    .to_string()
}

pub fn thread_read_line(id: i64, thread_id: &str) -> String {
    json!({
        "method": "thread/read",
        "id": id,
        "params": {"threadId": thread_id}
    })
    .to_string()
}

pub fn thread_turns_line(id: i64, thread_id: &str, cursor: Option<&str>) -> String {
    json!({
        "method": "thread/turns/list",
        "id": id,
        "params": {
            "threadId": thread_id,
            "limit": wanxiang_core::RECENT_TURNS,
            "sortDirection": "desc",
            "cursor": cursor,
        }
    })
    .to_string()
}

pub fn thread_items_line(id: i64, thread_id: &str, turn_id: &str, cursor: Option<&str>) -> String {
    json!({
        "method": "thread/items/list",
        "id": id,
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "limit": 100,
            "cursor": cursor,
        }
    })
    .to_string()
}

pub fn parse_thread_list(value: &Value) -> Vec<ThreadHead> {
    let data = value
        .pointer("/result/data")
        .or_else(|| value.get("data"))
        .and_then(|item| item.as_array());
    let Some(data) = data else {
        return Vec::new();
    };
    data.iter().filter_map(head_from_server).collect()
}

pub fn parse_thread_read(value: &Value) -> Option<ThreadHead> {
    value.pointer("/result/thread").or_else(|| value.get("thread")).and_then(head_from_server)
}

pub fn context_percent(used: Option<i64>, window: Option<i64>) -> Option<u32> {
    let (used, window) = (used?, window?);
    if used < 0 || window <= 0 {
        return None;
    }
    let percent = (used.saturating_mul(100) + window / 2) / window;
    u32::try_from(percent).ok().map(|value| value.min(100))
}

pub fn title_of(head: &ThreadHead) -> String {
    if let Some(name) = head.name.as_ref().map(|name| name.trim()).filter(|name| !name.is_empty()) {
        return name.to_string();
    }
    let prefix: String = head.id.chars().take(8).collect();
    format!("Codex {prefix}")
}

pub fn parse_jsonl_head(text: &str) -> Option<ThreadHead> {
    let mut id = None;
    let mut cwd = None;
    let mut created = None;
    let mut updated = None;
    let mut used = None;
    let mut window = None;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = value.get("type").and_then(|item| item.as_str());
        let payload = value.get("payload");
        if kind == Some("session_meta") {
            if let Some(payload) = payload {
                id = payload.get("id").and_then(|item| item.as_str()).map(str::to_string);
                cwd = payload.get("cwd").and_then(|item| item.as_str()).map(str::to_string);
                created = payload
                    .get("timestamp")
                    .and_then(|item| item.as_str())
                    .map(str::to_string);
            }
        }
        updated = value
            .get("timestamp")
            .and_then(|item| item.as_str())
            .map(str::to_string)
            .or(updated);
        if kind == Some("event_msg")
            && payload.and_then(|item| item.get("type")).and_then(|item| item.as_str()) == Some("token_count")
        {
            let info = payload.and_then(|item| item.get("info"));
            used = info
                .and_then(|item| item.pointer("/last_token_usage/total_tokens"))
                .and_then(|item| item.as_i64());
            window = info.and_then(|item| item.get("model_context_window")).and_then(|item| item.as_i64());
        }
    }
    let id = id?;
    Some(ThreadHead {
        id,
        name: None,
        preview: String::new(),
        created_at: created.unwrap_or_default(),
        updated_at: updated.unwrap_or_default(),
        status: String::new(),
        cwd,
        used_tokens: used,
        window_tokens: window,
        from_jsonl: true,
    })
}

pub fn parse_jsonl_text(text: &str) -> (String, bool) {
    let mut chunks = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(message) = value.pointer("/payload/message").and_then(|item| item.as_str()) {
            chunks.push(message.to_string());
        }
        if let Some(items) = value.pointer("/payload/content").and_then(|item| item.as_array()) {
            for item in items {
                if let Some(text) = item.get("text").and_then(|item| item.as_str()) {
                    chunks.push(text.to_string());
                }
            }
        }
    }
    let start = chunks.len().saturating_sub(wanxiang_core::RECENT_TURNS);
    wanxiang_core::take_recent(&chunks[start..])
}

pub fn turn_ids(value: &Value) -> (Vec<String>, Option<String>) {
    let data = value
        .pointer("/result/data")
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();
    let ids = data
        .iter()
        .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(str::to_string))
        .collect();
    let next = value
        .pointer("/result/nextCursor")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    (ids, next)
}

pub fn list_jsonl(guard: &wanxiang_core::Guard, root: &std::path::Path) -> Result<Vec<ThreadHead>, wanxiang_core::GuardError> {
    use wanxiang_core::{Capability, GuardError, ToolId};
    if !guard.has_grant(ToolId::Codex, Capability::Conversation) {
        return Err(GuardError::NotGranted);
    }
    let mut heads = Vec::new();
    for path in crate::rollout_paths(root) {
        let _opened = guard.open_read(ToolId::Codex, Capability::Conversation, &path)?;
        let text = std::fs::read_to_string(&path).map_err(|err| GuardError::Io(err.kind()))?;
        if let Some(head) = parse_jsonl_head(&text) {
            heads.push(head);
        }
    }
    Ok(heads)
}

pub fn read_jsonl(
    guard: &wanxiang_core::Guard,
    path: &std::path::Path,
) -> Result<(String, bool), wanxiang_core::GuardError> {
    use wanxiang_core::{Capability, GuardError, ToolId};
    let _opened = guard.open_read(ToolId::Codex, Capability::Conversation, path)?;
    let text = std::fs::read_to_string(path).map_err(|err| GuardError::Io(err.kind()))?;
    Ok(parse_jsonl_text(&text))
}

pub fn item_texts(value: &Value) -> (Vec<String>, Option<String>) {
    let data = value
        .pointer("/result/data")
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();
    let texts = data.iter().filter_map(item_text).collect();
    let next = value
        .pointer("/result/nextCursor")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    (texts, next)
}

pub fn list_threads(
    handle: &wanxiang_core::SupervisorHandle,
    command: std::process::Command,
) -> Result<Vec<ThreadHead>, UsageError> {
    let tracked = handle.spawn_tracked(command).map_err(|_| UsageError::AppServer)?;
    let pid = tracked.pid;
    let result = talk_list(tracked.stdin, tracked.stdout);
    handle.stop_child(pid);
    result
}

fn talk_list(mut stdin: std::process::ChildStdin, stdout: impl std::io::Read + Send + 'static) -> Result<Vec<ThreadHead>, UsageError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if tx.send(std::mem::take(&mut line)).is_err() {
                break;
            }
        }
    });
    write_line(
        &mut stdin,
        r#"{"method":"initialize","id":1,"params":{"clientInfo":{"name":"wanxiang","version":"0.0.0"}}}"#,
    )?;
    let ready = next_id(&rx, 1)?;
    if ready.get("error").is_some() {
        return Err(UsageError::AppServer);
    }
    write_line(&mut stdin, r#"{"method":"initialized"}"#)?;
    write_line(&mut stdin, &thread_list_line(2))?;
    let listed = next_id(&rx, 2)?;
    if listed.get("error").is_some() {
        return Err(UsageError::AppServer);
    }
    Ok(parse_thread_list(&listed))
}

pub fn read_thread(
    handle: &wanxiang_core::SupervisorHandle,
    command: std::process::Command,
    thread_id: &str,
) -> Result<ThreadText, UsageError> {
    let tracked = handle.spawn_tracked(command).map_err(|_| UsageError::AppServer)?;
    let pid = tracked.pid;
    let result = talk(tracked.stdin, tracked.stdout, thread_id);
    handle.stop_child(pid);
    result
}

fn talk(
    mut stdin: std::process::ChildStdin,
    stdout: impl std::io::Read + Send + 'static,
    thread_id: &str,
) -> Result<ThreadText, UsageError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if tx.send(std::mem::take(&mut line)).is_err() {
                break;
            }
        }
    });
    write_line(
        &mut stdin,
        r#"{"method":"initialize","id":1,"params":{"clientInfo":{"name":"wanxiang","version":"0.0.0"}}}"#,
    )?;
    let ready = next_id(&rx, 1)?;
    if ready.get("error").is_some() {
        return Err(UsageError::AppServer);
    }
    write_line(&mut stdin, r#"{"method":"initialized"}"#)?;
    write_line(&mut stdin, &thread_list_line(2))?;
    let listed = next_id(&rx, 2)?;
    if listed.get("error").is_some() {
        return Err(UsageError::AppServer);
    }
    let heads = parse_thread_list(&listed);
    let Some(mut head) = heads.into_iter().find(|head| head.id == thread_id) else {
        return Err(UsageError::QueryFailed);
    };
    write_line(&mut stdin, &thread_read_line(3, thread_id))?;
    let read = next_id(&rx, 3)?;
    if read.get("error").is_none() {
        if let Some(summary) = parse_thread_read(&read) {
            head.name = summary.name.or(head.name);
            head.preview = summary.preview;
            head.status = summary.status;
            head.created_at = summary.created_at;
            head.updated_at = summary.updated_at;
        }
    }
    head.cwd = None;
    head.from_jsonl = false;
    let mut chunks = Vec::new();
    let mut turn_cursor = None;
    let mut turns_seen = 0usize;
    let mut truncated = false;
    let mut rpc = 4i64;
    while turns_seen < wanxiang_core::RECENT_TURNS {
        write_line(&mut stdin, &thread_turns_line(rpc, thread_id, turn_cursor.as_deref()))?;
        let page = next_id(&rx, rpc)?;
        rpc += 1;
        if page.get("error").is_some() {
            return Err(UsageError::AppServer);
        }
        let (ids, next) = turn_ids(&page);
        if ids.is_empty() {
            break;
        }
        for turn in ids {
            if turns_seen >= wanxiang_core::RECENT_TURNS {
                truncated = true;
                break;
            }
            turns_seen += 1;
            let mut item_cursor = None;
            for _ in 0..wanxiang_core::ITEM_PAGES {
                write_line(
                    &mut stdin,
                    &thread_items_line(rpc, thread_id, &turn, item_cursor.as_deref()),
                )?;
                let items = next_id(&rx, rpc)?;
                rpc += 1;
                if items.get("error").is_some() {
                    return Err(UsageError::AppServer);
                }
                let (texts, next_items) = item_texts(&items);
                chunks.extend(texts);
                match next_items {
                    Some(cursor) => item_cursor = Some(cursor),
                    None => break,
                }
            }
            if item_cursor.is_some() {
                truncated = true;
            }
            let (joined, cut) = wanxiang_core::take_recent(&chunks);
            if cut {
                return Ok(ThreadText {
                    head,
                    text: joined,
                    truncated: true,
                });
            }
        }
        match next {
            Some(cursor) if turns_seen < wanxiang_core::RECENT_TURNS => turn_cursor = Some(cursor),
            Some(_) => {
                truncated = true;
                break;
            }
            None => break,
        }
    }
    let (text, cut) = wanxiang_core::take_recent(&chunks);
    Ok(ThreadText {
        head,
        text,
        truncated: truncated || cut,
    })
}

fn head_from_server(value: &Value) -> Option<ThreadHead> {
    let id = value.get("id")?.as_str()?.to_string();
    Some(ThreadHead {
        id,
        name: value.get("name").and_then(|item| item.as_str()).map(str::to_string),
        preview: value.get("preview").and_then(|item| item.as_str()).unwrap_or("").to_string(),
        created_at: json_text(value.get("createdAt")),
        updated_at: json_text(value.get("updatedAt")),
        status: value.get("status").and_then(|item| item.as_str()).unwrap_or("").to_string(),
        cwd: None,
        used_tokens: None,
        window_tokens: None,
        from_jsonl: false,
    })
}

fn json_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn item_text(value: &Value) -> Option<String> {
    let item = value.get("item").unwrap_or(value);
    if let Some(text) = item.get("text").and_then(|part| part.as_str()) {
        return Some(text.to_string());
    }
    if let Some(text) = item.pointer("/content/0/text").and_then(|part| part.as_str()) {
        return Some(text.to_string());
    }
    item.get("content").and_then(|part| part.as_str()).map(str::to_string)
}
