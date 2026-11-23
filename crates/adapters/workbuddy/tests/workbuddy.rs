use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;
use wanxiang_adapter_workbuddy::{
    find_session, parse_session, percent, read_file, session_meta, session_paths, short_title,
    transcript, valid_id, DB_PATTERN, PROJECTS_PATTERN,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-wb-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

const SESSION: &str = concat!(
    r#"{"id":"1","type":"session-meta","meta":{"codebuddy.ai/hostKind":"x"},"sessionId":"ab-12","timestamp":1794372000000}"#,
    "\n",
    r#"{"id":"2","type":"message","role":"user","content":[{"type":"text","text":"<system-reminder>注入</system-reminder>"}],"timestamp":"2026-11-10T01:00:00Z"}"#,
    "\n",
    r#"{"id":"3","type":"message","role":"user","content":[{"type":"input_text","text":"整理发布清单\n按优先级"}],"timestamp":"2026-11-10T01:00:01Z"}"#,
    "\n",
    r#"{"id":"4","type":"message","role":"assistant","content":[{"type":"output_text","text":"好的，三项"},{"type":"tool_use","name":"x"}],"timestamp":"2026-11-10T01:00:02Z"}"#,
    "\n",
    r#"{"type":"file-history-snapshot","snapshot":{},"timestamp":"2026-11-10T01:00:03Z"}"#,
    "\n",
    "坏行\n"
);

#[test]
fn session_title_time_and_transcript() {
    let session = parse_session("ab-12", SESSION.as_bytes());
    assert_eq!(session.first_user.as_deref(), Some("整理发布清单\n按优先级"));
    assert_eq!(session.title(), "整理发布清单");
    assert_eq!(session.messages, 2);
    assert_eq!(session.updated_ms, 1_794_372_000_000);
    assert_eq!(transcript(SESSION.as_bytes()), vec!["用户：整理发布清单\n按优先级", "助手：好的，三项"]);
    assert!(valid_id("01a1d07f-ea5f-791a"));
    assert!(!valid_id("a/b"));
    assert_eq!(short_title(&"长".repeat(50)).chars().count(), 41);
    assert_eq!(percent(50, 200), Some(25));
}

#[test]
fn files_and_database_need_the_grant() {
    let root = temp("grant");
    let project = root.join("projects").join("Users-x-proj");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("ab-12.jsonl"), SESSION).unwrap();
    fs::write(root.join("keyblob"), "secret").unwrap();
    let db = Connection::open(root.join("workbuddy.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE sessions (id text, cwd text, user_id text, title text, custom_title text, status text, created_at text, \
         updated_at text, last_activity_at text, deleted_at text);
         CREATE TABLE session_usage (session_id text, used integer, size integer, updated_at text, credit_json text);
         INSERT INTO sessions (id, title, custom_title, deleted_at) VALUES ('ab-12', '自动标题', '我的标题', NULL);
         INSERT INTO sessions (id, title, custom_title, deleted_at) VALUES ('cd-34', '旧的', NULL, '2026-11-01');
         INSERT INTO session_usage VALUES ('ab-12', 64000, 256000, '', '{}');",
    )
    .unwrap();
    drop(db);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    let paths = session_paths(&root);
    assert_eq!(paths.len(), 1);
    assert!(read_file(&guard, Capability::Conversation, &paths[0]).is_err());
    assert!(session_meta(&guard, &root).is_err());
    for pattern in [PROJECTS_PATTERN, DB_PATTERN] {
        guard
            .grant(
                ToolId::WorkBuddy,
                Capability::Conversation,
                PathScope { root: root.clone(), pattern: pattern.to_string() },
            )
            .unwrap();
    }
    assert!(read_file(&guard, Capability::Conversation, &paths[0]).is_ok());
    assert!(read_file(&guard, Capability::Conversation, &root.join("keyblob")).is_err());
    let meta = session_meta(&guard, &root).unwrap();
    assert_eq!(meta["ab-12"].title, "我的标题");
    assert_eq!(meta["ab-12"].context, Some((64_000, 256_000)));
    assert!(!meta["ab-12"].deleted);
    assert!(meta["cd-34"].deleted);
    assert_eq!(find_session(&root, "ab-12"), Some(paths[0].clone()));
    assert_eq!(find_session(&root, "../x"), None);
    let _ = fs::remove_dir_all(&root);
}
