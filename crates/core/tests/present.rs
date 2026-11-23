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
        harness_access: RecordAccess::Unauthorized,
        claude_access: RecordAccess::Unauthorized,
        antigravity_access: RecordAccess::Unauthorized,
        grok_access: RecordAccess::Unauthorized,
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
    assert!(text.contains("Harness 未授权，未计入"));
    assert!(text.contains("ChatGPT 未授权，未计入"));
    assert!(text.contains("DS Bot 未授权，未计入"));
    assert!(text.contains("未保存密钥"));
    assert!(text.contains("更早没有本机记录，不补算"));
    assert!(text.contains("按各工具自身周期，不受上方日期影响"));
    assert!(!text.contains("3.49"));
    assert!(!text.contains("128.40"));
    assert!(!text.contains("去授权"));
    assert!(text.contains("去设置授权"));
    assert_eq!(page.cards.len(), 7);
    assert!(text.contains("Claude 未授权，未计入"));
    assert!(text.contains("Antigravity 未授权，未计入"));
    assert!(text.contains("Gemini、WorkBuddy、Grok Bot、Muse 的 token 用量暂不提供，未计入"));
    assert_eq!(detail(&page, "Grok Bot").tokens, "暂不提供");
    assert_eq!(detail(&page, "Muse").tokens, "暂不提供");
    let grok = page.cards.iter().find(|card| card.name == "Grok Bot · 每周额度").unwrap();
    assert_eq!(grok.note, "未授权");
    assert_eq!(detail(&page, "Gemini").tokens, "暂不提供");
    assert_eq!(detail(&page, "WorkBuddy").calls, "暂不提供");
    assert_eq!(detail(&page, "Antigravity").source, "未授权");
    assert_eq!(page.total, "—");
    assert_eq!(page.total_reason, "未授权");
    assert_eq!(detail(&page, "ChatGPT").fee, "不提供");
    assert!(page.cards.iter().all(|card| card.note != "余额"));
}

#[test]
fn four_usage_states_stay_distinct() {
    let page = present_usage(&empty());
    assert_eq!(detail(&page, "ChatGPT").tokens, "未授权");
    assert_eq!(detail(&page, "DS Bot").calls, "未授权");
    assert_eq!(page.cards[0].link.as_deref(), Some("去设置授权"));
    assert_eq!(page.cards[0].value, "—");
    assert_eq!(detail(&page, "Cursor").tokens, "暂不提供");
    assert_eq!(detail(&page, "DeepSeek Harness").tokens, "未授权");

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
    assert_eq!(detail(&page, "ChatGPT").tokens, "需要重新授权");
    assert_eq!(detail(&page, "DS Bot").calls, "需要重新授权");
    assert_eq!(page.total, "—");
    assert_eq!(page.total_reason, "需要重新授权");
    assert!(page.banner.iter().any(|line| line == "ChatGPT 需要重新授权，未计入"));
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
    assert_eq!(detail(&page, "DeepSeek Harness").source, "未授权");
    assert_eq!(detail(&page, "DS Bot").fee, "不提供");
    assert!(page.banner.iter().any(|line| line.contains("Cursor 用量暂不提供")));
    assert!(page.banner.iter().any(|line| line == "Harness 未授权，未计入"));
    assert!(!page.total_reason.contains("暂不提供"));

    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.bot_access = RecordAccess::Authorized;
    facts.codex_app_server = true;
    facts.key_saved = true;
    facts.account_linked = true;
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "ChatGPT").tokens, "无本机记录");
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
    assert!(page.banner.iter().any(|line| line == "有 2 条调用不属于 Bot 或群聊，未计入 DS Bot"));
}

#[test]
fn partial_sessions_join_the_banner() {
    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.rows.push((
        "2026-11-09".to_string(),
        "codex".to_string(),
        "partial_sessions".to_string(),
        3,
    ));
    let page = present_usage(&facts);
    assert!(page.banner.iter().any(|line| line == "有 3 个会话只计入部分用量"));
}

