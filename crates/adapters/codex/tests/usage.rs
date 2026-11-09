use std::fs;
use std::path::PathBuf;
use std::process::Command;

use wanxiang_adapter_codex::{
    combine_rollouts, find_codex_cli_in, parse_app_server_result, parse_rollout, query_rate_limits,
    read_rollouts, rollout_paths,
};
use wanxiang_core::{window_title, Supervisor, UsageError};

fn text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(format!("rollout-{name}.jsonl"));
    fs::read_to_string(path).unwrap()
}

#[test]
fn cumulative_rollout_counts_each_increment_once() {
    let rollout = parse_rollout(&text("cumulative"));
    assert_eq!(rollout.samples, 3);
    assert_eq!(rollout.tokens(), Some(107544));
    assert!(rollout.windows.is_empty());
}

#[test]
fn duplicate_rollout_counts_only_the_last_sample() {
    let rollout = parse_rollout(&text("duplicate"));
    assert_eq!(rollout.events.len(), 1);
    assert!(rollout.incomplete);
    assert!(rollout.partial);
    let value = text("duplicate")
        .lines()
        .find_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            (value.pointer("/payload/type").and_then(|item| item.as_str()) == Some("token_count"))
                .then_some(value)
        })
        .expect("token_count");
    let last = value
        .pointer("/payload/info/last_token_usage/total_tokens")
        .and_then(|item| item.as_i64())
        .unwrap();
    assert_eq!(rollout.tokens(), Some(last));
}

#[test]
fn reset_rollout_starts_a_new_segment() {
    let rollout = parse_rollout(&text("reset"));
    assert_eq!(rollout.events.len(), 2);
    let last_sum: i64 = text("reset")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value.pointer("/payload/type").and_then(|item| item.as_str()) == Some("token_count"))
        .map(|value| {
            value
                .pointer("/payload/info/last_token_usage/total_tokens")
                .and_then(|item| item.as_i64())
                .unwrap()
        })
        .sum();
    assert_eq!(rollout.tokens(), Some(last_sum));
}

#[test]
fn unmetered_rollout_is_missing_not_zero() {
    let rollout = parse_rollout(&text("unmetered"));
    assert_eq!(rollout.samples, 0);
    assert_eq!(rollout.tokens(), None);
    assert_eq!(combine_rollouts(&[rollout]), None);
}

#[test]
fn mixed_rollouts_exclude_unmetered_sessions() {
    let rows = [parse_rollout(&text("cumulative")), parse_rollout(&text("unmetered"))];
    assert_eq!(combine_rollouts(&rows), Some((107544, 1)));
}

#[test]
fn only_explicit_compacted_events_count() {
    let rollout = parse_rollout(&text("compacted"));
    assert_eq!(rollout.compactions, Some(2));
    let dropped = parse_rollout(&format!(
        "{}\n{}",
        text("cumulative"),
        r#"{"timestamp":"2026-08-27T10:00:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":2},"last_token_usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":2}}}}"#
    ));
    assert_eq!(dropped.compactions, None);
}

#[test]
fn illegal_used_percent_drops_only_that_window() {
    let value = serde_json::json!({
        "rateLimits": {
            "primary": {"usedPercent": 101, "windowDurationMins": 300, "resetsAt": 10},
            "secondary": {"usedPercent": 34, "windowDurationMins": null}
        }
    });
    let windows = parse_app_server_result(&value);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].used_percent, 34);
    assert_eq!(windows[0].minutes, None);
    assert_eq!(windows[0].resets_at, None);
    assert_eq!(window_title(None), "周期未知");
    assert_eq!(window_title(Some(300)), "5 小时窗口");
    assert_eq!(window_title(Some(10080)), "7 天窗口");
}

#[test]
fn app_server_snapshot_drops_account_fields() {
    let value = serde_json::json!({
        "result": {
            "accountId": "acct-should-not-appear",
            "rateLimitResetCredits": {"available": 1},
            "rateLimits": {
                "limitName": "drop-me",
                "planType": "drop-plan",
                "credits": {"balance": 1},
                "individualLimit": {"usedPercent": 1},
                "spendControlReached": false,
                "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 20}
            }
        }
    });
    let windows = parse_app_server_result(&value);
    let debug = format!("{windows:?}");
    assert!(!debug.contains("acct-should-not-appear"));
    assert!(!debug.contains("drop-plan"));
    assert!(!debug.contains("drop-me"));
    assert_eq!(windows[0].used_percent, 12);
    assert_eq!(windows[0].resets_at, Some(20));
}

