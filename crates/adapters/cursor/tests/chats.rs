use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;
use wanxiang_adapter_cursor::{chat_chunks, list_chats, state_db, valid_id, STATE_PATTERN};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-cursor-chats-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

const A: &str = "11111111-1111-1111-1111-111111111111";
const B: &str = "22222222-2222-2222-2222-222222222222";
const C: &str = "33333333-3333-3333-3333-333333333333";

fn build(root: &Path) {
    let path = state_db(root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
         CREATE TABLE cursorDiskKV (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);",
    )
    .unwrap();
    let put = |key: &str, value: &str| {
        db.execute("INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)", [key, value]).unwrap();
    };
    db.execute("INSERT INTO ItemTable (key, value) VALUES ('loginItem/demo', 'SECRET')", []).unwrap();
    // A：有名字、有头部顺序、用 token 算百分比
    put(
        &format!("composerData:{A}"),
        &format!(r#"{{"composerId":"{A}","name":"整理发布","createdAt":1000,"lastUpdatedAt":5000,"contextTokensUsed":450,"contextTokenLimit":1000,"blobEncryptionKey":"K"}}"#),
    );
    put(
        &format!("composerConvHeaders:{A}"),
        r#"[{"bubbleId":"b2","type":2,"grouping":{"hasText":true}},{"bubbleId":"b1","type":1},{"bubbleId":"b3","type":2,"grouping":{"hasText":false}}]"#,
    );
    put(&format!("bubbleId:{A}:b1"), r#"{"type":1,"text":"先列出待办","createdAt":"2026-01-01T00:00:02Z"}"#);
    put(&format!("bubbleId:{A}:b2"), r#"{"type":2,"text":"好的","createdAt":"2026-01-01T00:00:03Z"}"#);
    put(&format!("bubbleId:{A}:b3"), r#"{"type":2,"text":"","createdAt":"2026-01-01T00:00:04Z"}"#);
    // B：没有名字、没有头部，按时间排；百分比来自 contextUsagePercent
    put(
        &format!("composerData:{B}"),
        &format!(r#"{{"composerId":"{B}","createdAt":2000,"contextUsagePercent":12.6}}"#),
    );
    put(&format!("bubbleId:{B}:y"), r#"{"type":2,"text":"答复","createdAt":"2026-01-01T00:00:09Z"}"#);
    put(&format!("bubbleId:{B}:x"), r#"{"type":1,"text":"第一行问题\n第二行","createdAt":"2026-01-01T00:00:08Z"}"#);
    // C：子代理，不列
    put(
        &format!("composerData:{C}"),
        &format!(r#"{{"composerId":"{C}","createdAt":3000,"subagentInfo":{{"x":1}}}}"#),
    );
    put(&format!("bubbleId:{C}:z"), r#"{"type":1,"text":"内部","createdAt":"2026-01-01T00:00:01Z"}"#);
}

#[test]
fn lists_and_reads_only_with_the_grant() {
    let root = temp("list");
    build(&root);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    assert!(list_chats(&guard, &root).is_err());
    guard
        .grant(
            ToolId::Cursor,
            Capability::Conversation,
            PathScope { root: root.clone(), pattern: STATE_PATTERN.to_string() },
        )
        .unwrap();
    let chats = list_chats(&guard, &root).unwrap();
    assert_eq!(chats.len(), 2);
    assert_eq!(chats[0].id, A);
    assert_eq!(chats[0].title, "整理发布");
    assert_eq!(chats[0].updated_ms, 5000);
    assert_eq!(chats[0].percent, Some(45));
    assert_eq!(chats[0].messages, 2);
    assert_eq!(chats[1].messages, 2);
    assert_eq!(chats[1].title, "第一行问题");
    assert_eq!(chats[1].updated_ms, 2000);
    assert_eq!(chats[1].percent, Some(13));

    let a = chat_chunks(&guard, &root, A).unwrap().unwrap();
    assert_eq!(a, vec!["助手：好的", "用户：先列出待办"]);
    let b = chat_chunks(&guard, &root, B).unwrap().unwrap();
    assert_eq!(b, vec!["用户：第一行问题\n第二行", "助手：答复"]);
    assert_eq!(chat_chunks(&guard, &root, "44444444-4444-4444-4444-444444444444").unwrap(), None);
    // 拼不出白名单外的键
    assert_eq!(chat_chunks(&guard, &root, "x:../ItemTable").unwrap(), None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ids_are_restricted() {
    assert!(valid_id("bc-00d70b06-3e42-472e-8694-541c051206b7"));
    assert!(valid_id(A));
    assert!(!valid_id("loginItem/demo"));
    assert!(!valid_id("a:b"));
    assert!(!valid_id(""));
}

#[test]
fn other_capabilities_cannot_open_the_database() {
    let root = temp("cap");
    build(&root);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    guard
        .grant(
            ToolId::Cursor,
            Capability::SpaceScan,
            PathScope { root: root.clone(), pattern: "**".to_string() },
        )
        .unwrap();
    assert!(list_chats(&guard, &root).is_err());
    let _ = fs::remove_dir_all(&root);
}
