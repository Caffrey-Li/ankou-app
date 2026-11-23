use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;
use wanxiang_adapter_antigravity::{
    calls, context, context_of, conversation_paths, list_conversations, percent, proto_fields,
    step_call, text_ms, transcript, valid_id, Proto, CONVERSATIONS_PATTERN, SUMMARY_PATTERN,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-ag-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn num(field: u32, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    varint(u64::from(field) << 3, &mut out);
    varint(value, &mut out);
    out
}

fn msg(field: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    varint((u64::from(field) << 3) | 2, &mut out);
    varint(body.len() as u64, &mut out);
    out.extend_from_slice(body);
    out
}

fn step_metadata(seconds: u64, input: u64, output: u64, cache: u64) -> Vec<u8> {
    let mut created = num(1, seconds);
    created.extend(num(2, 5));
    let mut usage = num(1, 1401);
    usage.extend(num(2, input));
    usage.extend(num(3, output));
    usage.extend(num(5, cache));
    usage.extend(num(6, 26));
    let mut out = msg(1, &created);
    out.extend(num(3, 2));
    out.extend(msg(9, &usage));
    out.extend(msg(12, b"0123"));
    out
}

fn gen_metadata(used: u64, limit: u64) -> Vec<u8> {
    let mut window = num(1, used);
    window.extend(num(4, limit));
    let nine = [num(2, 1), msg(10, &window)].concat();
    let first = [num(3, 1418), msg(9, &nine)].concat();
    msg(1, &first)
}

#[test]
fn protobuf_fields_and_bad_input() {
    let raw = [num(1, 300), msg(2, b"ab")].concat();
    let fields = proto_fields(&raw).unwrap();
    assert_eq!(fields[0], (1, Proto::Varint(300)));
    assert_eq!(fields[1], (2, Proto::Bytes(b"ab")));
    assert!(proto_fields(&[0x12, 0x05, b'a']).is_none());
    assert!(proto_fields(&[0x07]).is_none());
}

#[test]
fn step_usage_and_context() {
    let call = step_call(&step_metadata(1_794_761_022, 13_207, 149, 8_410)).unwrap();
    assert_eq!(call.time_ms, 1_794_761_022_000);
    assert_eq!(call.tokens, 13_207 + 149 + 8_410);
    // 没有用量的步骤（用户输入）不算调用
    let mut plain = msg(1, &num(1, 1_794_761_022));
    plain.extend(num(3, 4));
    assert!(step_call(&plain).is_none());
    assert_eq!(context_of(&gen_metadata(25_531, 256_000)), Some((25_531, 256_000)));
    assert_eq!(context_of(&gen_metadata(5, 0)), None);
    assert_eq!(percent(25_531, 256_000), Some(10));
    assert_eq!(percent(300_000, 256_000), Some(100));
}

#[test]
fn times_and_ids() {
    assert_eq!(text_ms("1970-01-01 00:00:01.5"), Some(1_500));
    assert_eq!(text_ms("1970-01-01T00:00:02Z"), Some(2_000));
    assert_eq!(text_ms("1970-01-01 00:00:03 +0000 UTC"), Some(3_000));
    assert!(valid_id("2255d05d-398d-430a-8788-2d31d26f8d97"));
    assert!(!valid_id("../x"));
}

fn build(root: &PathBuf) {
    fs::create_dir_all(root.join("conversations")).unwrap();
    let summary = Connection::open(root.join("conversation_summaries.db")).unwrap();
    summary
        .execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id text, title text, preview text, step_count integer, \
             last_modified_time datetime, workspace_uris text, status text, source text, project_id text, agent_name text, \
             parent_conversation_id text, nesting_depth integer);
             CREATE TABLE conversations_fts_content (id INTEGER PRIMARY KEY, c0, c1, c2, c3, c4, c5, c6, c7, c8, c9, c10, c11, c12, c13, c14);
             INSERT INTO conversation_summaries VALUES ('aa-11', '整理发布清单', '预览', 12, '2026-11-11 04:46:38.13321', '', '', '', '', '', '', 0);
             INSERT INTO conversation_summaries VALUES ('bb-22', '', '打开网页看看', 3, '2026-11-11 04:47:08', '', '', '', '', 'browser', 'aa-11', 1);
             INSERT INTO conversations_fts_content (c0, c1, c13, c14) VALUES ('aa-11', -1, '', '');
             INSERT INTO conversations_fts_content (c0, c1, c13, c14) VALUES ('aa-11', 1, '再改一下', '已改好');
             INSERT INTO conversations_fts_content (c0, c1, c13, c14) VALUES ('aa-11', 0, '先列清单', '好的，三项');",
        )
        .unwrap();
    let steps = Connection::open(root.join("conversations").join("aa-11.db")).unwrap();
    steps
        .execute_batch(
            "CREATE TABLE steps (idx integer, step_type integer, metadata blob);
             CREATE TABLE gen_metadata (idx integer, data blob, size integer);",
        )
        .unwrap();
    steps
        .execute("INSERT INTO steps VALUES (0, 14, ?1)", [msg(1, &num(1, 1_794_761_000))])
        .unwrap();
    steps
        .execute("INSERT INTO steps VALUES (1, 15, ?1)", [step_metadata(1_794_761_022, 100, 20, 30)])
        .unwrap();
    steps
        .execute("INSERT INTO gen_metadata VALUES (0, ?1, 1)", [gen_metadata(1_000, 200_000)])
        .unwrap();
    steps
        .execute("INSERT INTO gen_metadata VALUES (1, ?1, 1)", [gen_metadata(51_200, 256_000)])
        .unwrap();
    fs::write(root.join("antigravity-oauth-token"), "x").unwrap();
}

