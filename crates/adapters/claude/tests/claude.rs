use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use wanxiang_adapter_claude::{
    deeplink, desktop_session, find_session, latest_plan, parse_session, read_file, session_paths,
    short_title, transcript, valid_id, PROJECTS_PATTERN,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-claude-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

const SESSION: &str = concat!(
    r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<command-name>/clear</command-name>"},"timestamp":"2026-11-10T01:00:00Z"}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":"整理发布清单\n按优先级"},"timestamp":"2026-11-10T01:00:01Z"}"#,
    "\n",
    r#"{"type":"assistant","message":{"id":"msg_1","model":"claude-opus-5-5","content":[{"type":"thinking","thinking":"想"}],"usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":5}},"timestamp":"2026-11-10T01:00:02Z"}"#,
    "\n",
    r#"{"type":"assistant","message":{"id":"msg_1","model":"claude-opus-5-5","content":[{"type":"text","text":"好的，三项"}],"usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":20}},"timestamp":"2026-11-10T01:00:03Z"}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"输出"}]},"timestamp":"2026-11-10T01:00:04Z"}"#,
    "\n",
    r#"{"type":"assistant","isSidechain":true,"message":{"id":"msg_2","model":"claude-opus-5-5","content":[{"type":"text","text":"子任务"}],"usage":{"input_tokens":1,"output_tokens":1}},"timestamp":"2026-11-10T01:00:05Z"}"#,
    "\n",
    r#"{"type":"assistant","message":{"id":"msg_3","model":"<synthetic>","content":[{"type":"text","text":"x"}],"usage":{"input_tokens":0,"output_tokens":0}},"timestamp":"2026-11-10T01:00:06Z"}"#,
    "\n",
    r#"{"type":"custom-title","customTitle":"发布清单","sessionId":"s"}"#,
    "\n",
    "坏行\n"
);

#[test]
fn session_counts_each_reply_once_and_keeps_only_numbers() {
    let session = parse_session("aa-11", SESSION.as_bytes());
    assert_eq!(session.calls.len(), 2);
    assert_eq!(session.calls[0].tokens, 1130);
    assert_eq!(session.calls[1].tokens, 2);
    assert_eq!(session.last_context, Some(1130));
    assert_eq!(session.title(), "发布清单");
    assert_eq!(session.first_user.as_deref(), Some("整理发布清单\n按优先级"));
    assert!(session.created_ms > 0 && session.updated_ms > session.created_ms);
    let untitled = parse_session("b", SESSION.replace("custom-title", "other").as_bytes());
    assert_eq!(untitled.title(), "整理发布清单");
}

#[test]
fn transcript_has_only_spoken_text() {
    let chunks = transcript(SESSION.as_bytes());
    assert_eq!(chunks, vec!["用户：整理发布清单\n按优先级", "助手：好的，三项"]);
}

#[test]
fn plan_takes_the_newest_sample_with_data() {
    let raw = r#"{"version":1,"samples":[{"t":1,"org":"a","u":{"fh":10,"sd":20}},{"t":3,"org":"b","u":{}},{"t":2,"org":"a","u":{"fh":150,"sd":40}}]}"#;
    let plan = latest_plan(raw.as_bytes()).unwrap();
    assert_eq!(plan.at_ms, 2);
    assert_eq!(plan.five_hour, Some(100));
    assert_eq!(plan.seven_day, Some(40));
    assert!(latest_plan(br#"{"samples":[]}"#).is_none());
}

#[test]
fn desktop_session_keeps_title_and_id_only() {
    let raw = r#"{"sessionId":"x","cliSessionId":"ab-12","title":" 改设置页 ","isArchived":false,"cwd":"/hidden"}"#;
    let session = desktop_session(raw.as_bytes()).unwrap();
    assert_eq!(session.cli_session_id, "ab-12");
    assert_eq!(session.title, "改设置页");
    assert!(!session.archived);
}

#[test]
fn files_are_read_only_with_the_grant() {
    let root = temp("grant");
    let project = root.join("projects").join("-Users-x-proj");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("ab12.jsonl"), SESSION).unwrap();
    fs::write(root.join("settings.json"), "{}").unwrap();
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    let paths = session_paths(&root);
    assert_eq!(paths.len(), 1);
    assert!(read_file(&guard, Capability::Usage, &paths[0]).is_err());
    guard
        .grant(
            ToolId::Claude,
            Capability::Usage,
            PathScope { root: root.clone(), pattern: PROJECTS_PATTERN.to_string() },
        )
        .unwrap();
    assert!(read_file(&guard, Capability::Usage, &paths[0]).is_ok());
    assert!(read_file(&guard, Capability::Conversation, &paths[0]).is_err());
    assert!(read_file(&guard, Capability::Usage, &root.join("settings.json")).is_err());
    assert_eq!(find_session(&root, "ab12"), Some(paths[0].clone()));
    assert_eq!(find_session(&root, "../x"), None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ids_titles_and_deeplinks() {
    assert!(valid_id("0a1b-2c"));
    assert!(!valid_id("a/b"));
    assert_eq!(short_title(&"长".repeat(50)).chars().count(), 41);
    assert_eq!(deeplink("a b\n交").unwrap(), "claude-cli://open?q=a%20b%0A%E4%BA%A4");
    assert!(deeplink(&"x".repeat(5001)).is_none());
}

#[test]
fn transcript_unwraps_desktop_message_markup() {
    let wrapped = concat!(
        r#"{"type":"user","message":{"role":"user","content":"@/tmp/a.png <wake reason=\"mention\">\n<project id=\"p\">\n<thread ts=\"t\">\n<message trigger=\"true\" from=\"human\" id=\"m\">帮我看下截图\n第二行\n  <attachment kind=\"image\" name=\"a.png\"></attachment></message>\n</thread></project></wake>"}}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user","content":"<wake><message from=\"system\">定时提醒</message></wake>"}}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user","content":"[auto-continue] The usage limit reset."}}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user","content":"@/tmp/b.txt 改一下这个文件"}}"#,
        "\n"
    );
    assert_eq!(
        transcript(wrapped.as_bytes()),
        vec!["用户：帮我看下截图\n第二行\n[附件]", "用户：改一下这个文件"]
    );
    let session = parse_session("w", wrapped.as_bytes());
    assert_eq!(session.first_user.as_deref(), Some("帮我看下截图\n第二行\n[附件]"));
}

#[test]
fn transcript_skips_multiline_injected_markup() {
    let text = concat!(
        r#"{"type":"user","message":{"role":"user","content":"<system-reminder>\n提醒内容\n</system-reminder>"}}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user","content":"正常提问"}}"#,
        "\n"
    );
    assert_eq!(transcript(text.as_bytes()), vec!["用户：正常提问"]);
}
