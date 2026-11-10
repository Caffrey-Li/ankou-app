use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use wanxiang_adapter_codex::{combine_rollouts, parse_rollout};
use wanxiang_adapter_dsh::{daily_counts, parse_balance_json, CallRow};
use wanxiang_core::{
    clock_at_utc, local_calendar_day, present_usage, unix_secs, RecordAccess, UsageFacts, UsagePage,
};

fn sample_page() -> UsagePage {
    let anchor = unix_secs(clock_at_utc(2026, 10, 25, 12, 0, 0)) * 1000;
    let now = unix_secs(clock_at_utc(2026, 10, 24, 11, 45, 18));
    let today = "2026-10-25".to_string();
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "../../crates/adapters/dsh/fixtures/usage.json"
    ))
    .unwrap();
    let calls = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| CallRow {
            time_ms: anchor + row["offsetHours"].as_i64().unwrap() * 3_600_000,
            session: row["session"].as_str().unwrap().to_string(),
            model: row["model"].as_str().unwrap().to_string(),
            owner: row["owner"].as_str().unwrap().to_string(),
            name: row["name"].as_str().unwrap().to_string(),
            input: row["input"].as_i64().unwrap(),
            cache_read: row["cache_read"].as_i64().unwrap(),
            cache_write: row["cache_write"].as_i64().unwrap(),
            output: row["output"].as_i64().unwrap(),
        })
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for day in daily_counts(&calls, 0) {
        rows.push((
            day.day.clone(),
            "ds_bot".to_string(),
            "tokens".to_string(),
            day.tokens,
        ));
        rows.push((
            day.day.clone(),
            "ds_bot".to_string(),
            "calls".to_string(),
            day.calls,
        ));
        if day.excluded > 0 {
            rows.push((
                day.day,
                "ds_bot".to_string(),
                "excluded_calls".to_string(),
                i64::try_from(day.excluded).unwrap_or(0),
            ));
        }
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/adapters/codex/fixtures");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    let parsed: Vec<_> = files
        .iter()
        .map(|path| parse_rollout(&std::fs::read_to_string(path).unwrap()))
        .collect();
    let excluded = combine_rollouts(&parsed)
        .map(|(_, count)| count)
        .unwrap_or(0);
    let mut days = BTreeMap::<String, i64>::new();
    for row in &parsed {
        for (at, tokens) in row.at_ms.iter().zip(&row.events) {
            let instant = UNIX_EPOCH + Duration::from_millis((*at).max(0) as u64);
            *days.entry(local_calendar_day(instant, 0)).or_default() += tokens;
        }
    }
    for (day, amount) in days {
        rows.push((day, "codex".to_string(), "tokens".to_string(), amount));
    }
    if excluded > 0 {
        rows.push((
            today.clone(),
            "codex".to_string(),
            "excluded_sessions".to_string(),
            i64::try_from(excluded).unwrap_or(0),
        ));
    }
    if let Some(newest) = parsed
        .iter()
        .filter(|row| !row.windows.is_empty())
        .max_by_key(|row| row.limits_at.unwrap_or(0))
    {
        for (index, window) in newest.windows.iter().enumerate() {
            let name = match window.minutes {
                Some(minutes) => format!("window_{minutes}_{index}_bps"),
                None => format!("window_unknown_{index}_bps"),
            };
            rows.push((
                "2026-10-24".to_string(),
                "codex".to_string(),
                name.clone(),
                i64::from(window.used_percent) * 100,
            ));
            if let Some(resets) = window.resets_at {
                rows.push((
                    "2026-10-24".to_string(),
                    "codex".to_string(),
                    format!("{name}_reset"),
                    resets,
                ));
            }
        }
    }
    let balances = parse_balance_json(include_str!(
        "../../crates/adapters/dsh/fixtures/balance.json"
    ))
    .unwrap();
    rows.push((
        today.clone(),
        "deepseek_harness".to_string(),
        balances[0].currency.metric().to_string(),
        balances[0].total_minor,
    ));
    let stamp = now.to_string();
    present_usage(&UsageFacts {
        now,
        offset_secs: 0,
        today,
        range_start: "2026-10-01".to_string(),
        range_end: "2026-10-25".to_string(),
        rows,
        reports: vec![
            (
                "codex".to_string(),
                "tokens".to_string(),
                "ok".to_string(),
                stamp.clone(),
            ),
            (
                "codex".to_string(),
                "windows".to_string(),
                "ok".to_string(),
                stamp.clone(),
            ),
            (
                "ds_bot".to_string(),
                "tokens".to_string(),
                "ok".to_string(),
                stamp.clone(),
            ),
            (
                "deepseek_harness".to_string(),
                "balance".to_string(),
                "ok".to_string(),
                stamp,
            ),
        ],
        key_saved: true,
        account_linked: true,
        codex_app_server: true,
        codex_access: RecordAccess::Authorized,
        bot_access: RecordAccess::Authorized,
    })
}

#[test]
fn m2_fixture_page_is_the_parsed_golden_sample() {
    let page = sample_page();
    assert!(
        page.cards.iter().any(|card| card.percent == Some(6)),
        "{:?}",
        page.cards
    );
    assert!(
        page.cards.iter().any(|card| card.percent == Some(1)),
        "{:?}",
        page.cards
    );
    assert!(
        page.cards.iter().any(|card| card.value.contains("12.34")),
        "{:?}",
        page.cards
    );
    assert_eq!(page.cards.len(), 4);
    assert_eq!(
        page.details
            .iter()
            .find(|row| row.name == "Cursor")
            .unwrap()
            .tokens,
        "暂不提供"
    );
    assert_eq!(
        page.details
            .iter()
            .find(|row| row.name == "DeepSeek Harness")
            .unwrap()
            .tokens,
        "暂不提供"
    );
    assert_ne!(page.total, "—");
    assert_eq!(page.range_label, "10 月 1 日 – 25 日");
    assert!(page
        .days
        .iter()
        .any(|day| day.codex.is_none() && day.bot.is_none()));
    assert!(page.days.iter().any(|day| day.bot.is_some()));
    let text = format!("{page:?}");
    assert!(!text.contains("128.40"));
    assert!(!text.contains("3.49"));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/src/fixture-m2.json");
    let json = format!("{}\n", serde_json::to_string_pretty(&page).unwrap());
    if !path.exists() || std::env::var_os("WANXIANG_WRITE_FIXTURE").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), json);
}