#[test]
fn reads_only_granted_databases() {
    let root = temp("grant");
    build(&root);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    assert!(list_conversations(&guard, Capability::Conversation, &root).is_err());
    for pattern in [SUMMARY_PATTERN, CONVERSATIONS_PATTERN] {
        guard
            .grant(
                ToolId::Antigravity,
                Capability::Conversation,
                PathScope { root: root.clone(), pattern: pattern.to_string() },
            )
            .unwrap();
    }
    let list = list_conversations(&guard, Capability::Conversation, &root).unwrap();
    assert_eq!(list.len(), 2);
    let main = list.iter().find(|item| item.id == "aa-11").unwrap();
    assert_eq!(main.title, "整理发布清单");
    assert_eq!(main.steps, 12);
    assert!(!main.nested && main.updated_ms > 0);
    let nested = list.iter().find(|item| item.id == "bb-22").unwrap();
    assert!(nested.nested);
    assert_eq!(nested.title, "打开网页看看");
    let chunks = transcript(&guard, &root, "aa-11").unwrap().unwrap();
    assert_eq!(chunks, vec!["用户：先列清单", "助手：好的，三项", "用户：再改一下", "助手：已改好"]);
    assert_eq!(transcript(&guard, &root, "cc-33").unwrap(), None);
    let paths = conversation_paths(&root);
    assert_eq!(paths.len(), 1);
    assert_eq!(context(&guard, &paths[0]).unwrap(), Some((51_200, 256_000)));
    // 用量是另一项授权
    assert!(calls(&guard, Capability::Usage, &paths[0]).is_err());
    guard
        .grant(
            ToolId::Antigravity,
            Capability::Usage,
            PathScope { root: root.clone(), pattern: CONVERSATIONS_PATTERN.to_string() },
        )
        .unwrap();
    let found = calls(&guard, Capability::Usage, &paths[0]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].tokens, 150);
    assert!(guard
        .open_read(ToolId::Antigravity, Capability::Usage, &root.join("antigravity-oauth-token"))
        .is_err());
    let _ = fs::remove_dir_all(&root);
}
