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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Unauthorized,
    NeedsReauth,
    ReadFailed,
    ServerUnavailable,
    NoRecords,
    Ok,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Auth {
    Missing,
    Active,
    NeedsReauth,
}

#[derive(Clone, Copy, Debug)]
enum ServerAttempt {
    Skipped,
    Failed,
    Listed(usize),
}

#[derive(Clone, Copy, Debug)]
enum LocalRead {
    Unread,
    Empty,
    Rows,
    Denied,
}

fn codex_status(auth: Auth, server: ServerAttempt, local: LocalRead) -> SourceStatus {
    match auth {
        Auth::Missing => SourceStatus::Unauthorized,
        Auth::NeedsReauth => SourceStatus::NeedsReauth,
        Auth::Active => match server {
            ServerAttempt::Listed(0) => SourceStatus::NoRecords,
            ServerAttempt::Listed(_) => SourceStatus::Ok,
            ServerAttempt::Failed => match local {
                LocalRead::Rows => SourceStatus::Ok,
                LocalRead::Empty => SourceStatus::ServerUnavailable,
                LocalRead::Denied | LocalRead::Unread => SourceStatus::ReadFailed,
            },
            ServerAttempt::Skipped => match local {
                LocalRead::Rows => SourceStatus::Ok,
                LocalRead::Empty => SourceStatus::NoRecords,
                LocalRead::Denied | LocalRead::Unread => SourceStatus::ReadFailed,
            },
        },
    }
}

fn bot_status(auth: Auth, local: LocalRead) -> SourceStatus {
    match auth {
        Auth::Missing => SourceStatus::Unauthorized,
        Auth::NeedsReauth => SourceStatus::NeedsReauth,
        Auth::Active => match local {
            LocalRead::Rows => SourceStatus::Ok,
            LocalRead::Empty => SourceStatus::NoRecords,
            LocalRead::Denied | LocalRead::Unread => SourceStatus::ReadFailed,
        },
    }
}

fn conversation_auth<'a>(
    grants: &'a [wanxiang_core::StoredGrant],
    tool: ToolId,
) -> (Auth, Option<&'a wanxiang_core::StoredGrant>) {
    let active = grants.iter().find(|grant| {
        grant.tool == tool
            && grant.capability == Capability::Conversation
            && grant.status == GrantStatus::Active
    });
    if active.is_some() {
        return (Auth::Active, active);
    }
    let reauth = grants.iter().any(|grant| {
        grant.tool == tool
            && grant.capability == Capability::Conversation
            && grant.status == GrantStatus::NeedsReauth
    });
    (
        if reauth {
            Auth::NeedsReauth
        } else {
            Auth::Missing
        },
        None,
    )
}

#[derive(Clone, serde::Serialize)]
pub struct ConversationPage {
    pub rows: Vec<ConversationRow>,
    pub codex: SourceStatus,
    pub ds_bot: SourceStatus,
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
    let (codex_auth, codex_grant) = conversation_auth(&grants, ToolId::Codex);
    let (codex_server, codex_local) = if let Some(grant) = codex_grant {
        let server_on = hub
            .store
            .settings()
            .ok()
            .is_some_and(|settings| settings.codex_app_server);
        let mut server = ServerAttempt::Skipped;
        let mut from_server = false;
        if server_on {
            if let Some(bin) = find_codex_cli(&std::env::var("PATH").unwrap_or_default()) {
                let mut command = Command::new(bin);
                command.arg("app-server");
                match list_threads(&hub.gate.supervisor(), command) {
                    Ok(heads) => {
                        from_server = true;
                        server = ServerAttempt::Listed(heads.len());
                        for head in heads {
                            rows.push(row_from_head("codex", &head, false));
                        }
                    }
                    Err(_) => server = ServerAttempt::Failed,
                }
            }
        }
        let local = if from_server {
            LocalRead::Unread
        } else {
            match list_jsonl(hub.store.guard(), &grant.root) {
                Ok(heads) => {
                    let read = if heads.is_empty() {
                        LocalRead::Empty
                    } else {
                        LocalRead::Rows
                    };
                    for head in heads {
                        rows.push(row_from_head("codex", &head, true));
                    }
                    read
                }
                Err(_) => LocalRead::Denied,
            }
        };
        (server, local)
    } else {
        (ServerAttempt::Skipped, LocalRead::Unread)
    };
    let (bot_auth, bot_grant) = conversation_auth(&grants, ToolId::DsBot);
    let bot_local = if let Some(grant) = bot_grant {
        let db = grant.root.join("bot/usage.sqlite");
        match list_sessions(hub.store.guard(), &db) {
            Ok(heads) => {
                let read = if heads.is_empty() {
                    LocalRead::Empty
                } else {
                    LocalRead::Rows
                };
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
                read
            }
            Err(_) => LocalRead::Denied,
        }
    } else {
        LocalRead::Unread
    };
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
    ConversationPage {
        rows,
        codex: codex_status(codex_auth, codex_server, codex_local),
        ds_bot: bot_status(bot_auth, bot_local),
    }
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

    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use wanxiang_core::{
        Capability, Clock, PathScope, RefreshGate, SecretStore, Store, Supervisor, SystemClock, ToolId,
        ToolScheduler,
    };

