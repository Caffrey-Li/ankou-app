use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use wanxiang_adapter_grokbot::{
    agents, classify, decode_key, parse_transcript, slices, transcript, weekly_usage, Slice,
    PERSIST_PATTERN,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-grok-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

/// 小写 base32、不补等号（与应用的文件名一致）。
fn encode(key: &str) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::new();
    let (mut bits, mut count) = (0u32, 0u32);
    for byte in key.bytes() {
        bits = (bits << 8) | u32::from(byte);
        count += 8;
        while count >= 5 {
            count -= 5;
            out.push(ALPHABET[((bits >> count) & 31) as usize] as char);
        }
        bits &= (1 << count) - 1;
    }
    if count > 0 {
        out.push(ALPHABET[((bits << (5 - count)) & 31) as usize] as char);
    }
    out
}

const ACCOUNT: &str = "sand.client.slice.account.acct%7Cuser_X";
const AGENT: &str = "0a1b2c3d-1111-2222-3333-444455556666";

fn write(root: &PathBuf, key: &str, body: &str) {
    let dir = root.join("sand-client-persistence");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{}.blob", encode(key))), body).unwrap();
}

#[test]
fn keys_decode_and_only_three_kinds_are_known() {
    let key = format!("{ACCOUNT}.transcript.replicas.{AGENT}");
    assert_eq!(decode_key(&encode(&key)).as_deref(), Some(key.as_str()));
    assert_eq!(classify(&key), Some(Slice::Transcript(AGENT.to_string())));
    assert_eq!(classify(&format!("{ACCOUNT}.roster.last-roster")), Some(Slice::Roster));
    assert_eq!(classify(&format!("{ACCOUNT}.weekly-usage.cache")), Some(Slice::Usage));
    assert_eq!(classify(&format!("{ACCOUNT}.composer-drafts.{AGENT}")), None);
    assert_eq!(classify(&format!("{ACCOUNT}.transcript.replicas.../x")), None);
    assert_eq!(classify("other.roster.last-roster"), None);
    assert!(decode_key("not base32!").is_none());
}

#[test]
fn transcript_keeps_spoken_text_in_order() {
    let value: serde_json::Value = serde_json::from_str(
        r#"{"entries":[
            {"kind":"send-message","id":"3","message":{"type":"text","content":"好的，三项"},"timestampMs":30,"seq":3},
            {"kind":"message","id":"1","role":"user","content":"整理发布清单","timestampMs":10,"seq":1},
            {"kind":"send-message","id":"4","message":{"type":"widget","widget":{}},"timestampMs":40,"seq":4},
            {"kind":"user-attachment","id":"2","file_name":"a.png","timestampMs":20,"seq":2},
            {"kind":"message","id":"5","role":"assistant","content":" 已完成 ","timestampMs":50,"seq":5}
        ]}"#,
    )
    .unwrap();
    assert_eq!(parse_transcript(&value), vec!["用户：整理发布清单", "助手：好的，三项", "助手：已完成"]);
}

#[test]
fn reads_roster_transcript_and_usage_with_the_grant() {
    let root = temp("grant");
    write(
        &root,
        &format!("{ACCOUNT}.roster.last-roster"),
        &format!(r#"{{"schemaVersion":1,"value":{{"rows":[{{"id":"{AGENT}","name":"周报 Bot","title":"","lastActivityAt":1794372000000,"updatedAt":1794300000000}},{{"id":"../x","name":"坏"}}]}}}}"#),
    );
    write(
        &root,
        &format!("{ACCOUNT}.transcript.replicas.{AGENT}"),
        r#"{"schemaVersion":1,"value":{"entries":[{"kind":"message","id":"1","role":"user","content":"你好","timestampMs":1,"seq":1}]}}"#,
    );
    write(
        &root,
        &format!("{ACCOUNT}.weekly-usage.cache"),
        r#"{"schemaVersion":1,"value":{"kind":"present","reading":{"usage":{"percentUsed":8.46,"nextResetMs":1794600000000},"readAtMs":1794370000000},"expiresAtMs":1}}"#,
    );
    write(&root, &format!("{ACCOUNT}.composer-drafts.{AGENT}"), r#"{"value":{"draft":{"prompt":"草稿"}}}"#);
    assert_eq!(slices(&root).len(), 3);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    assert!(agents(&guard, &root).is_err());
    guard
        .grant(
            ToolId::GrokBot,
            Capability::Conversation,
            PathScope { root: root.clone(), pattern: PERSIST_PATTERN.to_string() },
        )
        .unwrap();
    let list = agents(&guard, &root).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "周报 Bot");
    assert_eq!(list[0].updated_ms, 1_794_372_000_000);
    assert_eq!(transcript(&guard, &root, AGENT).unwrap().unwrap(), vec!["用户：你好"]);
    assert_eq!(transcript(&guard, &root, "ffff-0000").unwrap(), None);
    // 额度是另一项授权
    assert!(weekly_usage(&guard, &root).is_err());
    guard
        .grant(
            ToolId::GrokBot,
            Capability::Usage,
            PathScope { root: root.clone(), pattern: PERSIST_PATTERN.to_string() },
        )
        .unwrap();
    let usage = weekly_usage(&guard, &root).unwrap().unwrap();
    assert_eq!(usage.percent, 8);
    assert_eq!(usage.resets_ms, Some(1_794_600_000_000));
    assert_eq!(usage.read_ms, Some(1_794_370_000_000));
    let _ = fs::remove_dir_all(&root);
}
