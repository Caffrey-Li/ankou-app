use std::fs;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;

use wanxiang_adapter_codex::{
    context_percent, list_jsonl, parse_jsonl_head, parse_thread_list, read_jsonl, read_thread, thread_list_line,
    thread_read_line, title_of,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, Supervisor, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn guard() -> Guard {
    Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>)
}

#[test]
fn thread_list_line_always_sets_use_state_db_only() {
    for id in [2, 7, 11] {
        let line = thread_list_line(id);
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["params"]["useStateDbOnly"], true);
        assert_eq!(value["method"], "thread/list");
    }
}

#[test]
fn thread_read_omits_include_turns() {
    let line = thread_read_line(3, "thread-1");
    assert!(!line.contains("includeTurns"));
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(value["params"].get("includeTurns").is_none());
}

#[test]
fn server_list_drops_cwd_and_keeps_documented_fields() {
    let value = serde_json::json!({
        "result": {"data": [{
            "id": "thread-1",
            "preview": "摘",
            "createdAt": "2026-10-24T00:00:00Z",
            "updatedAt": "2026-10-24T01:00:00Z",
            "name": null,
            "status": "idle",
            "cwd": "/hidden"
        }]}
    });
    let head = parse_thread_list(&value).pop().unwrap();
    assert_eq!(head.id, "thread-1");
    assert_eq!(head.preview, "摘");
    assert_eq!(head.status, "idle");
    assert!(head.cwd.is_none());
    assert_eq!(title_of(&head), "Codex thread-1");
    assert!(head.used_tokens.is_none());
    assert!(context_percent(head.used_tokens, head.window_tokens).is_none());
}

#[test]
fn jsonl_context_uses_last_usage_over_the_window() {
    let text = concat!(
        r#"{"timestamp":"2026-10-24T11:44:10Z","type":"session_meta","payload":{"id":"00000000-0000-0000-0000-00000000000a","cwd":"/Users/example/projects/demo","timestamp":"2026-10-24T11:44:10Z"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-24T11:45:18Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":99999},"last_token_usage":{"total_tokens":80},"model_context_window":100}}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"type":"message","content":[{"text":"你好"}]}}"#,
        "\n"
    );
    let head = parse_jsonl_head(text).unwrap();
    assert_eq!(head.cwd.as_deref(), Some("/Users/example/projects/demo"));
    assert_eq!(context_percent(head.used_tokens, head.window_tokens), Some(80));
    assert_eq!(title_of(&head), "Codex 00000000");
    let (body, truncated) = wanxiang_adapter_codex::parse_jsonl_text(text);
    assert_eq!(body, "你好");
    assert!(!truncated);
}

#[test]
fn usage_grant_does_not_read_the_conversation() {
    let root = std::env::temp_dir().join(format!("wanxiang-codex-conv-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sessions/2026/08/27")).unwrap();
    fs::write(root.join("sessions/2026/08/27/demo.jsonl"), "{}\n").unwrap();
    let guard = guard();
    guard
        .grant(
            ToolId::Codex,
            Capability::Usage,
            PathScope {
                root: root.clone(),
                pattern: "sessions/**/*.jsonl".to_string(),
            },
        )
        .unwrap();
    assert!(list_jsonl(&guard, &root).is_err());
    let file = root.join("sessions/2026/08/27/demo.jsonl");
    assert!(read_jsonl(&guard, &file).is_err());
    guard
        .grant(
            ToolId::Codex,
            Capability::Conversation,
            PathScope {
                root: root.clone(),
                pattern: "sessions/**/*.jsonl".to_string(),
            },
        )
        .unwrap();
    assert!(list_jsonl(&guard, &root).is_ok());
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn fake_app_server_requires_use_state_db_only_on_every_list() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("wanxiang-fake-threads-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("fake-codex");
    let log = dir.join("log");
    let mut file = fs::File::create(&script).unwrap();
    write!(
        file,
        r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$WANXIANG_FAKE_LOG"
  case "$line" in
    *'"id":1'*) printf '%s\n' '{{"id":1,"result":{{}}}}' ;;
    *'"id":2'*) printf '%s\n' '{{"id":2,"result":{{"data":[{{"id":"thread-1","preview":"摘","createdAt":"2026-10-24T00:00:00Z","updatedAt":"2026-10-24T01:00:00Z","name":"标题","status":"idle","cwd":"/hidden"}}],"nextCursor":null}}}}' ;;
    *'"id":3'*) printf '%s\n' '{{"id":3,"result":{{"thread":{{"id":"thread-1","preview":"摘","createdAt":"2026-10-24T00:00:00Z","updatedAt":"2026-10-24T01:00:00Z","name":"标题","status":"idle"}}}}}}' ;;
    *'"id":4'*) printf '%s\n' '{{"id":4,"result":{{"data":[{{"id":"turn-1"}}],"nextCursor":null}}}}' ;;
    *'"id":5'*) printf '%s\n' '{{"id":5,"result":{{"data":[{{"turnId":"turn-1","item":{{"text":"你好"}}}}],"nextCursor":null}}}}' ;;
  esac
done
"#
    )
    .unwrap();
    drop(file);
    let mut mode = fs::metadata(&script).unwrap().permissions();
    mode.set_mode(0o755);
    fs::set_permissions(&script, mode).unwrap();
    let mut command = Command::new(&script);
    command.env("WANXIANG_FAKE_LOG", &log);
    let supervisor = Supervisor::new();
    let thread = read_thread(&supervisor.handle(), command, "thread-1").unwrap();
    assert_eq!(thread.head.cwd, None);
    assert_eq!(thread.head.name.as_deref(), Some("标题"));
    assert_eq!(thread.text, "你好");
    assert!(!thread.truncated);
    let seen = fs::read_to_string(&log).unwrap();
    let lists: Vec<_> = seen.lines().filter(|line| line.contains("thread/list")).collect();
    assert!(!lists.is_empty());
    assert!(lists.iter().all(|line| line.contains("\"useStateDbOnly\":true")));
    assert!(!seen.contains("includeTurns"));
    let _ = fs::remove_dir_all(&dir);
}