    use super::{bot_status, codex_status, conversations, Auth, LocalRead, ServerAttempt, SourceStatus};
    use crate::commands::{Hub, ScanSlots};

    fn expect_codex(auth: Auth, server: ServerAttempt, local: LocalRead, status: SourceStatus) {
        assert_eq!(
            codex_status(auth, server, local),
            status,
            "auth={auth:?} server={server:?} local={local:?}"
        );
    }

    #[test]
    fn codex_status_covers_each_outcome() {
        expect_codex(Auth::Missing, ServerAttempt::Skipped, LocalRead::Unread, SourceStatus::Unauthorized);
        expect_codex(Auth::Missing, ServerAttempt::Failed, LocalRead::Empty, SourceStatus::Unauthorized);
        expect_codex(Auth::Missing, ServerAttempt::Skipped, LocalRead::Rows, SourceStatus::Unauthorized);
        expect_codex(Auth::Active, ServerAttempt::Skipped, LocalRead::Empty, SourceStatus::NoRecords);
        expect_codex(Auth::Active, ServerAttempt::Skipped, LocalRead::Rows, SourceStatus::Ok);
        expect_codex(Auth::Active, ServerAttempt::Skipped, LocalRead::Denied, SourceStatus::ReadFailed);
        expect_codex(Auth::Active, ServerAttempt::Skipped, LocalRead::Unread, SourceStatus::ReadFailed);
        expect_codex(Auth::Active, ServerAttempt::Failed, LocalRead::Empty, SourceStatus::ServerUnavailable);
        expect_codex(Auth::Active, ServerAttempt::Failed, LocalRead::Rows, SourceStatus::Ok);
        expect_codex(Auth::Active, ServerAttempt::Failed, LocalRead::Denied, SourceStatus::ReadFailed);
        expect_codex(Auth::Active, ServerAttempt::Failed, LocalRead::Unread, SourceStatus::ReadFailed);
        expect_codex(Auth::Active, ServerAttempt::Listed(0), LocalRead::Unread, SourceStatus::NoRecords);
        expect_codex(Auth::Active, ServerAttempt::Listed(3), LocalRead::Denied, SourceStatus::Ok);
        for server in [ServerAttempt::Skipped, ServerAttempt::Failed, ServerAttempt::Listed(0), ServerAttempt::Listed(2)] {
            for local in [LocalRead::Unread, LocalRead::Empty, LocalRead::Rows, LocalRead::Denied] {
                expect_codex(Auth::NeedsReauth, server, local, SourceStatus::NeedsReauth);
            }
        }
    }

    #[test]
    fn ds_bot_status_has_no_server_failure() {
        assert_eq!(bot_status(Auth::Missing, LocalRead::Rows), SourceStatus::Unauthorized);
        assert_eq!(bot_status(Auth::Active, LocalRead::Empty), SourceStatus::NoRecords);
        assert_eq!(bot_status(Auth::Active, LocalRead::Rows), SourceStatus::Ok);
        assert_eq!(bot_status(Auth::Active, LocalRead::Denied), SourceStatus::ReadFailed);
        assert_eq!(bot_status(Auth::Active, LocalRead::Unread), SourceStatus::ReadFailed);
        assert_eq!(bot_status(Auth::NeedsReauth, LocalRead::Denied), SourceStatus::NeedsReauth);
        assert_eq!(bot_status(Auth::NeedsReauth, LocalRead::Rows), SourceStatus::NeedsReauth);
        for local in [LocalRead::Unread, LocalRead::Empty, LocalRead::Rows, LocalRead::Denied] {
            assert_ne!(bot_status(Auth::Active, local), SourceStatus::ServerUnavailable);
            assert_ne!(bot_status(Auth::Missing, local), SourceStatus::ServerUnavailable);
            assert_ne!(bot_status(Auth::NeedsReauth, local), SourceStatus::ServerUnavailable);
            assert_ne!(bot_status(Auth::Active, local), SourceStatus::NeedsReauth);
        }
    }

