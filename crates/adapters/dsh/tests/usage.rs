use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use wanxiang_adapter_dsh::{
    fetch_balance, owner_kind, parse_balance_json, read_usage_copy, summarize_calls,
    usage_db_candidates, CallRow, OwnerKind,
};
use wanxiang_core::UsageError;

fn rows_at(now_ms: i64) -> Vec<CallRow> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/usage.json")).unwrap();
    value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| CallRow {
            time_ms: now_ms + row["offsetHours"].as_i64().unwrap() * 3_600_000,
            session: row["session"].as_str().unwrap().to_string(),
            model: row["model"].as_str().unwrap().to_string(),
            owner: row["owner"].as_str().unwrap().to_string(),
            name: row["name"].as_str().unwrap().to_string(),
            input: row["input"].as_i64().unwrap(),
            cache_read: row["cache_read"].as_i64().unwrap(),
            cache_write: row["cache_write"].as_i64().unwrap(),
            output: row["output"].as_i64().unwrap(),
        })
        .collect()
}

fn noon() -> i64 {
    chrono_lite(2026, 10, 25, 12)
}

fn chrono_lite(year: i32, month: u32, day: u32, hour: u32) -> i64 {
    let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
    let time = date.and_hms_opt(hour, 0, 0).unwrap().and_utc();
    time.timestamp_millis()
}

fn since(now_ms: i64, days: i64) -> i64 {
    let day = 86_400_000;
    let start = now_ms - (now_ms.rem_euclid(day));
    start - (days - 1) * day
}

#[test]
fn dsh_fixture_matches_the_old_daily_totals() {
    let now = noon();
    let rows = rows_at(now);
    let today = summarize_calls(&rows, since(now, 1));
    assert_eq!(today.tokens, 300);
    assert_eq!(today.calls, 2);
    assert_eq!(today.groups.len(), 1);
    assert_eq!(today.groups[0].name, "示例 Bot");
    assert_eq!(today.groups[0].kind, OwnerKind::Bot);
    let week = summarize_calls(&rows, since(now, 7));
    assert_eq!(week.tokens, 315);
    assert_eq!(week.calls, 3);
    let month = summarize_calls(&rows, since(now, 30));
    assert_eq!(month.tokens, 315);
    assert_eq!(month.groups.iter().map(|group| group.tokens).sum::<i64>(), 315);
}

#[test]
fn calls_are_not_added_to_harness() {
    let rows = rows_at(noon());
    let summary = summarize_calls(&rows, 0);
    assert_eq!(summary.tokens, 315);
    assert_eq!(summary.calls, 3);
    assert_eq!(summary.excluded, 1);
    assert!(summary.groups.iter().all(|group| group.kind.counted()));
    assert_eq!(owner_kind("agent:old"), OwnerKind::Agent);
    assert!(!OwnerKind::Agent.counted());
    assert_eq!(
        summary.groups.iter().map(|group| group.tokens).sum::<i64>(),
        summary.tokens
    );
}

#[test]
fn balance_fixture_is_cny_minor_units_not_tokens() {
    let balances = parse_balance_json(include_str!("../fixtures/balance.json")).unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].total_minor, 1234);
    assert_eq!(balances[0].granted_minor, 234);
    assert_eq!(balances[0].topped_up_minor, 1000);
    let tokens = summarize_calls(&rows_at(noon()), 0).tokens;
    assert_ne!(tokens, balances[0].total_minor);
    assert!(parse_balance_json(r#"{"is_available":true,"balance_infos":[]}"#).is_err());
    let zero = parse_balance_json(
        r#"{"is_available":false,"balance_infos":[{"currency":"CNY","total_balance":"0.00","granted_balance":"0.00","topped_up_balance":"0.00"}]}"#,
    )
    .unwrap();
    assert_eq!(zero[0].total_minor, 0);
}

#[test]
fn balance_request_reports_invalid_key_without_echoing_it() {
    let (port, request) = serve("401 Unauthorized", "nope");
    let err = fetch_balance(&format!("http://127.0.0.1:{port}"), "test-key").unwrap_err();
    assert_eq!(err, UsageError::InvalidKey);
    let text = format!("{err} {err:?}");
    assert!(!text.contains("test-key"));
    let seen = request.join().unwrap();
    assert!(seen.contains("GET /user/balance"));
    assert!(seen.contains("Authorization: Bearer test-key"));
}

#[test]
fn balance_request_reports_network_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let err = fetch_balance(&format!("http://127.0.0.1:{port}"), "test-key").unwrap_err();
    assert_eq!(err, UsageError::Network);
    assert!(!format!("{err} {err:?}").contains("test-key"));
}

#[test]
fn copied_database_round_trip_and_candidates_skip_credentials() {
    let root = std::env::temp_dir().join(format!("wanxiang-dsh-fix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("bot")).unwrap();
    std::fs::write(root.join(format!(".{}{}", "cred", "entials.yaml")), b"hidden").unwrap();
    std::fs::write(root.join(format!("{}{}", "auth", ".json")), b"hidden").unwrap();
    let db = root.join("bot/usage.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE calls (key TEXT, session TEXT, time INTEGER, provider TEXT, model TEXT, kind TEXT, live INTEGER, input INTEGER, cache_read INTEGER, cache_write INTEGER, output INTEGER);
         CREATE TABLE owners (session TEXT, owner TEXT, name TEXT);
         INSERT INTO calls VALUES ('k', 's', 10, 'p', 'm', 'chat', 1, 1, 2, 3, 4);
         INSERT INTO owners VALUES ('s', 'bot:demo', '示例 Bot');",
    )
    .unwrap();
    drop(conn);
    let names: Vec<String> = usage_db_candidates(&root)
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["usage.sqlite".to_string()]);
    let rows = read_usage_copy(&db).unwrap();
    assert_eq!(summarize_calls(&rows, 0).tokens, 10);
    let _ = std::fs::remove_dir_all(&root);
}

fn serve(status: &str, body: &str) -> (u16, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let status = status.to_string();
    let body = body.to_string();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 2048];
        let mut request = String::new();
        loop {
            let read = socket.read(&mut buffer).unwrap_or(0);
            if read == 0 {
                break;
            }
            request.push_str(&String::from_utf8_lossy(&buffer[..read]));
            if request.contains("\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes());
        request
    });
    (port, handle)
}
