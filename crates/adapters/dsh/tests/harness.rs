use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wanxiang_adapter_dsh::{
    decode_zstd, is_bot_workspace, parse_session, read_session_bytes, session_chunks, session_paths,
    BOT_SESSION_PATTERN, HARNESS_PATTERN,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

/// 手写的 zstd 帧：只用「原样块」，不依赖压缩器，便于夹具。
fn raw_frame(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x38];
    let blocks: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.chunks(100 * 1024).collect()
    };
    for (index, block) in blocks.iter().enumerate() {
        let last = u32::from(index + 1 == blocks.len());
        let header = last | ((block.len() as u32) << 3);
        out.extend_from_slice(&header.to_le_bytes()[..3]);
        out.extend_from_slice(block);
    }
    out
}

fn sample(id: &str, cwd: &str) -> String {
    [
        format!(r#"{{"type":"session","id":"{id}","createdAt":1000,"cwd":"{cwd}","version":4}}"#),
        r#"{"type":"session/title","seq":1,"time":1100,"data":{"title":"整理发布清单"}}"#.to_string(),
        r#"{"type":"request/context","seq":2,"time":1200,"data":{"contextWindow":1000,"model":"m"}}"#.to_string(),
        r#"{"type":"user/message","seq":3,"time":1300,"data":{"content":[{"type":"text","text":"先列出待办"}]}}"#.to_string(),
        r#"{"type":"assistant/message","seq":4,"time":1400,"data":{"message":{"content":[{"type":"reasoning","text":"内部推理"},{"type":"text","text":"好的，三项待办"}]},"usage":{"inputTokens":100,"cacheReadTokens":20,"cacheWriteTokens":0,"outputTokens":30,"totalTokens":150}}}"#.to_string(),
        r#"{"type":"tool/result","seq":5,"time":1450,"data":{"message":{"content":[{"type":"text","text":"工具输出"}]}}}"#.to_string(),
        r#"{"type":"assistant/message","seq":6,"time":86401500,"data":{"message":{"content":[{"type":"text","text":"完成"}]},"usage":{"inputTokens":400,"cacheReadTokens":0,"cacheWriteTokens":0,"outputTokens":50,"totalTokens":450}}}"#.to_string(),
        "{\"type\":\"assistant/message\",\"broken".to_string(),
    ]
    .join("\n")
}

#[test]
fn parses_usage_title_and_context_without_text() {
    let body = sample("s1", "/Users/me/project");
    let decoded = decode_zstd(&raw_frame(body.as_bytes()));
    assert_eq!(decoded, body.as_bytes());
    let session = parse_session(&decoded).unwrap();
    assert_eq!(session.id, "s1");
    assert_eq!(session.title, "整理发布清单");
    assert_eq!(session.calls.len(), 2);
    assert_eq!(session.calls[0].tokens, 150);
    assert_eq!(session.calls[1].time_ms, 86401500);
    assert_eq!(session.last_tokens, Some(450));
    assert_eq!(session.context_percent(), Some(45));
    assert_eq!(session.updated_ms, 86401500);
    assert!(!session.is_bot());
}

#[test]
fn appended_frames_are_read_in_order_and_a_cut_frame_keeps_what_was_read() {
    let body = sample("s2", "/Users/me/.dsh/bot/workspace");
    let (left, right) = body.split_at(body.find("\n{\"type\":\"user").unwrap());
    let mut bytes = raw_frame(left.as_bytes());
    bytes.extend(raw_frame(right.as_bytes()));
    assert_eq!(decode_zstd(&bytes), body.as_bytes());
    let session = parse_session(&decode_zstd(&bytes)).unwrap();
    assert!(session.is_bot());
    let cut = &bytes[..bytes.len() - 10];
    let partial = decode_zstd(cut);
    assert!(partial.starts_with(left.as_bytes()));
}

#[test]
fn transcript_keeps_user_and_assistant_text_only() {
    let body = sample("s3", "/tmp/x");
    let chunks = session_chunks(body.as_bytes());
    assert_eq!(chunks, vec!["用户：先列出待办", "助手：好的，三项待办", "助手：完成"]);
}

#[test]
fn bot_workspace_detection() {
    assert!(is_bot_workspace("/Users/a/.dsh/bot/workspace"));
    assert!(is_bot_workspace("/Users/a/.dsh/bot/workspace/sub"));
    assert!(!is_bot_workspace("/Users/a/Projects/bot/workspace"));
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-harness-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_session(root: &Path, workspace: &str, id: &str, cwd: &str) -> PathBuf {
    let dir = root.join("sessions").join(workspace).join(format!("session-{id}"));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("session.v4.jsonl.zstd");
    fs::write(&file, raw_frame(sample(id, cwd).as_bytes())).unwrap();
    fs::write(dir.join("session.lock"), b"").unwrap();
    file
}

#[test]
fn reads_only_through_the_guard_and_patterns_split_bot_from_harness() {
    let root = temp("guard");
    let harness = write_session(&root, "--Users-me-project--", "h1", "/Users/me/project");
    let bot = write_session(&root, "--Users-me-.dsh-bot-workspace--", "b1", "/Users/me/.dsh/bot/workspace");
    fs::create_dir_all(root.join("bot")).unwrap();
    fs::write(root.join("bot/secrets.json"), b"hidden").unwrap();
    let mut expected = vec![harness.clone(), bot.clone()];
    expected.sort();
    assert_eq!(session_paths(&root), expected);

    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    assert!(read_session_bytes(&guard, ToolId::DeepSeekHarness, Capability::Usage, &harness).is_err());
    guard
        .grant(
            ToolId::DeepSeekHarness,
            Capability::Usage,
            PathScope { root: root.clone(), pattern: HARNESS_PATTERN.to_string() },
        )
        .unwrap();
    let bytes = read_session_bytes(&guard, ToolId::DeepSeekHarness, Capability::Usage, &harness).unwrap();
    assert_eq!(parse_session(&bytes).unwrap().id, "h1");
    assert!(read_session_bytes(&guard, ToolId::DeepSeekHarness, Capability::Usage, &root.join("bot/secrets.json")).is_err());
    assert!(read_session_bytes(&guard, ToolId::DeepSeekHarness, Capability::Conversation, &harness).is_err());

    guard
        .grant(
            ToolId::DsBot,
            Capability::Conversation,
            PathScope { root: root.clone(), pattern: BOT_SESSION_PATTERN.to_string() },
        )
        .unwrap();
    assert!(read_session_bytes(&guard, ToolId::DsBot, Capability::Conversation, &bot).is_ok());
    assert!(read_session_bytes(&guard, ToolId::DsBot, Capability::Conversation, &harness).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_rewritten_assistant_message_is_counted_once() {
    let body = [
        r#"{"type":"session","id":"d1","createdAt":1,"cwd":"/x"}"#,
        r#"{"type":"assistant/message","time":10,"data":{"message":{"id":"m1","content":[]},"usage":{"inputTokens":10,"outputTokens":1}}}"#,
        r#"{"type":"assistant/message","time":12,"data":{"message":{"id":"m1","content":[]},"usage":{"inputTokens":10,"outputTokens":5}}}"#,
        r#"{"type":"assistant/message","time":20,"data":{"message":{"id":"m2","content":[]},"usage":{"inputTokens":20,"outputTokens":2}}}"#,
    ]
    .join("\n");
    let session = parse_session(body.as_bytes()).unwrap();
    assert_eq!(session.calls.len(), 2);
    assert_eq!(session.duplicates, 1);
    assert_eq!(session.calls[0].tokens, 15);
    assert_eq!(session.calls[1].tokens, 22);
}