    #[test]
    fn source_status_names_match_the_ui_contract() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/src/__fixtures__/ipc-names.json");
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut expected: Vec<String> = value["source_status"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_string())
            .collect();
        expected.sort();
        let mut actual: Vec<String> = [
            SourceStatus::Unauthorized,
            SourceStatus::NeedsReauth,
            SourceStatus::ReadFailed,
            SourceStatus::ServerUnavailable,
            SourceStatus::NoRecords,
            SourceStatus::Ok,
        ]
        .into_iter()
        .map(|status| match serde_json::to_value(status).unwrap() {
            serde_json::Value::String(name) => name,
            other => panic!("枚举应序列化成字符串，实际是 {other}"),
        })
        .collect();
        actual.sort();
        assert_eq!(actual, expected);
        assert_eq!(
            actual,
            vec![
                "needs_reauth".to_string(),
                "no_records".to_string(),
                "ok".to_string(),
                "read_failed".to_string(),
                "server_unavailable".to_string(),
                "unauthorized".to_string(),
            ]
        );
    }

    fn temp_hub(name: &str) -> (PathBuf, Hub) {
        let dir = std::env::temp_dir().join(format!("wanxiang-handoff-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("codex")).unwrap();
        std::fs::create_dir_all(dir.join("dsh").join("bot")).unwrap();
        (dir.clone(), open_hub(&dir))
    }

    fn open_hub(dir: &Path) -> Hub {
        let supervisor = Supervisor::new();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(wanxiang_core::Guard::new(
            Arc::clone(&gate) as Arc<dyn ToolScheduler>,
            Arc::clone(&clock),
        ));
        let store = Arc::new(Store::open(&dir.join("wanxiang.sqlite"), guard, clock).unwrap());
        let hub = Hub {
            store,
            secrets: SecretStore::new(),
            scans: ScanSlots::default(),
            gate,
            emit: Arc::new(|_| {}),
        };
        hub
    }

    fn grant(hub: &Hub, tool: ToolId, root: &Path, pattern: &str) {
        hub.store
            .grant(
                tool,
                Capability::Conversation,
                PathScope {
                    root: root.to_path_buf(),
                    pattern: pattern.to_string(),
                },
            )
            .unwrap();
    }

    fn empty_bot_db(path: &Path) {
        let status = std::process::Command::new("sqlite3")
            .arg(path)
            .arg(
                "CREATE TABLE calls (key TEXT, session TEXT, time INTEGER, provider TEXT, model TEXT, kind TEXT, live INTEGER, input INTEGER, cache_read INTEGER, cache_write INTEGER, output INTEGER);
                 CREATE TABLE owners (session TEXT, owner TEXT, name TEXT);",
            )
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn page_statuses(page: &super::ConversationPage) -> (SourceStatus, SourceStatus) {
        let value = serde_json::to_value(page).unwrap();
        assert!(value.get("notes").is_none());
        let codex = serde_json::from_value(value["codex"].clone()).unwrap();
        let bot = serde_json::from_value(value["ds_bot"].clone()).unwrap();
        (codex, bot)
    }

    #[test]
    fn both_sources_are_unauthorized_when_nothing_is_granted() {
        let (dir, hub) = temp_hub("none");
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::Unauthorized, SourceStatus::Unauthorized));
        assert!(page.rows.iter().all(|row| row.tool == "cursor"));
        assert!(!hub.store.settings().unwrap().codex_app_server);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_unauthorized_and_one_without_records() {
        let (dir, hub) = temp_hub("mix-codex");
        grant(&hub, ToolId::Codex, &dir.join("codex"), "sessions/**/*.jsonl");
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::NoRecords, SourceStatus::Unauthorized));
        assert!(page.rows.iter().all(|row| row.tool == "cursor"));

        let (dir, hub) = temp_hub("mix-bot");
        empty_bot_db(&dir.join("dsh/bot/usage.sqlite"));
        grant(&hub, ToolId::DsBot, &dir.join("dsh"), "bot/usage.sqlite*");
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::Unauthorized, SourceStatus::NoRecords));
        assert!(page.rows.iter().all(|row| row.tool == "cursor"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn granted_reads_report_records_or_an_empty_list() {
        let (dir, hub) = temp_hub("rows");
        let codex = dir.join("codex");
        std::fs::create_dir_all(codex.join("sessions/2026/01/01")).unwrap();
        std::fs::write(
            codex.join("sessions/2026/01/01/demo.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"demo\",\"timestamp\":\"2026-01-01T00:00:00Z\"},\"timestamp\":\"2026-01-01T00:00:00Z\"}\n",
        )
        .unwrap();
        grant(&hub, ToolId::Codex, &codex, "sessions/**/*.jsonl");
        empty_bot_db(&dir.join("dsh/bot/usage.sqlite"));
        let status = std::process::Command::new("sqlite3")
            .arg(dir.join("dsh/bot/usage.sqlite"))
            .arg("INSERT INTO calls VALUES ('k', 's', 10, 'p', 'm', 'chat', 1, 1, 0, 0, 1); INSERT INTO owners VALUES ('s', 'bot:demo', '示例');")
            .status()
            .unwrap();
        assert!(status.success());
        grant(&hub, ToolId::DsBot, &dir.join("dsh"), "bot/usage.sqlite*");
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::Ok, SourceStatus::Ok));
        assert_eq!(page.rows.iter().filter(|row| row.tool == "codex").count(), 1);
        assert_eq!(page.rows.iter().filter(|row| row.tool == "ds_bot").count(), 1);
        assert!(!hub.store.settings().unwrap().codex_app_server);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_grant_is_no_records_and_not_server_unavailable() {
        let (dir, hub) = temp_hub("empty-both");
        grant(&hub, ToolId::Codex, &dir.join("codex"), "sessions/**/*.jsonl");
        empty_bot_db(&dir.join("dsh/bot/usage.sqlite"));
        grant(&hub, ToolId::DsBot, &dir.join("dsh"), "bot/usage.sqlite*");
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::NoRecords, SourceStatus::NoRecords));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn needs_reauth_is_not_treated_as_missing_or_a_failed_read() {
        let (dir, hub) = temp_hub("reauth");
        grant(&hub, ToolId::Codex, &dir.join("codex"), "sessions/**/*.jsonl");
        empty_bot_db(&dir.join("dsh/bot/usage.sqlite"));
        grant(&hub, ToolId::DsBot, &dir.join("dsh"), "bot/usage.sqlite*");
        drop(hub);
        for name in ["codex", "dsh"] {
            let live = dir.join(name);
            let moved = dir.join(format!("{name}-moved"));
            std::fs::rename(&live, &moved).unwrap();
            std::fs::create_dir_all(&live).unwrap();
        }
        std::fs::create_dir_all(dir.join("dsh/bot")).unwrap();
        let hub = open_hub(&dir);
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::NeedsReauth, SourceStatus::NeedsReauth));
        assert!(page.rows.iter().all(|row| row.tool == "cursor"));
        assert!(!hub.store.settings().unwrap().codex_app_server);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_granted_read_error_is_read_failed() {
        let (dir, hub) = temp_hub("read-fail");
        let codex = dir.join("codex");
        std::fs::create_dir_all(codex.join("sessions/2026/01/01")).unwrap();
        let blocked = codex.join("sessions/2026/01/01/blocked.jsonl");
        std::fs::write(&blocked, "{}\n").unwrap();
        let mut permissions = std::fs::metadata(&blocked).unwrap().permissions();
        permissions.set_readonly(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0);
        }
        std::fs::set_permissions(&blocked, permissions).unwrap();
        grant(&hub, ToolId::Codex, &codex, "sessions/**/*.jsonl");
        std::fs::write(dir.join("dsh/bot/usage.sqlite"), b"not a database").unwrap();
        grant(&hub, ToolId::DsBot, &dir.join("dsh"), "bot/usage.sqlite*");
        let page = conversations(&hub);
        assert_eq!(page_statuses(&page), (SourceStatus::ReadFailed, SourceStatus::ReadFailed));
        assert!(page.rows.iter().all(|row| row.tool == "cursor"));
        assert!(!hub.store.settings().unwrap().codex_app_server);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&blocked).unwrap().permissions();
            permissions.set_mode(0o644);
            std::fs::set_permissions(&blocked, permissions).unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
