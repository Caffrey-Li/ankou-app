use std::fs;
use std::sync::Arc;

use wanxiang_adapter_dsh::{list_sessions, session_transcript};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

#[test]
fn usage_grant_does_not_list_bot_sessions() {
    let root = std::env::temp_dir().join(format!("wanxiang-dsh-conv-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bot")).unwrap();
    let db = root.join("bot/usage.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE calls (key TEXT, session TEXT, time INTEGER, provider TEXT, model TEXT, kind TEXT, live INTEGER, input INTEGER, cache_read INTEGER, cache_write INTEGER, output INTEGER);
         CREATE TABLE owners (session TEXT, owner TEXT, name TEXT);
         INSERT INTO calls VALUES ('k', 's', 10, 'p', 'm', 'chat', 1, 1, 0, 0, 1);
         INSERT INTO owners VALUES ('s', 'bot:demo', '示例 Bot');",
    )
    .unwrap();
    drop(conn);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    guard
        .grant(
            ToolId::DsBot,
            Capability::Usage,
            PathScope {
                root: root.clone(),
                pattern: "bot/usage.sqlite*".to_string(),
            },
        )
        .unwrap();
    assert!(list_sessions(&guard, &db).is_err());
    assert!(session_transcript(&guard, &db).is_err());
    guard
        .grant(
            ToolId::DsBot,
            Capability::Conversation,
            PathScope {
                root: root.clone(),
                pattern: "bot/usage.sqlite*".to_string(),
            },
        )
        .unwrap();
    let heads = list_sessions(&guard, &db).unwrap();
    assert_eq!(heads[0].name, "示例 Bot");
    assert!(session_transcript(&guard, &db).unwrap().is_none());
    let _ = fs::remove_dir_all(&root);
}
