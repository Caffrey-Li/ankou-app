use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use wanxiang_adapter_codex::{
    context_percent, find_codex_cli, list_jsonl, list_threads, parse_jsonl_head, read_jsonl,
    read_thread, rollout_paths, title_of,
};
use wanxiang_adapter_cursor::{chat_chunks, list_chats};
#[cfg(debug_assertions)]
use wanxiang_adapter_cursor::read_fixture;
use wanxiang_adapter_dsh::{
    fetch_summary, list_sessions, owner_kind, parse_session, read_session_bytes, session_chunks, session_paths,
};
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
    pub harness: SourceStatus,
    pub cursor: SourceStatus,
    pub claude: SourceStatus,
    pub antigravity: SourceStatus,
    pub workbuddy: SourceStatus,
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

/// 读会话文件可能要几秒（Harness 会话需要解压），放到后台线程，不卡界面。
#[tauri::command]
pub async fn list_conversations<R: tauri::Runtime>(
    app: AppHandle<R>,
) -> Result<ConversationPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let hub = app.state::<Hub>();
        conversations(&hub)
    })
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn prepare_handoff<R: tauri::Runtime>(
    app: AppHandle<R>,
    tool: String,
    id: String,
) -> Result<HandoffDraft, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let hub = app.state::<Hub>();
        draft(&hub, &tool, &id)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn begin_summary(hub: State<'_, Hub>, transcript: String) -> Result<SummaryPreview, String> {
    let linked = hub
        .store
        .guard()
        .account_link_active(ToolId::DeepSeekHarness);
    // 用本地记号判断是否存过 Key，不在主线程上读钥匙串
    let key_saved = hub.store.key_saved_flag().ok().flatten().unwrap_or(false);
    let body = summary_body(&transcript);
    let token = authorize_summary(hub.store.guard(), linked, key_saved, body.as_bytes())
        .map_err(summary_text)?;
    Ok(SummaryPreview {
        token: token.to_hex(),
        chars: summary_chars(&body),
        body,
    })
}

