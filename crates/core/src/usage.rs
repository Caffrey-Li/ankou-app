use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Datelike, NaiveDate, TimeZone, Utc};

pub const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);
pub const STALE_AFTER: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageState {
    Ok,
    NoKey,
    InvalidKey,
    Network,
    QueryFailed,
    AppServer,
    AppOff,
    NeedsReauth,
    Unreadable,
    Unavailable,
}

impl UsageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NoKey => "no_key",
            Self::InvalidKey => "invalid_key",
            Self::Network => "network",
            Self::QueryFailed => "query_failed",
            Self::AppServer => "app_server",
            Self::AppOff => "app_off",
            Self::NeedsReauth => "needs_reauth",
            Self::Unreadable => "unreadable",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "ok" => Self::Ok,
            "no_key" => Self::NoKey,
            "invalid_key" => Self::InvalidKey,
            "network" => Self::Network,
            "query_failed" => Self::QueryFailed,
            "app_server" => Self::AppServer,
            "app_off" => Self::AppOff,
            "needs_reauth" => Self::NeedsReauth,
            "unreadable" => Self::Unreadable,
            "unavailable" => Self::Unavailable,
            _ => return None,
        })
    }
}

impl fmt::Display for UsageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "已更新",
            Self::NoKey => "未保存密钥",
            Self::InvalidKey => "密钥无效",
            Self::Network => "网络失败",
            Self::QueryFailed => "查询失败",
            Self::AppServer => "额度窗口不可用",
            Self::AppOff => "应用未运行",
            Self::NeedsReauth => "需要重新授权",
            Self::Unreadable => "本地记录读不了",
            Self::Unavailable => "暂不提供",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UsageError {
    NoKey,
    InvalidKey,
    Network,
    QueryFailed,
    AppServer,
    AppOff,
    NeedsReauth,
    Unreadable,
    Unavailable,
    Stopped,
}

impl UsageError {
    pub fn state(self) -> UsageState {
        match self {
            Self::NoKey => UsageState::NoKey,
            Self::InvalidKey => UsageState::InvalidKey,
            Self::Network => UsageState::Network,
            Self::QueryFailed => UsageState::QueryFailed,
            Self::AppServer => UsageState::AppServer,
            Self::AppOff => UsageState::AppOff,
            Self::NeedsReauth => UsageState::NeedsReauth,
            Self::Unreadable => UsageState::Unreadable,
            Self::Unavailable | Self::Stopped => UsageState::Unavailable,
        }
    }
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => f.write_str("已停止"),
            other => write!(f, "{}", other.state()),
        }
    }
}

impl fmt::Debug for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Currency {
    Cny,
    Usd,
}

impl Currency {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "CNY" => Some(Self::Cny),
            "USD" => Some(Self::Usd),
            _ => None,
        }
    }

    pub fn metric(self) -> &'static str {
        match self {
            Self::Cny => "balance_cny_fen",
            Self::Usd => "balance_usd_cent",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Cny => "¥",
            Self::Usd => "$",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalanceAmount {
    pub currency: Currency,
    pub total_minor: i64,
    pub granted_minor: i64,
    pub topped_up_minor: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSample {
    pub minutes: Option<i64>,
    pub used_percent: i32,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageSnapshot {
    pub tokens: Vec<(String, i64)>,
    pub calls: Vec<(String, i64)>,
    pub balances: Vec<BalanceAmount>,
    pub windows: Vec<WindowSample>,
    pub excluded_calls: u64,
    pub excluded_call_days: Vec<(String, i64)>,
    pub excluded_sessions: u64,
    pub partial_sessions: u64,
    pub partial: bool,
    pub window_live: bool,
    pub manage_session_counts: bool,
}

impl UsageSnapshot {
    pub fn empty() -> Self {
        Self {
            tokens: Vec::new(),
            calls: Vec::new(),
            balances: Vec::new(),
            windows: Vec::new(),
            excluded_calls: 0,
            excluded_call_days: Vec::new(),
            excluded_sessions: 0,
            partial_sessions: 0,
            partial: false,
            window_live: false,
            manage_session_counts: false,
        }
    }
}

pub fn window_title(minutes: Option<i64>) -> String {
    match minutes {
        Some(300) => "5 小时窗口".to_string(),
        Some(10080) => "7 天窗口".to_string(),
        Some(mins) if mins > 0 => format!("{mins} 分钟窗口"),
        _ => "周期未知".to_string(),
    }
}

pub fn accept_percent(value: i64) -> Option<i32> {
    if (0..=100).contains(&value) {
        i32::try_from(value).ok()
    } else {
        None
    }
}

/// 把金额字符串换成最小货币单位。非法输入返回 None，不折成 0。
pub fn parse_minor_units(text: &str) -> Option<i64> {
    if text.is_empty()
        || text
            .as_bytes()
            .iter()
            .any(|byte| byte.is_ascii_whitespace())
    {
        return None;
    }
    let (whole, frac) = match text.split_once('.') {
        Some((whole, frac)) => (whole, frac),
        None => (text, ""),
    };
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if frac.len() > 2 || !frac.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let yuan: i64 = whole.parse().ok()?;
    let frac_scale: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse().ok()?,
    };
    yuan.checked_mul(100)?.checked_add(frac_scale)
}

pub fn format_minor(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let minor = minor.unsigned_abs();
    format!("{sign}{}.{:02}", minor / 100, minor % 100)
}

pub fn unix_secs(now: SystemTime) -> i64 {
    now.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn local_calendar_day(now: SystemTime, offset_secs: i32) -> String {
    let shifted = unix_secs(now).saturating_add(i64::from(offset_secs));
    Utc.timestamp_opt(shifted, 0)
        .single()
        .map(|time| time.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

pub fn system_offset_secs(now: SystemTime) -> i32 {
    Utc.timestamp_opt(unix_secs(now), 0)
        .single()
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .offset()
                .local_minus_utc()
        })
        .unwrap_or(0)
}

pub fn calendar_cutoff(today: &str, months: u32) -> Option<String> {
    let date = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    let months_i = i32::try_from(months).ok()?;
    let mut month = date.month() as i32 - months_i;
    let mut year = date.year();
    while month <= 0 {
        month += 12;
        year -= 1;
    }
    let day = date.day().min(days_in_month(year, month as u32)?);
    NaiveDate::from_ymd_opt(year, month as u32, day)
        .map(|cutoff| cutoff.format("%Y-%m-%d").to_string())
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)?
        .pred_opt()
        .map(|date| date.day())
}

pub fn clock_at_utc(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> SystemTime {
    let time = Utc
        .with_ymd_and_hms(year, month, day, hour, min, sec)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("纪元"));
    UNIX_EPOCH + Duration::from_secs(time.timestamp().max(0) as u64)
}