#[cfg(unix)]
#[test]
fn app_server_reads_rate_limits_without_extra_params() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("wanxiang-fake-codex-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("fake-codex");
    let log = dir.join("log");
    fs::write(
        &script,
        "#!/bin/sh\nwhile IFS= read -r line; do\nprintf '%s\\n' \"$line\" >> \"$WANXIANG_FAKE_LOG\"\ncase \"$line\" in\n*'\"method\":\"initialize\"'*) printf '%s\\n' '{\"id\":1,\"result\":{}}' ;;\n*'account/rateLimits/read'*) printf '%s\\n' \"$WANXIANG_FAKE_RESULT\" ;;\nesac\ndone\n",
    )
    .unwrap();
    let mut mode = fs::metadata(&script).unwrap().permissions();
    mode.set_mode(0o755);
    fs::set_permissions(&script, mode).unwrap();
    let result = r#"{"id":2,"result":{"accountId":"acct-should-not-appear","rateLimits":{"planType":"drop-plan","primary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":20}}}}"#;
    let mut command = Command::new(&script);
    command
        .env("WANXIANG_FAKE_LOG", &log)
        .env("WANXIANG_FAKE_RESULT", result);
    let supervisor = Supervisor::new();
    let windows = query_rate_limits(&supervisor.handle(), command, |_| {}).unwrap();
    assert_eq!(windows[0].used_percent, 12);
    let seen = fs::read_to_string(&log).unwrap();
    assert!(!seen.contains("excludeResetCreditDetails"));
    assert!(!seen.contains("supportsLunaReserve"));
    assert!(!seen.contains("ordinaryUsageAllowed"));
    assert!(!seen.contains("jsonrpc"));
    assert!(seen.find("initialize").unwrap() < seen.find("account/rateLimits/read").unwrap());
    assert!(!format!("{windows:?}").contains("acct-should-not-appear"));
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn codex_cli_skips_the_desktop_binary_and_finds_the_path_name() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("wanxiang-codex-bin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let desktop = root.join("Codex.app/Contents/MacOS/Codex");
    let cli = root.join("bin/codex");
    for path in [&desktop, &cli] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"#!/bin/sh\n").unwrap();
        let mut mode = fs::metadata(path).unwrap().permissions();
        mode.set_mode(0o755);
        fs::set_permissions(path, mode).unwrap();
    }
    let desktop_dir = desktop.parent().unwrap().to_str().unwrap();
    assert!(find_codex_cli_in(desktop_dir, &[]).is_none());
    let path_env = format!("{desktop_dir}:{}", cli.parent().unwrap().display());
    let found = find_codex_cli_in(&path_env, &[]).unwrap();
    assert_eq!(found, cli);
    assert_eq!(found.file_name().unwrap(), "codex");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn readers_do_not_open_credential_files() {
    let root = std::env::temp_dir().join(format!("wanxiang-codex-read-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sessions/2026/08/27")).unwrap();
    let auth = format!("{}{}", "auth", ".json");
    let creds = format!(".{}{}", "cred", "entials.yaml");
    let database = format!("{}{}", "state", ".vscdb");
    fs::write(root.join(&auth), b"hidden").unwrap();
    fs::write(root.join("sessions").join(&auth), b"hidden").unwrap();
    fs::write(root.join(creds), b"hidden").unwrap();
    fs::write(root.join(database), b"hidden").unwrap();
    fs::write(root.join("sessions/2026/08/27/demo.jsonl"), "{}\n").unwrap();
    let paths = rollout_paths(&root);
    assert!(paths.iter().all(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl")));
    let mut opened = Vec::new();
    let _ = read_rollouts(&root, &mut opened);
    assert_eq!(opened.len(), 1);
    assert!(opened.iter().all(|path| !path.file_name().unwrap().to_string_lossy().contains(&auth)));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn fixtures_contain_no_email_or_secret_prefix() {
    let prefix = format!("{}{}", "s", "k-");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    for entry in fs::read_dir(&root).unwrap() {
        let text = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains('@'), "夹具里不该有邮箱");
        assert!(!text.contains(&prefix));
    }
}

#[test]
fn missing_program_is_app_server_unavailable() {
    let supervisor = Supervisor::new();
    let err = query_rate_limits(
        &supervisor.handle(),
        Command::new(std::env::temp_dir().join("wanxiang-missing-codex")),
        |_| {},
    )
    .unwrap_err();
    assert_eq!(err, UsageError::AppServer);
    assert!(!format!("{err:?}").contains(&format!("{}{}", "auth", ".json")));
}