/// 读钥匙串和联网都放到后台线程，界面不会在等待时卡住。
#[tauri::command]
pub async fn send_summary<R: tauri::Runtime>(
    app: AppHandle<R>,
    token: String,
    body: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let hub = app.state::<Hub>();
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
    })
    .await
    .map_err(|err| err.to_string())?
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
                // 调用记录里也有 Harness 与内部代理的会话，只列 Bot 与群聊
                let heads: Vec<_> = heads
                    .into_iter()
                    .filter(|head| owner_kind(&head.owner).counted())
                    .collect();
                let read = if heads.is_empty() {
                    LocalRead::Empty
                } else {
                    LocalRead::Rows
                };
                let files = bot_sessions(hub.store.guard(), &grant.root);
                for head in heads {
                    let file = files.iter().find(|session| session.id == head.session);
                    let title = if !head.name.is_empty() {
                        head.name
                    } else if let Some(file) = file.filter(|file| !file.title.is_empty()) {
                        file.title.clone()
                    } else {
                        "未命名对话".to_string()
                    };
                    let percent = file.and_then(|file| file.context_percent());
                    let updated = file
                        .map(|file| file.updated_ms.max(head.updated_ms))
                        .unwrap_or(head.updated_ms);
                    rows.push(ConversationRow {
                        id: head.session,
                        tool: "ds_bot".to_string(),
                        title,
                        updated: updated.to_string(),
                        percent,
                        cwd: None,
                        status: String::new(),
                        context: if percent.is_some() {
                            String::new()
                        } else if file.is_some() {
                            "未知".to_string()
                        } else {
                            "暂不提供".to_string()
                        },
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
    let (harness_auth, harness_grant) = conversation_auth(&grants, ToolId::DeepSeekHarness);
    let harness_local = if let Some(grant) = harness_grant {
        harness_rows(hub.store.guard(), &grant.root, &mut rows)
    } else {
        LocalRead::Unread
    };
    let (cursor_auth, cursor_grant) = conversation_auth(&grants, ToolId::Cursor);
    let cursor_local = if let Some(grant) = cursor_grant {
        cursor_rows(hub.store.guard(), &grant.root, &mut rows)
    } else {
        LocalRead::Unread
    };
    let (claude_auth, _) = conversation_auth(&grants, ToolId::Claude);
    let claude_local = match claude_grants(&grants) {
        Some((records, desktop)) => claude_rows(hub.store.guard(), &records, desktop.as_deref(), &mut rows),
        None => LocalRead::Unread,
    };
    let (antigravity_auth, _) = conversation_auth(&grants, ToolId::Antigravity);
    let antigravity_roots = antigravity_roots(&grants);
    let antigravity_local = if antigravity_roots.is_empty() {
        LocalRead::Unread
    } else {
        antigravity_rows(hub.store.guard(), &antigravity_roots, &mut rows)
    };
    let (workbuddy_auth, _) = conversation_auth(&grants, ToolId::WorkBuddy);
    let workbuddy_local = match workbuddy_root(&grants) {
        Some(root) => workbuddy_rows(hub.store.guard(), &root, &mut rows),
        None => LocalRead::Unread,
    };
    append_cursor_fixture(hub, &mut rows);
    ConversationPage {
        rows,
        codex: codex_status(codex_auth, codex_server, codex_local),
        ds_bot: bot_status(bot_auth, bot_local),
        harness: bot_status(harness_auth, harness_local),
        cursor: bot_status(cursor_auth, cursor_local),
        claude: bot_status(claude_auth, claude_local),
        antigravity: bot_status(antigravity_auth, antigravity_local),
        workbuddy: bot_status(workbuddy_auth, workbuddy_local),
    }
}

/// 取最近几轮做交接草稿；超长时只留最近部分。
fn recent_draft(chunks: &[String]) -> HandoffDraft {
    let start = chunks.len().saturating_sub(wanxiang_core::RECENT_TURNS);
    let (text, truncated) = wanxiang_core::take_recent(&chunks[start..]);
    let truncated = truncated || start > 0;
    HandoffDraft {
        text,
        truncated,
        note: if truncated {
            "长对话只取了最近部分".to_string()
        } else {
            String::new()
        },
    }
}

/// Antigravity 对话的授权目录（桌面应用与命令行版各一个，列表库授权了才算）。
fn antigravity_roots(grants: &[wanxiang_core::StoredGrant]) -> Vec<std::path::PathBuf> {
    grants
        .iter()
        .filter(|grant| {
            grant.tool == ToolId::Antigravity
                && grant.capability == Capability::Conversation
                && grant.status == GrantStatus::Active
                && grant.pattern == wanxiang_adapter_antigravity::SUMMARY_PATTERN
        })
        .map(|grant| grant.root.clone())
        .collect()
}

/// Antigravity 对话：列表库给出标题、时间；步骤库给出上下文占用。子代理的对话不单列。
fn antigravity_rows(
    guard: &wanxiang_core::Guard,
    roots: &[std::path::PathBuf],
    rows: &mut Vec<ConversationRow>,
) -> LocalRead {
    use wanxiang_adapter_antigravity as ag;
    let mut readable = 0;
    let mut listed = 0;
    for root in roots {
        let Ok(list) = ag::list_conversations(guard, Capability::Conversation, root) else {
            continue;
        };
        readable += 1;
        for item in list.into_iter().filter(|item| !item.nested) {
            let context = ag::conversation_db(root, &item.id)
                .and_then(|db| ag::context(guard, &db).ok().flatten());
            let percent = context.and_then(|(used, limit)| ag::percent(used, limit));
            listed += 1;
            rows.push(ConversationRow {
                id: item.id,
                tool: "antigravity".to_string(),
                title: if item.title.is_empty() {
                    "未命名对话".to_string()
                } else {
                    item.title
                },
                updated: item.updated_ms.to_string(),
                percent,
                cwd: None,
                status: String::new(),
                context: if percent.is_some() {
                    String::new()
                } else {
                    "未知".to_string()
                },
                source: "summary_db".to_string(),
            });
        }
    }
    if readable == 0 {
        LocalRead::Denied
    } else if listed == 0 {
        LocalRead::Empty
    } else {
        LocalRead::Rows
    }
}

fn antigravity_draft(hub: &Hub, id: &str) -> Result<HandoffDraft, String> {
    let grants = hub.store.grants().map_err(|err| err.to_string())?;
    let roots = antigravity_roots(&grants);
    if roots.is_empty() {
        return Err("Antigravity 对话未授权".to_string());
    }
    let mut failed = false;
    for root in &roots {
        match wanxiang_adapter_antigravity::transcript(hub.store.guard(), root, id) {
            Ok(Some(chunks)) => return Ok(recent_draft(&chunks)),
            Ok(None) => {}
            Err(_) => failed = true,
        }
    }
    Err(if failed {
        "读取 Antigravity 对话失败".to_string()
    } else {
        "这条对话已经不在，无法交接".to_string()
    })
}

/// WorkBuddy 对话的授权目录：会话记录授权了才算（会话表可选）。
fn workbuddy_root(grants: &[wanxiang_core::StoredGrant]) -> Option<std::path::PathBuf> {
    grants
        .iter()
        .find(|grant| {
            grant.tool == ToolId::WorkBuddy
                && grant.capability == Capability::Conversation
                && grant.status == GrantStatus::Active
                && grant.pattern == wanxiang_adapter_workbuddy::PROJECTS_PATTERN
        })
        .map(|grant| grant.root.clone())
}

/// WorkBuddy 对话：会话记录给出首句与时间；会话表给出标题、删除标记与上下文占用。
fn workbuddy_rows(
    guard: &wanxiang_core::Guard,
    root: &std::path::Path,
    rows: &mut Vec<ConversationRow>,
) -> LocalRead {
    use wanxiang_adapter_workbuddy as wb;
    let meta = wb::session_meta(guard, root).unwrap_or_default();
    let paths = wb::session_paths(root);
    if paths.is_empty() {
        return LocalRead::Empty;
    }
    let mut readable = 0;
    let mut listed = 0;
    for path in paths {
        let Ok(bytes) = wb::read_file(guard, Capability::Conversation, &path) else {
            continue;
        };
        readable += 1;
        let id = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !wb::valid_id(&id) {
            continue;
        }
        let session = wb::parse_session(&id, &bytes);
        drop(bytes);
        let info = meta.get(&id);
        if session.messages == 0 || info.is_some_and(|info| info.deleted) {
            continue;
        }
        let title = info
            .map(|info| info.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| session.title());
        let percent = info
            .and_then(|info| info.context)
            .and_then(|(used, limit)| wb::percent(used, limit));
        listed += 1;
        rows.push(ConversationRow {
            id,
            tool: "workbuddy".to_string(),
            title: if title.is_empty() {
                "未命名对话".to_string()
            } else {
                title
            },
            updated: session.updated_ms.to_string(),
            percent,
            cwd: None,
            status: String::new(),
            context: if percent.is_some() {
                String::new()
            } else {
                "未知".to_string()
            },
            source: "session_file".to_string(),
        });
    }
    if readable == 0 {
        LocalRead::Denied
    } else if listed == 0 {
        LocalRead::Empty
    } else {
        LocalRead::Rows
    }
}

fn workbuddy_draft(hub: &Hub, id: &str) -> Result<HandoffDraft, String> {
    let grants = hub.store.grants().map_err(|err| err.to_string())?;
    let Some(root) = workbuddy_root(&grants) else {
        return Err("WorkBuddy 对话未授权".to_string());
    };
    let path = wanxiang_adapter_workbuddy::find_session(&root, id)
        .ok_or_else(|| "这条对话已经不在，无法交接".to_string())?;
    let bytes =
        wanxiang_adapter_workbuddy::read_file(hub.store.guard(), Capability::Conversation, &path)
            .map_err(|_| "读取 WorkBuddy 对话失败".to_string())?;
    let chunks = wanxiang_adapter_workbuddy::transcript(&bytes);
    drop(bytes);
    Ok(recent_draft(&chunks))
}

/// Claude 对话的两个授权根：会话记录（必需）与桌面应用的会话标题（可选）。
fn claude_grants(
    grants: &[wanxiang_core::StoredGrant],
) -> Option<(std::path::PathBuf, Option<std::path::PathBuf>)> {
    let find = |pattern: &str| {
        grants
            .iter()
            .find(|grant| {
                grant.tool == ToolId::Claude
                    && grant.capability == Capability::Conversation
                    && grant.status == GrantStatus::Active
                    && grant.pattern == pattern
            })
            .map(|grant| grant.root.clone())
    };
    let records = find(wanxiang_adapter_claude::PROJECTS_PATTERN)?;
    Some((records, find(wanxiang_adapter_claude::DESKTOP_SESSIONS_PATTERN)))
}

/// Claude 对话：会话记录给出时间与用量，桌面应用给出标题；归档的会话不列。
fn claude_rows(
    guard: &wanxiang_core::Guard,
    records: &std::path::Path,
    desktop: Option<&std::path::Path>,
    rows: &mut Vec<ConversationRow>,
) -> LocalRead {
    use wanxiang_adapter_claude as claude;
    let mut titles = std::collections::HashMap::new();
    let mut archived = std::collections::HashSet::new();
    if let Some(desktop) = desktop {
        for path in claude::desktop_session_paths(desktop) {
            let Ok(bytes) = claude::read_file(guard, Capability::Conversation, &path) else {
                continue;
            };
            if let Some(session) = claude::desktop_session(&bytes) {
                if session.archived {
                    archived.insert(session.cli_session_id.clone());
                }
                if !session.title.is_empty() {
                    titles.insert(session.cli_session_id, session.title);
                }
            }
        }
    }
    let paths = claude::session_paths(records);
    if paths.is_empty() {
        return LocalRead::Empty;
    }
    let mut readable = 0;
    let mut listed = 0;
    for path in paths {
        let Ok(bytes) = claude::read_file(guard, Capability::Conversation, &path) else {
            continue;
        };
        readable += 1;
        let id = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !claude::valid_id(&id) || archived.contains(&id) {
            continue;
        }
        let session = claude::parse_session(&id, &bytes);
        drop(bytes);
        if session.calls.is_empty() && session.first_user.is_none() {
            continue;
        }
        let title = titles.get(&id).cloned().unwrap_or_else(|| session.title());
        listed += 1;
        rows.push(ConversationRow {
            id,
            tool: "claude".to_string(),
            title: if title.is_empty() {
                "未命名对话".to_string()
            } else {
                title
            },
            updated: session.updated_ms.to_string(),
            // 本机记录里没有上下文窗口大小，比例未知；不按猜的窗口折算
            percent: None,
            cwd: None,
            status: String::new(),
            context: "未知".to_string(),
            source: "session_file".to_string(),
        });
    }
    if readable == 0 {
        LocalRead::Denied
    } else if listed == 0 {
        LocalRead::Empty
    } else {
        LocalRead::Rows
    }
}

fn claude_draft(hub: &Hub, id: &str) -> Result<HandoffDraft, String> {
    let grants = hub.store.grants().map_err(|err| err.to_string())?;
    let Some((records, _)) = claude_grants(&grants) else {
        return Err("Claude 对话未授权".to_string());
    };
    let path = wanxiang_adapter_claude::find_session(&records, id)
        .ok_or_else(|| "这条对话已经不在，无法交接".to_string())?;
    let bytes =
        wanxiang_adapter_claude::read_file(hub.store.guard(), Capability::Conversation, &path)
            .map_err(|_| "读取 Claude 对话失败".to_string())?;
    let chunks = wanxiang_adapter_claude::transcript(&bytes);
    drop(bytes);
    let start = chunks.len().saturating_sub(wanxiang_core::RECENT_TURNS);
    let (text, truncated) = wanxiang_core::take_recent(&chunks[start..]);
    let truncated = truncated || start > 0;
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

/// Cursor 对话：只读本机对话数据库里与对话有关的键，列标题、更新时间与上下文占用。
fn cursor_rows(
    guard: &wanxiang_core::Guard,
    root: &std::path::Path,
    rows: &mut Vec<ConversationRow>,
) -> LocalRead {
    match list_chats(guard, root) {
        Ok(chats) => {
            let read = if chats.is_empty() {
                LocalRead::Empty
            } else {
                LocalRead::Rows
            };
            for chat in chats {
                rows.push(ConversationRow {
                    id: chat.id,
                    tool: "cursor".to_string(),
                    title: if chat.title.is_empty() {
                        "未命名对话".to_string()
                    } else {
                        chat.title
                    },
                    updated: chat.updated_ms.to_string(),
                    percent: chat.percent,
                    cwd: None,
                    status: String::new(),
                    context: if chat.percent.is_some() {
                        String::new()
                    } else {
                        "未知".to_string()
                    },
                    source: "cursor_db".to_string(),
                });
            }
            read
        }
        Err(_) => LocalRead::Denied,
    }
}

fn cursor_draft(hub: &Hub, id: &str) -> Result<HandoffDraft, String> {
    let grants = hub.store.grants().map_err(|err| err.to_string())?;
    let Some(grant) = grants.iter().find(|grant| {
        grant.tool == ToolId::Cursor
            && grant.capability == Capability::Conversation
            && grant.status == GrantStatus::Active
    }) else {
        return cursor_fixture_draft(hub, id);
    };
    let chunks = chat_chunks(hub.store.guard(), &grant.root, id)
        .map_err(|_| "读取 Cursor 对话失败".to_string())?
        .ok_or_else(|| "这条对话已经不在，无法交接".to_string())?;
    let start = chunks.len().saturating_sub(wanxiang_core::RECENT_TURNS);
    let (text, truncated) = wanxiang_core::take_recent(&chunks[start..]);
    let truncated = truncated || start > 0;
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

/// Harness 对话：本机会话文件里的标题、更新时间与上下文占用；Bot 工作区的会话不列在这里。
fn harness_rows(
    guard: &wanxiang_core::Guard,
    root: &std::path::Path,
    rows: &mut Vec<ConversationRow>,
) -> LocalRead {
    let paths = session_paths(root);
    if paths.is_empty() {
        return LocalRead::Empty;
    }
    let mut readable = 0;
    let mut listed = 0;
    for path in paths {
        let Ok(bytes) =
            read_session_bytes(guard, ToolId::DeepSeekHarness, Capability::Conversation, &path)
        else {
            continue;
        };
        readable += 1;
        let Some(session) = parse_session(&bytes) else {
            continue;
        };
        if session.is_bot() {
            continue;
        }
        let percent = session.context_percent();
        listed += 1;
        rows.push(ConversationRow {
            id: session.id.clone(),
            tool: "deepseek_harness".to_string(),
            title: if session.title.is_empty() {
                "未命名对话".to_string()
            } else {
                session.title.clone()
            },
            updated: session.updated_ms.to_string(),
            percent,
            cwd: (!session.cwd.is_empty()).then(|| session.cwd.clone()),
            status: String::new(),
            context: if percent.is_some() {
                String::new()
            } else {
                "未知".to_string()
            },
            source: "session_file".to_string(),
        });
    }
    if readable == 0 {
        LocalRead::Denied
    } else if listed == 0 {
        LocalRead::Empty
    } else {
        LocalRead::Rows
    }
}

/// Bot 工作区的会话文件，只取元数据（标题、上下文占用、更新时间）。
fn bot_sessions(
    guard: &wanxiang_core::Guard,
    root: &std::path::Path,
) -> Vec<wanxiang_adapter_dsh::DshSession> {
    session_paths(root)
        .into_iter()
        .filter_map(|path| {
            read_session_bytes(guard, ToolId::DsBot, Capability::Conversation, &path).ok()
        })
        .filter_map(|bytes| parse_session(&bytes))
        .filter(|session| session.is_bot())
        .collect()
}

/// 按会话编号找到会话文件，取最近的用户与助手文字。
fn dsh_draft(
    guard: &wanxiang_core::Guard,
    tool: ToolId,
    root: &std::path::Path,
    id: &str,
    want_bot: bool,
) -> Result<HandoffDraft, String> {
    for path in session_paths(root) {
        let Ok(bytes) = read_session_bytes(guard, tool, Capability::Conversation, &path) else {
            continue;
        };
        let Some(session) = parse_session(&bytes) else {
            continue;
        };
        if session.id != id || session.is_bot() != want_bot {
            continue;
        }
        let chunks = session_chunks(&bytes);
        let start = chunks.len().saturating_sub(wanxiang_core::RECENT_TURNS);
        let (text, truncated) = wanxiang_core::take_recent(&chunks[start..]);
        let truncated = truncated || start > 0;
        return Ok(HandoffDraft {
            text,
            truncated,
            note: if truncated {
                "长对话只取了最近部分".to_string()
            } else {
                String::new()
            },
        });
    }
    Err("这条对话已经不在，无法交接".to_string())
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
            dsh_draft(hub.store.guard(), ToolId::DsBot, &grant.root, id, true)
        }
        "cursor" => cursor_draft(hub, id),
        "claude" => claude_draft(hub, id),
        "antigravity" => antigravity_draft(hub, id),
        "workbuddy" => workbuddy_draft(hub, id),
        "deepseek_harness" => {
            let grants = hub.store.grants().map_err(|err| err.to_string())?;
            let Some(grant) = grants.iter().find(|grant| {
                grant.tool == ToolId::DeepSeekHarness
                    && grant.capability == Capability::Conversation
                    && grant.status == GrantStatus::Active
            }) else {
                return Err("DeepSeek Harness 对话未授权".to_string());
            };
            dsh_draft(hub.store.guard(), ToolId::DeepSeekHarness, &grant.root, id, false)
        }
        _ => Err("这条对话已经不在，无法交接".to_string()),
    }
}

#[cfg(debug_assertions)]
fn append_cursor_fixture(hub: &Hub, rows: &mut Vec<ConversationRow>) {
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
}

#[cfg(not(debug_assertions))]
fn append_cursor_fixture(_hub: &Hub, _rows: &mut Vec<ConversationRow>) {}

#[cfg(debug_assertions)]
fn cursor_fixture_draft(hub: &Hub, id: &str) -> Result<HandoffDraft, String> {
    if std::env::var_os("WANXIANG_CURSOR_FIXTURE").is_none() {
        return Err("Cursor 对话未授权".to_string());
    }
    let text = read_fixture(hub.store.guard()).map_err(|_| "Cursor 对话未授权".to_string())?;
    if !text.contains(&format!("\"id\": \"{id}\"")) && !text.contains(&format!("\"id\":\"{id}\"")) {
        return Err("这条对话已经不在，无法交接".to_string());
    }
    Ok(HandoffDraft {
        text: "夹具正文".to_string(),
        truncated: false,
        note: String::new(),
    })
}

#[cfg(not(debug_assertions))]
fn cursor_fixture_draft(_hub: &Hub, _id: &str) -> Result<HandoffDraft, String> {
    Err("Cursor 对话未授权".to_string())
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

    use super::{
        bot_status, codex_status, conversations, draft, Auth, LocalRead, ServerAttempt, SourceStatus,
    };
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

    fn raw_frame(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x38];
        for (index, block) in data.chunks(100 * 1024).enumerate() {
            let last = u32::from((index + 1) * 100 * 1024 >= data.len());
            let header = last | ((block.len() as u32) << 3);
            out.extend_from_slice(&header.to_le_bytes()[..3]);
            out.extend_from_slice(block);
        }
        out
    }

    fn dsh_session(root: &Path, workspace: &str, id: &str, cwd: &str, title: &str) {
        let dir = root.join("sessions").join(workspace).join(format!("session-{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        let body = [
            format!(r#"{{"type":"session","id":"{id}","createdAt":1000,"cwd":"{cwd}"}}"#),
            format!(r#"{{"type":"session/title","time":1100,"data":{{"title":"{title}"}}}}"#),
            r#"{"type":"request/context","time":1200,"data":{"contextWindow":1000}}"#.to_string(),
            r#"{"type":"user/message","time":1300,"data":{"content":[{"type":"text","text":"问题"}]}}"#.to_string(),
            r#"{"type":"assistant/message","time":1400,"data":{"message":{"content":[{"type":"text","text":"回答"}]},"usage":{"inputTokens":800,"cacheReadTokens":0,"cacheWriteTokens":0,"outputTokens":50,"totalTokens":850}}}"#.to_string(),
        ]
        .join("\n");
        std::fs::write(dir.join("session.v4.jsonl.zstd"), raw_frame(body.as_bytes())).unwrap();
    }

    #[test]
    fn harness_lists_its_own_sessions_and_bot_sessions_stay_with_ds_bot() {
        let (dir, hub) = temp_hub("harness");
        let dsh = dir.join("dsh");
        dsh_session(&dsh, "--Users-me-project--", "h1", "/Users/me/project", "发布清单");
        dsh_session(&dsh, "--Users-me-.dsh-bot-workspace--", "b1", "/Users/me/.dsh/bot/workspace", "群聊");
        grant(&hub, ToolId::DeepSeekHarness, &dsh, wanxiang_adapter_dsh::HARNESS_PATTERN);
        let page = conversations(&hub);
        assert_eq!(page.harness, SourceStatus::Ok);
        let harness: Vec<_> = page.rows.iter().filter(|row| row.tool == "deepseek_harness").collect();
        assert_eq!(harness.len(), 1);
        assert_eq!(harness[0].title, "发布清单");
        assert_eq!(harness[0].percent, Some(85));
        let text = draft(&hub, "deepseek_harness", "h1").unwrap().text;
        assert!(text.contains("用户：问题"));
        assert!(text.contains("助手：回答"));
        assert!(draft(&hub, "deepseek_harness", "b1").is_err());

        empty_bot_db(&dsh.join("bot/usage.sqlite"));
        let status = std::process::Command::new("sqlite3")
            .arg(dsh.join("bot/usage.sqlite"))
            .arg("INSERT INTO calls VALUES ('k1', 'b1', 10, 'p', 'm', 'turn', 0, 1, 0, 0, 1); INSERT INTO calls VALUES ('k2', 'h1', 10, 'p', 'm', 'turn', 0, 1, 0, 0, 1); INSERT INTO owners VALUES ('b1', 'bot:demo', '示例 Bot');")
            .status()
            .unwrap();
        assert!(status.success());
        grant(&hub, ToolId::DsBot, &dsh, "bot/usage.sqlite*");
        grant(&hub, ToolId::DsBot, &dsh, wanxiang_adapter_dsh::BOT_SESSION_PATTERN);
        let page = conversations(&hub);
        let bot: Vec<_> = page.rows.iter().filter(|row| row.tool == "ds_bot").collect();
        assert_eq!(bot.len(), 1, "Harness 的会话不应出现在 DS Bot 里");
        assert_eq!(bot[0].title, "示例 Bot");
        assert_eq!(bot[0].percent, Some(85));
        let text = draft(&hub, "ds_bot", "b1").unwrap().text;
        assert!(text.contains("助手：回答"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
