use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use wanxiang_adapter_codex::{
    context_percent, find_codex_cli, list_jsonl, list_threads, parse_jsonl_head, read_jsonl,
    read_thread, rollout_paths, title_of,
};
use wanxiang_adapter_cursor::read_fixture;
use wanxiang_adapter_dsh::{fetch_summary, list_sessions, session_transcript};
use wanxiang_core::{
    authorize_summary, organize_local, redeem_summary, summary_body, summary_chars,
    write_export_if_chosen, Capability, ConfirmToken, DeeplinkError, GrantStatus, SummaryError,
    ToolId,
};

use crate::commands::Hub;

#[derive(Clone, serde::Serialize)]
pub struct ConversationRow {
    pub id: String,
    pub tool: String,
    pub title: String,
    pub updated: String,
    pub percent: Option<u32>,
    pub cwd: Option<String>,
    pub status: String,
    pub context: String,
    pub source: String,
}

#[derive(Clone, serde::Serialize)]
pub struct ConversationPage {
    pub rows: Vec<ConversationRow>,
    pub notes: Vec<String>,
}

#[derive(Clone, serde::Serialize)]
pub struct HandoffDraft {
    pub text: String,
    pub truncated: bool,
    pub note: String,
}

#[derive(Clone, serde::Serialize)]
pub struct SummaryPreview {
    pub token: String,
    pub body: String,
    pub chars: usize,
}

#[tauri::command]
pub fn list_conversations(hub: State<'_, Hub>) -> Result<ConversationPage, String> {
    Ok(conversations(&hub))
}

#[tauri::command]
pub fn prepare_handoff(
    hub: State<'_, Hub>,
    tool: String,
    id: String,
) -> Result<HandoffDraft, String> {
    draft(&hub, &tool, &id)
}

#[tauri::command]
pub fn begin_summary(hub: State<'_, Hub>, transcript: String) -> Result<SummaryPreview, String> {
    let linked = hub
        .store
        .guard()
        .account_link_active(ToolId::DeepSeekHarness);
    let key_saved = hub.secrets.is_saved().unwrap_or(false);
    let body = summary_body(&transcript);
    let token = authorize_summary(hub.store.guard(), linked, key_saved, body.as_bytes())
        .map_err(summary_text)?;
    Ok(SummaryPreview {
        token: token.to_hex(),
        chars: summary_chars(&body),
        body,
    })
}

#[tauri::command]
pub fn send_summary(hub: State<'_, Hub>, token: String, body: String) -> Result<String, String> {
    let token = ConfirmToken::from_hex(&token).ok_or_else(|| "确认已失效".to_string())?;
    redeem_summary(hub.store.guard(), &token, body.as_bytes()).map_err(summary_text)?;
    let key = hub
        .secrets
        .load()
        .map_err(|_| "先在设置里保存 DeepSeek 钥匙".to_string())?;
    let text = fetch_summary("https://api.deepseek.com", &key, &body)
        .map_err(|_| "摘要没有生成。编辑还在，可以改用本地整理或重试。".to_string());
    drop(key);
    text
}

#[tauri::command]
pub fn prepare_deeplink(hub: State<'_, Hub>, text: String) -> Result<String, String> {
    let enabled = hub
        .store
        .settings()
        .map(|settings| settings.cursor_deeplink)
        .unwrap_or(false);
    wanxiang_core::prepare_deeplink(hub.store.guard(), enabled, &text)
        .map(|token| token.to_hex())
        .map_err(deeplink_text)
}

#[tauri::command]
pub fn place_deeplink<R: tauri::Runtime>(
    app: AppHandle<R>,
    hub: State<'_, Hub>,
    token: String,
    text: String,
) -> Result<(), String> {
    let token = ConfirmToken::from_hex(&token).ok_or_else(|| "确认已失效".to_string())?;
    wanxiang_core::place_deeplink(hub.store.guard(), &token, &text, |url| {
        app.opener().open_url(url, None::<&str>).is_ok()
    })
    .map_err(|_| "未能放入：深链没有打开 · 编辑已保留".to_string())
}