#[test]
fn app_server_windows_say_actual_and_shorter_period_comes_first() {
    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.codex_app_server = true;
    facts.now = 1_700_000_000;
    for (metric, percent) in [("window_10080_1_bps", 2900_i64), ("window_300_0_bps", 6200)] {
        facts.rows.push(("2026-11-09".to_string(), "codex".to_string(), metric.to_string(), percent));
    }
    facts.rows.push((
        "2026-11-09".to_string(),
        "codex".to_string(),
        "window_source".to_string(),
        1,
    ));
    facts.reports.push((
        "codex".to_string(),
        "windows".to_string(),
        "ok".to_string(),
        facts.now.to_string(),
    ));
    let page = present_usage(&facts);
    assert!(page.cards[0].name.contains("5 小时"));
    assert!(page.cards[1].name.contains("7 天"));
    assert!(page.cards[0].note.starts_with("实际 · "));
    assert!(page.cards[1].note.starts_with("实际 · "));

    facts.rows.retain(|row| row.2 != "window_source");
    facts.rows.push((
        "2026-11-09".to_string(),
        "codex".to_string(),
        "window_source".to_string(),
        2,
    ));
    let page = present_usage(&facts);
    assert_eq!(page.cards[0].note, "本机记录");
}

#[test]
fn single_seven_day_window_names_the_missing_five_hour_card() {
    let mut facts = empty();
    facts.codex_access = RecordAccess::Authorized;
    facts.codex_app_server = true;
    facts.reports.push((
        "codex".to_string(),
        "windows".to_string(),
        "ok".to_string(),
        facts.now.to_string(),
    ));
    facts.rows.push((
        "2026-11-09".to_string(),
        "codex".to_string(),
        "window_10080_1_bps".to_string(),
        2900,
    ));
    let page = present_usage(&facts);
    assert_eq!(page.cards[0].name, "ChatGPT · 5 小时窗口");
    assert_eq!(page.cards[0].note, "无本机记录");
    assert_eq!(page.cards[1].name, "ChatGPT · 7 天窗口");
    assert_eq!(page.cards[1].percent, Some(29));
    assert!(page.cards.iter().all(|card| card.name != "ChatGPT · 窗口"));
}

#[test]
fn unauthorized_window_cards_keep_their_titles() {
    let page = present_usage(&empty());
    assert_eq!(page.cards[0].name, "ChatGPT · 5 小时窗口");
    assert_eq!(page.cards[1].name, "ChatGPT · 7 天窗口");
    assert_eq!(page.cards[0].note, "未授权");
}

#[test]
fn harness_usage_counts_once_and_joins_the_total() {
    let mut facts = empty();
    facts.harness_access = RecordAccess::Authorized;
    facts.bot_access = RecordAccess::Authorized;
    facts.rows.push(("2026-11-08".to_string(), "deepseek_harness".to_string(), "tokens".to_string(), 1_500));
    facts.rows.push(("2026-11-08".to_string(), "deepseek_harness".to_string(), "calls".to_string(), 3));
    facts.rows.push(("2026-11-09".to_string(), "ds_bot".to_string(), "tokens".to_string(), 500));
    facts.rows.push(("2026-11-09".to_string(), "ds_bot".to_string(), "calls".to_string(), 1));
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "DeepSeek Harness").tokens, "1,500");
    assert_eq!(detail(&page, "DeepSeek Harness").calls, "3");
    assert_eq!(detail(&page, "DeepSeek Harness").source, "本机记录");
    assert_eq!(page.total, "2,000");
    assert!(!page.banner.iter().any(|line| line.contains("Harness")));
    let day = page.days.iter().find(|day| day.label == "11 月 8 日").unwrap();
    assert_eq!(day.harness, Some(1_500));

    let mut facts = empty();
    facts.harness_access = RecordAccess::NeedsReauth;
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "DeepSeek Harness").tokens, "需要重新授权");
    assert!(page.banner.iter().any(|line| line == "Harness 需要重新授权，未计入"));
}

