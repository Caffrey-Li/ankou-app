use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc};

use crate::{format_minor, window_title, Currency, STALE_AFTER};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordAccess {
    Unauthorized,
    NeedsReauth,
    Authorized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageFacts {
    pub now: i64,
    pub offset_secs: i32,
    pub today: String,
    pub range_start: String,
    pub range_end: String,
    pub rows: Vec<(String, String, String, i64)>,
    pub reports: Vec<(String, String, String, String)>,
    pub key_saved: bool,
    pub account_linked: bool,
    pub codex_app_server: bool,
    pub codex_access: RecordAccess,
    pub bot_access: RecordAccess,
    pub harness_access: RecordAccess,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UsageCard {
    pub name: String,
    pub value: String,
    pub note: String,
    pub percent: Option<i32>,
    pub reset: Option<String>,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UsageDay {
    pub label: String,
    pub codex: Option<i64>,
    pub bot: Option<i64>,
    pub harness: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UsageDetail {
    pub name: String,
    pub tokens: String,
    pub calls: String,
    pub fee: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UsagePage {
    pub stamp: String,
    pub banner: Vec<String>,
    pub cards: Vec<UsageCard>,
    pub days: Vec<UsageDay>,
    pub details: Vec<UsageDetail>,
    pub caption: String,
    pub empty_note: String,
    pub range_label: String,
    pub total: String,
    pub total_reason: String,
}

pub fn present_usage(facts: &UsageFacts) -> UsagePage {
    let codex_open = facts.codex_access == RecordAccess::Authorized;
    let bot_open = facts.bot_access == RecordAccess::Authorized;
    let codex_tokens = codex_open
        .then(|| sum_metric(facts, "codex", "tokens"))
        .flatten();
    let bot_tokens = bot_open
        .then(|| sum_metric(facts, "ds_bot", "tokens"))
        .flatten();
    let bot_calls = bot_open
        .then(|| sum_metric(facts, "ds_bot", "calls"))
        .flatten();
    let harness_open = facts.harness_access == RecordAccess::Authorized;
    let harness_tokens = harness_open
        .then(|| sum_metric(facts, "deepseek_harness", "tokens"))
        .flatten();
    let harness_calls = harness_open
        .then(|| sum_metric(facts, "deepseek_harness", "calls"))
        .flatten();
    let excluded_calls = if bot_open {
        sum_metric(facts, "ds_bot", "excluded_calls").unwrap_or(0)
    } else {
        0
    };
    let excluded_sessions = if codex_open {
        sum_metric(facts, "codex", "excluded_sessions").unwrap_or(0)
    } else {
        0
    };
    let balance = latest_amount(facts, "deepseek_harness", "balance_cny_fen")
        .or_else(|| latest_amount(facts, "deepseek_harness", "balance_usd_cent"));
    let balance_currency = if latest_amount(facts, "deepseek_harness", "balance_cny_fen").is_some()
    {
        Currency::Cny
    } else {
        Currency::Usd
    };
    let windows = if codex_open {
        latest_windows(facts)
    } else {
        Vec::new()
    };
    let window_report = report(facts, "codex", "windows");
    let balance_report = report(facts, "deepseek_harness", "balance");
    let mut banner = vec!["Cursor 用量暂不提供，未计入".to_string()];
    match facts.harness_access {
        RecordAccess::Unauthorized => banner.push("Harness 未授权，未计入".to_string()),
        RecordAccess::NeedsReauth => banner.push("Harness 需要重新授权，未计入".to_string()),
        RecordAccess::Authorized => {}
    }
    match facts.codex_access {
        RecordAccess::Unauthorized => banner.push("ChatGPT 未授权，未计入".to_string()),
        RecordAccess::NeedsReauth => banner.push("ChatGPT 需要重新授权，未计入".to_string()),
        RecordAccess::Authorized => {
            if !facts.codex_app_server {
                if let Some((_, at)) = report(facts, "codex", "tokens") {
                    banner.push(format!(
                        "ChatGPT 显示 {} 的数据（应用未运行）",
                        hhmm(at, facts.offset_secs)
                    ));
                }
            }
            if excluded_sessions > 0 {
                banner.push(format!("有 {excluded_sessions} 个会话没有用量记录，未计入"));
            }
            let partial_sessions = sum_metric(facts, "codex", "partial_sessions").unwrap_or(0);
            if partial_sessions > 0 {
                banner.push(format!("有 {partial_sessions} 个会话只计入部分用量"));
            }
        }
    }
    match facts.bot_access {
        RecordAccess::Unauthorized => banner.push("DS Bot 未授权，未计入".to_string()),
        RecordAccess::NeedsReauth => banner.push("DS Bot 需要重新授权，未计入".to_string()),
        RecordAccess::Authorized if excluded_calls > 0 => {
            banner.push(format!("有 {excluded_calls} 条调用不属于 Bot 或群聊，未计入 DS Bot"));
        }
        RecordAccess::Authorized => {}
    }
    let window_hidden =
        !facts.codex_app_server || window_report.map(|(state, _)| state) == Some("app_off");
    let (balance_value, balance_note, balance_link) =
        balance_card(facts, balance, balance_currency, balance_report);
    let counted = [codex_tokens, bot_tokens, harness_tokens]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let (total, total_reason) = if counted.is_empty() {
        ("—".to_string(), total_reason(facts).to_string())
    } else {
        (group_int(counted.iter().sum()), String::new())
    };
    UsagePage {
        stamp: hhmm(facts.now, facts.offset_secs),
        banner,
        cards: {
            let mut cards = codex_cards(facts, &windows, window_hidden, window_report);
            cards.push(UsageCard {
                name: "DeepSeek 账户".to_string(),
                value: balance_value,
                note: balance_note,
                percent: None,
                reset: None,
                link: balance_link,
            });
            cards.push(UsageCard {
                name: "Cursor".to_string(),
                value: "—".to_string(),
                note: "暂不提供".to_string(),
                percent: None,
                reset: None,
                link: None,
            });
            cards
        },
        days: days_between(&facts.range_start, &facts.range_end)
            .into_iter()
            .map(|day| UsageDay {
                label: day_label(&day),
                codex: codex_open
                    .then(|| day_amount(facts, &day, "codex", "tokens"))
                    .flatten(),
                bot: bot_open
                    .then(|| day_amount(facts, &day, "ds_bot", "tokens"))
                    .flatten(),
                harness: harness_open
                    .then(|| day_amount(facts, &day, "deepseek_harness", "tokens"))
                    .flatten(),
            })
            .collect(),
        details: vec![
            counted_detail("ChatGPT", facts.codex_access, codex_tokens, None, true),
            counted_detail(
                "DeepSeek Harness",
                facts.harness_access,
                harness_tokens,
                harness_calls,
                false,
            ),
            counted_detail("DS Bot", facts.bot_access, bot_tokens, bot_calls, false),
            withheld_detail("Cursor"),
        ],
        caption: "按各工具自身周期，不受上方日期影响".to_string(),
        empty_note: "更早没有本机记录，不补算".to_string(),
        range_label: range_label(&facts.range_start, &facts.range_end),
        total,
        total_reason,
    }
}

fn total_reason(facts: &UsageFacts) -> &'static str {
    let open = facts.codex_access == RecordAccess::Authorized
        || facts.bot_access == RecordAccess::Authorized
        || facts.harness_access == RecordAccess::Authorized;
    let reauth = facts.codex_access == RecordAccess::NeedsReauth
        || facts.bot_access == RecordAccess::NeedsReauth
        || facts.harness_access == RecordAccess::NeedsReauth;
    if open {
        "无本机记录"
    } else if reauth {
        "需要重新授权"
    } else {
        "未授权"
    }
}

fn balance_card(
    facts: &UsageFacts,
    balance: Option<i64>,
    currency: Currency,
    report: Option<(&str, i64)>,
) -> (String, String, Option<String>) {
    if !facts.key_saved {
        return ("—".to_string(), "未保存密钥".to_string(), None);
    }
    if !facts.account_linked {
        return (
            "—".to_string(),
            "未授权".to_string(),
            Some("去设置授权".to_string()),
        );
    }
    if let Some(amount) = balance {
        let at = report.map(|(_, at)| at).unwrap_or(facts.now);
        let clock = hhmm(at, facts.offset_secs);
        let fresh = report.map(|(state, _)| state) == Some("ok")
            && facts.now.saturating_sub(at) <= STALE_AFTER.as_secs() as i64;
        let note = if fresh {
            format!("实际 · {clock}")
        } else {
            format!("过时 · {clock}")
        };
        return (
            format!("{}{}", currency.symbol(), format_minor(amount)),
            note,
            None,
        );
    }
    let note = match report.map(|(state, _)| state) {
        Some("invalid_key") => "密钥无效",
        Some("network") => "网络失败",
        Some("query_failed") => "查询失败",
        Some("no_key") => "未保存密钥",
        _ => "无本机记录",
    };
    ("—".to_string(), note.to_string(), None)
}

pub fn month_start(today: &str) -> String {
    let Some(date) = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok() else {
        return today.to_string();
    };
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
        .map(|day| day.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| today.to_string())
}

pub fn shift_days(today: &str, days: i64) -> String {
    let Some(date) = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok() else {
        return today.to_string();
    };
    (date + Duration::days(days)).format("%Y-%m-%d").to_string()
}

fn range_label(start: &str, end: &str) -> String {
    let Some(left) = NaiveDate::parse_from_str(start, "%Y-%m-%d").ok() else {
        return format!("{start} – {end}");
    };
    let Some(right) = NaiveDate::parse_from_str(end, "%Y-%m-%d").ok() else {
        return format!("{start} – {end}");
    };
    if left.year() == right.year() && left.month() == right.month() {
        format!("{} 月 {} 日 – {} 日", left.month(), left.day(), right.day())
    } else if left.year() == right.year() {
        format!(
            "{} 月 {} 日 – {} 月 {} 日",
            left.month(),
            left.day(),
            right.month(),
            right.day()
        )
    } else {
        format!("{} – {}", day_label(start), day_label(end))
    }
}

fn counted_detail(
    name: &str,
    access: RecordAccess,
    tokens: Option<i64>,
    calls: Option<i64>,
    dash_calls: bool,
) -> UsageDetail {
    match access {
        RecordAccess::Unauthorized => gap_detail(name, "未授权"),
        RecordAccess::NeedsReauth => gap_detail(name, "需要重新授权"),
        RecordAccess::Authorized => {
            let has_tokens = tokens.is_some();
            UsageDetail {
                name: name.to_string(),
                tokens: tokens
                    .map(group_int)
                    .unwrap_or_else(|| "无本机记录".to_string()),
                calls: if dash_calls && has_tokens {
                    "—".to_string()
                } else if dash_calls {
                    "无本机记录".to_string()
                } else {
                    calls
                        .map(group_int)
                        .unwrap_or_else(|| "无本机记录".to_string())
                },
                fee: "不提供".to_string(),
                source: if has_tokens {
                    "本机记录"
                } else {
                    "无本机记录"
                }
                .to_string(),
            }
        }
    }
}

fn gap_detail(name: &str, text: &str) -> UsageDetail {
    UsageDetail {
        name: name.to_string(),
        tokens: text.to_string(),
        calls: text.to_string(),
        fee: "不提供".to_string(),
        source: text.to_string(),
    }
}

fn withheld_detail(name: &str) -> UsageDetail {
    UsageDetail {
        name: name.to_string(),
        tokens: "暂不提供".to_string(),
        calls: "暂不提供".to_string(),
        fee: "不提供".to_string(),
        source: "—".to_string(),
    }
}

fn report<'a>(facts: &'a UsageFacts, tool: &str, metric: &str) -> Option<(&'a str, i64)> {
    facts
        .reports
        .iter()
        .find(|row| row.0 == tool && row.1 == metric)
        .map(|row| (row.2.as_str(), row.3.parse().unwrap_or(0)))
}

fn sum_metric(facts: &UsageFacts, tool: &str, metric: &str) -> Option<i64> {
    let mut seen = false;
    let mut total = 0;
    for (day, row_tool, row_metric, amount) in &facts.rows {
        if row_tool == tool && row_metric == metric && in_range(day, facts) {
            seen = true;
            total += amount;
        }
    }
    seen.then_some(total)
}

fn day_amount(facts: &UsageFacts, day: &str, tool: &str, metric: &str) -> Option<i64> {
    facts
        .rows
        .iter()
        .find(|row| row.0 == day && row.1 == tool && row.2 == metric)
        .map(|row| row.3)
}

fn latest_amount(facts: &UsageFacts, tool: &str, metric: &str) -> Option<i64> {
    facts
        .rows
        .iter()
        .filter(|row| row.1 == tool && row.2 == metric && row.0.as_str() <= facts.today.as_str())
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|row| row.3)
}

struct WindowView {
    percent: i32,
    minutes: Option<i64>,
    resets_at: Option<i64>,
}

fn codex_cards(
    facts: &UsageFacts,
    windows: &[WindowView],
    hidden: bool,
    window_report: Option<(&str, i64)>,
) -> Vec<UsageCard> {
    let gap = match facts.codex_access {
        RecordAccess::Unauthorized => Some("未授权"),
        RecordAccess::NeedsReauth => Some("需要重新授权"),
        RecordAccess::Authorized => None,
    };
    if let Some(note) = gap {
        return vec![
            status_card(FIVE_HOUR_CARD, note, true),
            status_card(SEVEN_DAY_CARD, note, true),
        ];
    }
    if hidden || windows.is_empty() {
        let note = if hidden {
            "应用未运行"
        } else {
            match window_report.map(|(state, _)| state) {
                Some("app_server") => "额度窗口不可用",
                Some("unreadable") => "本地记录读不了",
                _ => "无本机记录",
            }
        };
        return vec![
            status_card(FIVE_HOUR_CARD, note, false),
            status_card(SEVEN_DAY_CARD, note, false),
        ];
    }
    let live = latest_amount(facts, "codex", "window_source") == Some(1);
    let origin = if live {
        match window_report {
            Some((_, at)) => format!("实际 · {}", hhmm(at, facts.offset_secs)),
            None => "实际 · —".to_string(),
        }
    } else {
        "本机记录".to_string()
    };
    let mut cards: Vec<_> = windows
        .iter()
        .take(2)
        .map(|window| UsageCard {
            name: format!("ChatGPT · {}", window_title(window.minutes)),
            value: format!("{}%", window.percent),
            note: origin.clone(),
            percent: Some(window.percent),
            reset: window
                .resets_at
                .map(|at| format!("{} 重置", hhmm(at, facts.offset_secs))),
            link: None,
        })
        .collect();
    if cards.len() == 1 {
        // 只读到一个窗口时，另一张卡写明缺的是哪个窗口，顺序固定为 5 小时在前（Figma 18:320）。
        if windows[0].minutes == Some(SEVEN_DAY_MINUTES) {
            cards.insert(0, status_card(FIVE_HOUR_CARD, "无本机记录", false));
        } else if windows[0].minutes == Some(FIVE_HOUR_MINUTES) {
            cards.push(status_card(SEVEN_DAY_CARD, "无本机记录", false));
        } else {
            cards.push(status_card("ChatGPT · 其他窗口", "无本机记录", false));
        }
    } else if cards.len() == 2 && windows[0].minutes == Some(SEVEN_DAY_MINUTES) {
        cards.swap(0, 1);
    }
    cards
}

const FIVE_HOUR_MINUTES: i64 = 300;
const SEVEN_DAY_MINUTES: i64 = 10080;
const FIVE_HOUR_CARD: &str = "ChatGPT · 5 小时窗口";
const SEVEN_DAY_CARD: &str = "ChatGPT · 7 天窗口";

fn status_card(name: &str, note: &str, link: bool) -> UsageCard {
    UsageCard {
        name: name.to_string(),
        value: "—".to_string(),
        note: note.to_string(),
        percent: None,
        reset: None,
        link: link.then(|| "去设置授权".to_string()),
    }
}

fn latest_windows(facts: &UsageFacts) -> Vec<WindowView> {
    let day = facts
        .rows
        .iter()
        .filter(|row| {
            row.1 == "codex"
                && row.2.starts_with("window_")
                && !row.2.ends_with("_reset")
                && row.0.as_str() <= facts.today.as_str()
        })
        .map(|row| row.0.as_str())
        .max()
        .unwrap_or("");
    if day.is_empty() {
        return Vec::new();
    }
    let mut windows: Vec<_> = facts
        .rows
        .iter()
        .filter(|row| {
            row.0 == day
                && row.1 == "codex"
                && row.2.starts_with("window_")
                && !row.2.ends_with("_reset")
        })
        .filter_map(|row| {
            let percent = i32::try_from(row.3 / 100).ok()?;
            let resets_at = facts
                .rows
                .iter()
                .find(|reset| reset.0 == row.0 && reset.2 == format!("{}_reset", row.2))
                .map(|reset| reset.3);
            if resets_at.is_some_and(|at| at <= facts.now) {
                return None;
            }
            Some(WindowView {
                percent,
                minutes: minutes_of(&row.2),
                resets_at,
            })
        })
        .collect();
    windows.sort_by_key(|window| window.minutes.unwrap_or(i64::MAX));
    windows.truncate(2);
    windows
}

fn minutes_of(metric: &str) -> Option<i64> {
    let rest = metric.strip_prefix("window_")?.strip_suffix("_bps")?;
    let (minutes, _) = rest.rsplit_once('_')?;
    if minutes == "unknown" {
        None
    } else {
        minutes.parse().ok()
    }
}

fn in_range(day: &str, facts: &UsageFacts) -> bool {
    day >= facts.range_start.as_str() && day <= facts.range_end.as_str()
}

fn days_between(start: &str, end: &str) -> Vec<String> {
    let mut days = Vec::new();
    let Some(mut cursor) = NaiveDate::parse_from_str(start, "%Y-%m-%d").ok() else {
        return days;
    };
    let Some(end) = NaiveDate::parse_from_str(end, "%Y-%m-%d").ok() else {
        return days;
    };
    while cursor <= end && days.len() < 62 {
        days.push(cursor.format("%Y-%m-%d").to_string());
        cursor += Duration::days(1);
    }
    days
}

fn day_label(day: &str) -> String {
    let Some(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok() else {
        return day.to_string();
    };
    format!("{} 月 {} 日", date.month(), date.day())
}

fn hhmm(unix: i64, offset_secs: i32) -> String {
    let shifted = unix.saturating_add(i64::from(offset_secs));
    Utc.timestamp_opt(shifted, 0)
        .single()
        .map(|time| time.format("%H:%M").to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn group_int(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, byte) in digits.bytes().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(byte as char);
    }
    out.push_str(sign);
    out.chars().rev().collect()
}