#[tauri::command]
pub fn export_handoff<R: tauri::Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    let chosen = app
        .dialog()
        .file()
        .set_title("导出交接")
        .set_file_name("交接.md")
        .blocking_save_file();
    let path = chosen
        .map(|file| file.into_path().map_err(|err| err.to_string()))
        .transpose()?;
    write_export_if_chosen(&text, path.as_deref())
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn organize_handoff(text: String) -> OrganizedView {
    let organized = organize_local(&text);
    OrganizedView {
        goal: organized.goal,
        progress: organized.progress,
        unfinished: organized.unfinished,
        decisions: organized.decisions,
        next: organized.next,
        unknown: organized.unknown,
        sources: organized.sources,
        note: organized.note,
    }
}

#[derive(Clone, serde::Serialize)]
pub struct OrganizedView {
    pub goal: String,
    pub progress: String,
    pub unfinished: String,
    pub decisions: String,
    pub next: String,
    pub unknown: String,
    pub sources: String,
    pub note: String,
}

fn conversations(hub: &Hub) -> ConversationPage {
    let grants = hub.store.grants().unwrap_or_default();
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    let codex = grants.iter().find(|grant| {
        grant.tool == ToolId::Codex
            && grant.capability == Capability::Conversation
            && grant.status == GrantStatus::Active
    });
    if let Some(grant) = codex {
        let server_on = hub
            .store
            .settings()
            .ok()
            .is_some_and(|settings| settings.codex_app_server);
        let mut from_server = false;
        if server_on {
            if let Some(bin) = find_codex_cli(&std::env::var("PATH").unwrap_or_default()) {
                let mut command = Command::new(bin);
                command.arg("app-server");
                match list_threads(&hub.gate.supervisor(), command) {
                    Ok(heads) => {
                        from_server = true;
                        for head in heads {
                            rows.push(row_from_head("codex", &head, false));
                        }
                    }
                    Err(_) => notes.push("ChatGPT 应用未运行，没有读到本机会话".to_string()),
                }
            } else {
                notes.push("ChatGPT 应用未运行，没有读到本机会话".to_string());
            }
        }
        if !from_server {
            notes.retain(|note| note != "ChatGPT 应用未运行，没有读到本机会话");
            match list_jsonl(hub.store.guard(), &grant.root) {
                Ok(heads) => {
                    if heads.is_empty() && server_on {
                        notes.push("ChatGPT 应用未运行，没有读到本机会话".to_string());
                    }
                    for head in heads {
                        rows.push(row_from_head("codex", &head, true));
                    }
                }
                Err(_) => notes.push("ChatGPT 对话未授权".to_string()),
            }
        }
    } else {
        notes.push("ChatGPT 对话未授权".to_string());
    }
    let bot = grants.iter().find(|grant| {
        grant.tool == ToolId::DsBot
            && grant.capability == Capability::Conversation
            && grant.status == GrantStatus::Active
    });
    if let Some(grant) = bot {
        let db = grant.root.join("bot/usage.sqlite");
        match list_sessions(hub.store.guard(), &db) {
            Ok(heads) => {
                for head in heads {
                    let title = if head.name.is_empty() {
                        head.session.clone()
                    } else {
                        head.name
                    };
                    rows.push(ConversationRow {
                        id: head.session,
                        tool: "ds_bot".to_string(),
                        title,
                        updated: head.updated_ms.to_string(),
                        percent: None,
                        cwd: None,
                        status: String::new(),
                        context: "暂不提供".to_string(),
                        source: "usage_sqlite".to_string(),
                    });
                }
            }
            Err(_) => notes.push("DS Bot 对话未授权".to_string()),
        }
    } else {
        notes.push("DS Bot 对话未授权".to_string());
    }
    notes.push("Harness 对话暂不提供".to_string());
    notes.push("Cursor 对话未验证".to_string());
    if std::env::var_os("WANXIANG_CURSOR_FIXTURE").is_some() {
        if let Ok(text) = read_fixture(hub.store.guard()) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(items) = value.as_array() {
                    for item in items {
                        rows.push(ConversationRow {
                            id: item
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("fixture")
                                .to_string(),
                            tool: "cursor".to_string(),
                            title: item
                                .get("title")
                                .and_then(|v| v.as_str())
                                .unwrap_or("夹具对话")
                                .to_string(),
                            updated: item
                                .get("updated")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            percent: item
                                .get("percent")
                                .and_then(|v| v.as_u64())
                                .and_then(|v| u32::try_from(v).ok()),
                            cwd: None,
                            status: "夹具".to_string(),
                            context: String::new(),
                            source: "fixture".to_string(),
                        });
                    }
                }
            }
        }
    }
    ConversationPage { rows, notes }
}

