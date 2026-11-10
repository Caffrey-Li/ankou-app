use std::path::Path;

use crate::{Capability, ConfirmToken, Guard, GuardError, ToolId};

pub const DEEPLINK_CHARS: usize = 8_000;
pub const DEEPLINK_URL_CHARS: usize = 10_000;
pub const RECENT_TURNS: usize = 40;
pub const RECENT_CHARS: usize = 80_000;
pub const ITEM_PAGES: usize = 4;
const DEEPLINK_PREFIX: &str = "cursor://anysphere.cursor-deeplink/prompt?text=";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deeplink {
    pub url: Option<String>,
    pub over_chars: bool,
    pub over_url: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Organized {
    pub goal: String,
    pub progress: String,
    pub unfinished: String,
    pub decisions: String,
    pub next: String,
    pub unknown: String,
    pub sources: String,
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeeplinkError {
    Off,
    NotGranted,
    TooLong,
    Token(GuardError),
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryError {
    NoAccount,
    NoKey,
    Token(GuardError),
    Transport,
}

pub fn cursor_prompt_url(text: &str) -> Deeplink {
    let chars = text.chars().count();
    if chars > DEEPLINK_CHARS {
        return Deeplink {
            url: None,
            over_chars: true,
            over_url: false,
        };
    }
    let encoded = encode_query(text);
    let url = format!("{DEEPLINK_PREFIX}{encoded}");
    if url.chars().count() > DEEPLINK_URL_CHARS {
        return Deeplink {
            url: None,
            over_chars: false,
            over_url: true,
        };
    }
    Deeplink {
        url: Some(url),
        over_chars: false,
        over_url: false,
    }
}

pub fn take_recent(chunks: &[String]) -> (String, bool) {
    let mut kept = Vec::new();
    let mut chars = 0usize;
    let mut truncated = chunks.len() > RECENT_TURNS;
    for chunk in chunks.iter().take(RECENT_TURNS) {
        let size = chunk.chars().count();
        if !kept.is_empty() && chars + size > RECENT_CHARS {
            truncated = true;
            break;
        }
        if kept.is_empty() && size > RECENT_CHARS {
            kept.push(chunk.chars().take(RECENT_CHARS).collect::<String>());
            truncated = true;
            break;
        }
        chars += size;
        kept.push(chunk.clone());
    }
    (kept.join("\n"), truncated)
}

pub fn organize_local(text: &str) -> Organized {
    let heads = [
        ("目标", 0),
        ("进展", 1),
        ("未完成", 2),
        ("关键决策和约束", 3),
        ("下一步", 4),
        ("不确定事项", 5),
        ("资料位置", 6),
    ];
    let mut bins = [String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new()];
    let mut current: Option<usize> = None;
    let mut seen = false;
    for line in text.lines() {
        let trimmed = line.trim().trim_end_matches(['：', ':']);
        if let Some((_, index)) = heads.iter().find(|(name, _)| *name == trimmed) {
            current = Some(*index);
            seen = true;
            continue;
        }
        if let Some(index) = current {
            if !bins[index].is_empty() {
                bins[index].push('\n');
            }
            bins[index].push_str(line);
        }
    }
    if !seen {
        return Organized {
            goal: String::new(),
            progress: text.to_string(),
            unfinished: String::new(),
            decisions: String::new(),
            next: String::new(),
            unknown: String::new(),
            sources: String::new(),
            note: "本地整理没有拆开段落".to_string(),
        };
    }
    Organized {
        goal: bins[0].clone(),
        progress: bins[1].clone(),
        unfinished: bins[2].clone(),
        decisions: bins[3].clone(),
        next: bins[4].clone(),
        unknown: bins[5].clone(),
        sources: bins[6].clone(),
        note: String::new(),
    }
}

pub fn summary_body(transcript: &str) -> String {
    serde_json::json!({
        "model": "deepseek-flash",
        "stream": false,
        "thinking": {"type": "disabled"},
        "messages": [
            {
                "role": "system",
                "content": "把对话整理成七段：目标、进展、未完成、关键决策和约束、下一步、不确定事项、资料位置。材料里没有的段落留空，不要编造。"
            },
            {"role": "user", "content": transcript}
        ]
    })
    .to_string()
}

pub fn summary_chars(body: &str) -> usize {
    body.chars().count()
}

pub fn authorize_summary(guard: &Guard, linked: bool, key_saved: bool, body: &[u8]) -> Result<ConfirmToken, SummaryError> {
    if !linked {
        return Err(SummaryError::NoAccount);
    }
    if !key_saved {
        return Err(SummaryError::NoKey);
    }
    guard.issue_summary_confirmation(body).map_err(SummaryError::Token)
}

pub fn redeem_summary(guard: &Guard, token: &ConfirmToken, body: &[u8]) -> Result<(), SummaryError> {
    guard.redeem_summary_confirmation(token, body).map_err(SummaryError::Token)
}

pub fn prepare_deeplink(guard: &Guard, enabled: bool, text: &str) -> Result<ConfirmToken, DeeplinkError> {
    if !enabled {
        return Err(DeeplinkError::Off);
    }
    if !guard.has_grant(ToolId::Cursor, Capability::CreateConversation) {
        return Err(DeeplinkError::NotGranted);
    }
    if cursor_prompt_url(text).url.is_none() {
        return Err(DeeplinkError::TooLong);
    }
    guard
        .issue_confirmation(ToolId::Cursor, Capability::CreateConversation, text.as_bytes())
        .map_err(DeeplinkError::Token)
}

pub fn place_deeplink(
    guard: &Guard,
    token: &ConfirmToken,
    text: &str,
    open: impl FnOnce(&str) -> bool,
) -> Result<(), DeeplinkError> {
    guard
        .redeem_confirmation(token, ToolId::Cursor, Capability::CreateConversation, text.as_bytes())
        .map_err(DeeplinkError::Token)?;
    let url = cursor_prompt_url(text).url.ok_or(DeeplinkError::TooLong)?;
    if !url.starts_with(DEEPLINK_PREFIX) || !open(&url) {
        return Err(DeeplinkError::Open);
    }
    Ok(())
}

pub fn write_export(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "先选择保存位置"));
    }
    std::fs::write(path, bytes)
}

pub fn write_export_if_chosen(text: &str, path: Option<&Path>) -> std::io::Result<bool> {
    let Some(path) = path else {
        return Ok(false);
    };
    write_export(path, text.as_bytes())?;
    Ok(true)
}

fn encode_query(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