#[test]
fn claude_counts_tokens_and_shows_local_plan_windows() {
    let mut facts = empty();
    facts.claude_access = RecordAccess::Authorized;
    for (metric, amount) in [
        ("tokens", 1_200),
        ("calls", 3),
        ("window_300_0_bps", 4_900),
        ("window_10080_1_bps", 6_400),
        ("window_source", 2),
        ("window_at", facts.now - 600),
    ] {
        facts
            .rows
            .push(("2026-11-09".to_string(), "claude".to_string(), metric.to_string(), amount));
    }
    let page = present_usage(&facts);
    let five = page.cards.iter().find(|card| card.name == "Claude · 5 小时窗口").unwrap();
    let week = page.cards.iter().find(|card| card.name == "Claude · 每周额度").unwrap();
    assert_eq!(five.percent, Some(49));
    assert_eq!(week.percent, Some(64));
    assert!(five.note.starts_with("本机记录 · "));
    assert_eq!(detail(&page, "Claude").tokens, "1,200");
    assert_eq!(page.total, "1,200");
    assert!(!page.banner.iter().any(|line| line.contains("Claude")));
    assert_eq!(page.days.last().unwrap().claude, Some(1_200));

    let mut older = facts.clone();
    for row in older.rows.iter_mut() {
        if row.2 == "window_at" {
            row.3 = facts.now - 3 * 86_400;
        }
    }
    let page = present_usage(&older);
    let five = page.cards.iter().find(|card| card.name == "Claude · 5 小时窗口").unwrap();
    assert_eq!(five.percent, None);
    assert!(five.note.contains("过时"), "{}", five.note);
    let mut days_old = facts.clone();
    for row in days_old.rows.iter_mut() {
        if row.2 == "window_at" {
            row.3 = facts.now - 2 * 86_400;
        }
    }
    let page = present_usage(&days_old);
    let week = page.cards.iter().find(|card| card.name == "Claude · 每周额度").unwrap();
    assert_eq!(week.percent, Some(64));
    assert!(week.note.contains("月"), "{}", week.note);

    let mut empty_plan = empty();
    empty_plan.claude_access = RecordAccess::Authorized;
    let page = present_usage(&empty_plan);
    let five = page.cards.iter().find(|card| card.name == "Claude · 5 小时窗口").unwrap();
    assert_eq!(five.note, "无本机记录");
}

#[test]
fn antigravity_counts_tokens_and_calls_when_granted() {
    let mut facts = empty();
    facts.antigravity_access = RecordAccess::Authorized;
    let today = facts.today.clone();
    facts.rows.push((today.clone(), "antigravity".to_string(), "tokens".to_string(), 2_500));
    facts.rows.push((today, "antigravity".to_string(), "calls".to_string(), 3));
    let page = present_usage(&facts);
    assert_eq!(detail(&page, "Antigravity").tokens, "2,500");
    assert_eq!(detail(&page, "Antigravity").calls, "3");
    assert!(!page.banner.iter().any(|line| line.contains("Antigravity")));
    assert_eq!(page.total, "2,500");
    assert_eq!(page.days.last().unwrap().antigravity, Some(2_500));
}

#[test]
fn grok_bot_week_card_from_cached_reading() {
    let mut facts = empty();
    facts.grok_access = RecordAccess::Authorized;
    let page = present_usage(&facts);
    let card = page.cards.iter().find(|card| card.name == "Grok Bot · 每周额度").unwrap();
    assert_eq!(card.note, "无本机记录");
    for (metric, amount) in [
        ("window_10080_0_bps", 800),
        ("window_10080_0_bps_reset", facts.now + 2 * 86_400),
        ("window_at", facts.now - 600),
    ] {
        facts
            .rows
            .push((facts.today.clone(), "grok_bot".to_string(), metric.to_string(), amount));
    }
    let page = present_usage(&facts);
    let card = page.cards.iter().find(|card| card.name == "Grok Bot · 每周额度").unwrap();
    assert_eq!(card.percent, Some(8));
    assert!(card.note.starts_with("本机记录 · "));
    assert!(card.reset.as_deref().unwrap().ends_with("重置"));
    assert!(card.reset.as_deref().unwrap().contains("月"));
    // 重置时间已过：显示记录已过时
    let mut stale = facts.clone();
    for row in stale.rows.iter_mut() {
        if row.2 == "window_10080_0_bps_reset" {
            row.3 = facts.now - 10;
        }
    }
    let page = present_usage(&stale);
    let card = page.cards.iter().find(|card| card.name == "Grok Bot · 每周额度").unwrap();
    assert_eq!(card.note, "记录已过时");
}