fn row_from_head(
    tool: &str,
    head: &wanxiang_adapter_codex::ThreadHead,
    jsonl: bool,
) -> ConversationRow {
    let percent = context_percent(head.used_tokens, head.window_tokens);
    ConversationRow {
        id: head.id.clone(),
        tool: tool.to_string(),
        title: title_of(head),
        updated: head.updated_at.clone(),
        percent,
        cwd: if jsonl { head.cwd.clone() } else { None },
        status: head.status.clone(),
        context: if percent.is_some() {
            String::new()
        } else {
            "未知".to_string()
        },
        source: if jsonl {
            "jsonl".to_string()
        } else {
            "app_server".to_string()
        },
    }
}

fn draft(hub: &Hub, tool: &str, id: &str) -> Result<HandoffDraft, String> {
    match tool {
        "codex" => codex_draft(hub, id),
        "ds_bot" => {
            let grants = hub.store.grants().map_err(|err| err.to_string())?;
            let Some(grant) = grants.iter().find(|grant| {
                grant.tool == ToolId::DsBot
                    && grant.capability == Capability::Conversation
                    && grant.status == GrantStatus::Active
            }) else {
                return Err("DS Bot 对话未授权".to_string());
            };
            let db = grant.root.join("bot/usage.sqlite");
            match session_transcript(hub.store.guard(), &db) {
                Ok(None) => Ok(HandoffDraft {
                    text: String::new(),
                    truncated: false,
                    note: "暂不提供".to_string(),
                }),
                Ok(Some(_)) => Err("暂不提供".to_string()),
                Err(_) => Err("DS Bot 对话未授权".to_string()),
            }
        }
        "cursor" => {
            if std::env::var_os("WANXIANG_CURSOR_FIXTURE").is_none() {
                return Err("Cursor 对话未验证".to_string());
            }
            let text =
                read_fixture(hub.store.guard()).map_err(|_| "Cursor 对话未授权".to_string())?;
            if !text.contains(&format!("\"id\": \"{id}\""))
                && !text.contains(&format!("\"id\":\"{id}\""))
            {
                return Err("这条对话已经不在，无法交接".to_string());
            }
            Ok(HandoffDraft {
                text: "夹具正文".to_string(),
                truncated: false,
                note: String::new(),
            })
        }
        "deepseek_harness" => Err("暂不提供".to_string()),
        _ => Err("这条对话已经不在，无法交接".to_string()),
    }
}

