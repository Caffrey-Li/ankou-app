use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;
use wanxiang_adapter_cursor::{cli_chunks, list_cli_chats, parse_messages, CLI_META_PATTERN, CLI_STORE_PATTERN};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-cursor-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

const A: &str = "aaaaaaaa-1111-1111-1111-111111111111";
const B: &str = "bbbbbbbb-2222-2222-2222-222222222222";
const C: &str = "cccccccc-3333-3333-3333-333333333333";

fn chat(root: &Path, id: &str, meta: &str, rows: Option<&[&str]>) {
    let dir = root.join("chats").join("0f0f").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("meta.json"), meta).unwrap();
    let db = Connection::open(dir.join("store.db")).unwrap();
    db.execute_batch("CREATE TABLE meta (key TEXT, value TEXT);").unwrap();
    if let Some(rows) = rows {
        db.execute_batch("CREATE TABLE blobs (id TEXT PRIMARY KEY, data BLOB);").unwrap();
        for (index, row) in rows.iter().enumerate() {
            db.execute("INSERT INTO blobs (id, data) VALUES (?1, ?2)", rusqlite::params![format!("h{index}"), row.as_bytes()])
                .unwrap();
        }
        db.execute("INSERT INTO blobs (id, data) VALUES ('bin', X'0a0b0c')", []).unwrap();
    }
}

fn build(root: &Path) {
    chat(
        root,
        A,
        r#"{"schemaVersion":1,"createdAtMs":1000,"updatedAtMs":9000,"hasConversation":true,"title":"修复构建"}"#,
        Some(&[
            r#"{"role":"system","content":"你是助手"}"#,
            r#"{"role":"user","content":"<user_info>工作区</user_info>"}"#,
            r#"{"role":"user","content":[{"type":"text","text":"帮我修构建"}]}"#,
            r#"{"role":"assistant","content":[{"type":"reasoning","text":"想一想"},{"type":"text","text":"已修好"},{"type":"tool-call","toolName":"x"}]}"#,
            r#"{"role":"tool","content":[{"type":"tool-result","result":"ok"}]}"#,
        ]),
    );
    chat(
        root,
        B,
        r#"{"createdAtMs":2000,"hasConversation":true}"#,
        Some(&[r#"{"role":"user","content":"第一行问题\n第二行"}"#, r#"{"role":"assistant","content":"答复"}"#]),
    );
    // C：没有消息表
    chat(root, C, r#"{"createdAtMs":3000,"hasConversation":true}"#, None);
}

fn grant(guard: &Guard, root: &Path) {
    for pattern in [CLI_META_PATTERN, CLI_STORE_PATTERN] {
        guard
            .grant(
                ToolId::CursorCli,
                Capability::Conversation,
                PathScope { root: root.to_path_buf(), pattern: pattern.to_string() },
            )
            .unwrap();
    }
}

#[test]
fn lists_and_reads_cli_chats_only_with_the_grant() {
    let root = temp("list");
    build(&root);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    assert!(list_cli_chats(&guard, &root).is_err());
    // Cursor 应用的授权不能读命令行的对话
    guard
        .grant(ToolId::Cursor, Capability::Conversation, PathScope { root: root.clone(), pattern: CLI_META_PATTERN.to_string() })
        .unwrap();
    assert!(list_cli_chats(&guard, &root).is_err());
    grant(&guard, &root);
    let chats = list_cli_chats(&guard, &root).unwrap();
    assert_eq!(chats.len(), 2);
    assert_eq!(chats[0].id, A);
    assert_eq!(chats[0].title, "修复构建");
    assert_eq!(chats[0].messages, 2);
    assert_eq!(chats[1].title, "第一行问题");
    assert_eq!(chats[1].updated_ms, 2000);

    let a = cli_chunks(&guard, &root, A).unwrap().unwrap();
    assert_eq!(a, vec!["用户：帮我修构建", "助手：已修好"]);
    assert_eq!(cli_chunks(&guard, &root, C).unwrap().unwrap(), Vec::<String>::new());
    assert_eq!(cli_chunks(&guard, &root, "dddddddd-0000-0000-0000-000000000000").unwrap(), None);
    assert_eq!(cli_chunks(&guard, &root, "../x").unwrap(), None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parser_skips_system_tool_and_context() {
    let rows = [
        r#"{"role":"system","content":"s"}"#.as_bytes().to_vec(),
        r#"{"role":"user","content":"<attached>"}"#.as_bytes().to_vec(),
        r#"not json"#.as_bytes().to_vec(),
        r#"{"role":"assistant","content":[{"type":"text","text":"  "}]}"#.as_bytes().to_vec(),
        r#"{"role":"assistant","content":[{"type":"text","text":"好"}]}"#.as_bytes().to_vec(),
        r#"{"role":"user","content":[{"type":"text","text":"<timestamp>t</timestamp>\n<user_query>\n整理日志\n</user_query>"}]}"#
            .as_bytes()
            .to_vec(),
        r#"{"role":"user","content":[{"type":"text","text":"<system_reminder>x</system_reminder>"}]}"#.as_bytes().to_vec(),
    ];
    assert_eq!(
        parse_messages(rows),
        vec![(false, "好".to_string()), (true, "整理日志".to_string())]
    );
}
