use wanxiang_core::{present_usage, RecordAccess, UsageFacts};

fn empty() -> UsageFacts {
    UsageFacts {
        now: 1_700_000_000,
        offset_secs: 0,
        today: "2026-11-09".to_string(),
        range_start: "2026-11-01".to_string(),
        range_end: "2026-11-09".to_string(),
        rows: Vec::new(),
        reports: Vec::new(),
        key_saved: false,
        account_linked: false,
        codex_app_server: false,
        codex_access: RecordAccess::Unauthorized,
        bot_access: RecordAccess::Unauthorized,
    }
}

fn detail<'a>(page: &'a wanxiang_core::UsagePage, name: &str) -> &'a wanxiang_core::UsageDetail {
    page.details.iter().find(|row| row.name == name).unwrap()
}

#[test]
fn empty_usage_names_the_gaps_and_skips_example_figures() {
    let page = present_usage(&empty());
    let text = format!("{page:?}");
    assert!(text.contains("Cursor 用量暂不提供，未计入"));
    assert!(text.contains("Harness 用量暂不提供，未计入"));
    assert!(text.contains("Codex 未授权，未计入"));
    assert!(text.contains("DS Bot 未授权，未计入"));
    assert!(text.contains("未保存密钥"));
    assert!(text.contains("更早没有本机记录，不补算"));
    assert!(text.contains("按各工具自身周期，不受上方日期影响"));
    assert!(!text.contains("3.49"));
    assert!(!text.contains("128.40"));
    assert!(!text.contains("去授权"));
    assert!(text.contains("去设置授权"));
    assert_eq!(page.cards.len(), 4);
    assert_eq!(page.total, "—");
    assert_eq!(page.total_reason, "未授权");
    assert_eq!(detail(&page, "Codex").fee, "不提供");
    assert!(page.cards.iter().all(|card| card.note != "余额"));
}

#[test]
fn four_usage_states_stay_distinct() {
    let page = present_usage(&empty());
    assert_eq!(detail(&page, "Codex").tokens, "未授权");
    assert_eq!(detail(&page, "DS Bot").calls, "未授权");
    assert_eq!(page.cards[0].link.as_deref(), Some("去设置授权"));
    assert_eq!(page.cards[0].value, "—");
    assert_eq!(detail(&page, "Cursor").tokens, "暂不提供");
    assert_eq!(detail(&page, "DeepSeek Harness").tokens, "暂不提供");

    let mut linked = empty();
    linked.key_saved = true;
    let page = present_usage(&linked);
    let balance = page.cards.iter().find(|card| card.name == "DeepSeek 账户").unwrap();
    assert_eq!(balance.note, "未授权");
    assert_eq!(balance.link.as_deref(), Some("去设置授权"));

    let mut facts = empty();
    facts.codex_access = RecordAccess::NeedsReauth;
    facts.bot_access = RecordAccess::NeedsReauth;
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "Codex").tokens, "需要重新授权");
    assert_eq!(detail(&page, "DS Bot").calls, "需要重新授权");
    assert_eq!(page.total, "—");
    assert_eq!(page.total_reason, "需要重新授权");
    assert!(page.banner.iter().any(|line| line == "Codex 需要重新授权，未计入"));
    assert_eq!(page.cards[0].link.as_deref(), Some("去设置授权"));

    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.bot_access = RecordAccess::Authorized;
    facts.codex_app_server = true;
    facts.key_saved = true;
    facts.account_linked = true;
    facts.rows.push(("2026-11-09".to_string(), "ds_bot".to_string(), "tokens".to_string(), 300));
    facts.rows.push(("2026-11-09".to_string(), "ds_bot".to_string(), "calls".to_string(), 2));
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "DS Bot").tokens, "300");
    assert_eq!(detail(&page, "Cursor").tokens, "暂不提供");
    assert_eq!(detail(&page, "DeepSeek Harness").fee, "不提供");
    assert_eq!(detail(&page, "DeepSeek Harness").source, "—");
    assert_eq!(detail(&page, "DS Bot").fee, "不提供");
    assert!(page.banner.iter().any(|line| line.contains("Cursor 用量暂不提供")));
    assert!(page.banner.iter().any(|line| line.contains("Harness 用量暂不提供")));
    assert!(!page.total_reason.contains("暂不提供"));

    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.bot_access = RecordAccess::Authorized;
    facts.codex_app_server = true;
    facts.key_saved = true;
    facts.account_linked = true;
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "Codex").tokens, "无本机记录");
    assert_eq!(detail(&page, "DS Bot").tokens, "无本机记录");
    assert_eq!(page.cards[0].note, "无本机记录");
    assert_eq!(page.cards.iter().find(|card| card.name == "DeepSeek 账户").unwrap().note, "无本机记录");
    assert_eq!(page.total, "—");
    assert_eq!(page.total_reason, "无本机记录");
    assert_eq!(page.range_label, "11 月 1 日 – 9 日");
    assert_ne!(page.cards.iter().find(|card| card.name == "Cursor").unwrap().note, "无本机记录");
}

#[test]
fn missing_window_duration_is_unknown_period() {
    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.codex_app_server = true;
    facts.rows.push((
        "2026-11-09".to_string(),
        "codex".to_string(),
        "window_unknown_0_bps".to_string(),
        3400,
    ));
    facts.reports.push((
        "codex".to_string(),
        "windows".to_string(),
        "ok".to_string(),
        facts.now.to_string(),
    ));
    let page = present_usage(&facts);
    assert_eq!(page.cards[0].percent, Some(34));
    assert!(page.cards[0].name.contains("周期未知"));
    assert!(page.cards[0].reset.is_none());
}

#[test]
fn excluded_calls_are_named_in_the_banner() {
    let mut facts = empty();
    facts.bot_access = RecordAccess::Authorized;
    facts.rows.push((
        "2026-11-09".to_string(),
        "ds_bot".to_string(),
        "excluded_calls".to_string(),
        2,
    ));
    let page = present_usage(&facts);
    assert!(page.banner.iter().any(|line| line == "有 2 条调用无法归属，未计入"));
}