fn codex_draft(hub: &Hub, id: &str) -> Result<HandoffDraft, String> {
    let grants = hub.store.grants().map_err(|err| err.to_string())?;
    let Some(grant) = grants.iter().find(|grant| {
        grant.tool == ToolId::Codex
            && grant.capability == Capability::Conversation
            && grant.status == GrantStatus::Active
    }) else {
        return Err("ChatGPT 对话未授权".to_string());
    };
    let settings = hub.store.settings().map_err(|err| err.to_string())?;
    if settings.codex_app_server {
        if let Some(bin) = find_codex_cli(&std::env::var("PATH").unwrap_or_default()) {
            let mut command = Command::new(bin);
            command.arg("app-server");
            match read_thread(&hub.gate.supervisor(), command, id) {
                Ok(thread) => {
                    return Ok(HandoffDraft {
                        text: thread.text,
                        truncated: thread.truncated,
                        note: if thread.truncated {
                            "长对话只取了最近部分".to_string()
                        } else {
                            String::new()
                        },
                    });
                }
                Err(wanxiang_core::UsageError::QueryFailed) => {
                    return Err("这条对话已经不在，无法交接".to_string());
                }
                Err(_) => {}
            }
        }
    }
    let path = find_rollout(hub.store.guard(), &grant.root, id);
    let Some(path) = path else {
        return Err(if settings.codex_app_server {
            "ChatGPT 应用未运行，没有读到本机会话".to_string()
        } else {
            "这条对话已经不在，无法交接".to_string()
        });
    };
    let (text, truncated) =
        read_jsonl(hub.store.guard(), &path).map_err(|_| "ChatGPT 对话未授权".to_string())?;
    Ok(HandoffDraft {
        text,
        truncated,
        note: if truncated {
            "长对话只取了最近部分".to_string()
        } else {
            String::new()
        },
    })
}

fn find_rollout(guard: &wanxiang_core::Guard, root: &std::path::Path, id: &str) -> Option<PathBuf> {
    for path in rollout_paths(root) {
        if guard
            .open_read(ToolId::Codex, Capability::Conversation, &path)
            .is_err()
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if parse_jsonl_head(&text).is_some_and(|head| head.id == id) {
            return Some(path);
        }
    }
    None
}

fn deeplink_text(err: DeeplinkError) -> String {
    match err {
        DeeplinkError::TooLong => "超过 8000 字，不能打开深链。全文仍可复制。".to_string(),
        DeeplinkError::Off | DeeplinkError::NotGranted => "深链未开启".to_string(),
        DeeplinkError::Token(_) | DeeplinkError::Open => "确认已失效".to_string(),
    }
}

fn summary_text(err: SummaryError) -> String {
    match err {
        SummaryError::NoAccount => "先连接 DeepSeek 账户".to_string(),
        SummaryError::NoKey => "先在设置里保存 DeepSeek 钥匙".to_string(),
        SummaryError::Token(wanxiang_core::GuardError::ContentMismatch) => "确认已失效".to_string(),
        SummaryError::Token(wanxiang_core::GuardError::ConfirmationUsed) => {
            "确认已失效".to_string()
        }
        SummaryError::Token(wanxiang_core::GuardError::ConfirmationExpired) => {
            "确认已失效".to_string()
        }
        SummaryError::Token(_) => "确认已失效".to_string(),
        SummaryError::Transport => "摘要没有生成。编辑还在，可以改用本地整理或重试。".to_string(),
    }
}

#[cfg(test)]
mod tests {
    fn signature(source: &str, name: &str) -> String {
        let start = source.find(name).expect(name);
        let end = source[start..].find('{').unwrap() + start;
        source[start..end].to_string()
    }

    #[test]
    fn export_and_place_commands_take_no_path_and_cannot_open_an_arbitrary_url() {
        let source = include_str!("handoff.rs");
        let body = source.split("mod tests").next().unwrap();
        let export = signature(body, "pub fn export_handoff");
        assert!(export.contains("text: String"), "{export}");
        assert!(!export.contains("path"), "{export}");
        let place = signature(body, "pub fn place_deeplink");
        assert!(
            place.contains("token: String") && place.contains("text: String"),
            "{place}"
        );
        assert!(!place.contains("url:"), "{place}");
        assert!(body.contains("pub fn prepare_deeplink"));
        assert!(!body.contains("fn open_deeplink"));
        assert!(!body.contains("osascript"));
        assert!(!body.contains("Command::new(\"open\")"));
    }
}
