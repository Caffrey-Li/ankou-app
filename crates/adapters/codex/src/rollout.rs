use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::DateTime;
use serde_json::Value;
use wanxiang_core::{accept_percent, WindowSample};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolloutUsage {
    pub samples: u32,
    pub events: Vec<i64>,
    pub at_ms: Vec<i64>,
    pub incomplete: bool,
    pub compactions: Option<u32>,
    pub windows: Vec<WindowSample>,
    pub limits_at: Option<i64>,
}

impl RolloutUsage {
    pub fn tokens(&self) -> Option<i64> {
        if self.samples == 0 {
            None
        } else {
            Some(self.events.iter().sum())
        }
    }
}

#[derive(Clone, Copy)]
struct Counts {
    input: i64,
    cached: i64,
    output: i64,
    reasoning: i64,
    total: i64,
}

struct Building {
    samples: u32,
    events: Vec<i64>,
    at_ms: Vec<i64>,
    incomplete: bool,
    compactions: Option<u32>,
    previous: Option<Counts>,
    limits_at: Option<i64>,
    windows: Vec<WindowSample>,
}

pub fn parse_rollout(text: &str) -> RolloutUsage {
    let mut building = Building {
        samples: 0,
        events: Vec::new(),
        at_ms: Vec::new(),
        incomplete: false,
        compactions: None,
        previous: None,
        limits_at: None,
        windows: Vec::new(),
    };
    for line in text.split(['\n', '\r']) {
        consume(&mut building, line);
    }
    RolloutUsage {
        samples: building.samples,
        events: building.events,
        at_ms: building.at_ms,
        incomplete: building.incomplete,
        compactions: building.compactions,
        windows: building.windows,
        limits_at: building.limits_at,
    }
}

pub fn combine_rollouts(rows: &[RolloutUsage]) -> Option<(i64, u64)> {
    let counted: Vec<&RolloutUsage> = rows.iter().filter(|row| row.samples > 0).collect();
    if counted.is_empty() {
        return None;
    }
    let tokens = counted
        .iter()
        .map(|row| row.events.iter().sum::<i64>())
        .sum();
    let excluded = u64::try_from(rows.len() - counted.len()).unwrap_or(0);
    Some((tokens, excluded))
}

pub fn rollout_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    walk(&root.join("sessions"), &mut paths);
    paths.sort();
    paths
}

pub fn read_rollouts(root: &Path, opened: &mut Vec<PathBuf>) -> Vec<RolloutUsage> {
    rollout_paths(root)
        .into_iter()
        .filter_map(|path| {
            let file = File::open(&path).ok()?;
            opened.push(path);
            let mut text = String::new();
            let mut reader = BufReader::new(file);
            let mut line = String::new();
            while reader.read_line(&mut line).ok()? > 0 {
                text.push_str(&line);
                line.clear();
            }
            Some(parse_rollout(&text))
        })
        .collect()
}

fn walk(dir: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, paths);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            paths.push(path);
        }
    }
}

fn consume(rollout: &mut Building, line: &str) {
    if !["session_meta", "turn_context", "token_count", "compacted"]
        .iter()
        .any(|key| line.contains(key))
    {
        return;
    }
    let event: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => {
            rollout.incomplete = true;
            return;
        }
    };
    let Some(payload) = event.get("payload").filter(|value| value.is_object()) else {
        return;
    };
    let at = event.get("timestamp").and_then(timestamp_ms);
    match event.get("type").and_then(|value| value.as_str()) {
        Some("session_meta") | Some("turn_context") => {}
        Some("compacted") => {
            rollout.compactions = Some(rollout.compactions.unwrap_or(0) + 1);
        }
        Some("event_msg") if payload.get("type").and_then(|value| value.as_str()) == Some("token_count") => {
            remember_limits(rollout, payload.get("rate_limits"), at);
            let Some(info) = payload.get("info") else {
                return;
            };
            let Some(current) = counts(info.get("total_token_usage")) else {
                rollout.incomplete = true;
                return;
            };
            let last = counts(info.get("last_token_usage"));
            let previous = rollout.previous;
            let reset = previous
                .map(|previous| current.total < previous.total)
                .unwrap_or(false);
            let initial = match last {
                Some(last) if last.total <= current.total => last,
                _ => current,
            };
            let delta = if previous.is_none() || reset {
                initial
            } else {
                let previous = previous.expect("上一笔累计");
                Counts {
                    input: current.input - previous.input,
                    cached: current.cached - previous.cached,
                    output: current.output - previous.output,
                    reasoning: current.reasoning - previous.reasoning,
                    total: current.total - previous.total,
                }
            };
            if (previous.is_none() || reset) && current.total > delta.total {
                rollout.incomplete = true;
            }
            rollout.previous = Some(current);
            if at.is_none() || delta.input < 0 || delta.cached < 0 || delta.output < 0 || delta.reasoning < 0 || delta.total < 0
            {
                rollout.incomplete = true;
                return;
            }
            rollout.samples += 1;
            if delta.total > 0 {
                rollout.events.push(delta.total);
                rollout.at_ms.push(at.expect("时间戳"));
            }
        }
        _ => {}
    }
}

fn remember_limits(rollout: &mut Building, limits: Option<&Value>, at: Option<i64>) {
    let (Some(at), Some(limits)) = (at, limits) else {
        return;
    };
    let limit_id = limits.get("limit_id").and_then(|value| value.as_str());
    if limit_id.is_some_and(|id| id != "codex") {
        return;
    }
    if rollout.limits_at.is_some_and(|seen| at < seen) {
        return;
    }
    rollout.limits_at = Some(at);
    rollout.windows = ["primary", "secondary"]
        .into_iter()
        .filter_map(|name| limits.get(name).and_then(parse_rollout_window))
        .collect();
}

fn parse_rollout_window(value: &Value) -> Option<WindowSample> {
    if value.is_null() {
        return None;
    }
    let used = accept_percent(integer(value.get("used_percent")?)?)?;
    let minutes = match value.get("window_minutes") {
        None | Some(Value::Null) => None,
        Some(item) => Some(integer(item).filter(|minutes| *minutes > 0)?),
    };
    let resets_at = match value.get("resets_at") {
        None | Some(Value::Null) => None,
        Some(item) => integer(item),
    };
    Some(WindowSample {
        minutes,
        used_percent: used,
        resets_at,
    })
}

pub fn parse_app_server_result(value: &Value) -> Vec<WindowSample> {
    let rate_limits = value
        .pointer("/result/rateLimits")
        .or_else(|| value.get("rateLimits"));
    let Some(rate_limits) = rate_limits else {
        return Vec::new();
    };
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|name| rate_limits.get(name).and_then(parse_app_window))
        .collect()
}

fn parse_app_window(value: &Value) -> Option<WindowSample> {
    if value.is_null() {
        return None;
    }
    let used = accept_percent(integer(value.get("usedPercent")?)?)?;
    let minutes = match value.get("windowDurationMins") {
        None | Some(Value::Null) => None,
        Some(item) => Some(integer(item).filter(|minutes| *minutes > 0)?),
    };
    let resets_at = match value.get("resetsAt") {
        None | Some(Value::Null) => None,
        Some(item) => integer(item),
    };
    Some(WindowSample {
        minutes,
        used_percent: used,
        resets_at,
    })
}

fn counts(value: Option<&Value>) -> Option<Counts> {
    let value = value?;
    let input = integer(value.get("input_tokens")?)?;
    let cached = integer(value.get("cached_input_tokens")?)?;
    let output = integer(value.get("output_tokens")?)?;
    let reasoning = integer(value.get("reasoning_output_tokens")?)?;
    let total = integer(value.get("total_tokens")?)?;
    if input < 0 || cached < 0 || output < 0 || reasoning < 0 || total < 0 || cached > input || reasoning > output {
        return None;
    }
    Some(Counts {
        input,
        cached,
        output,
        reasoning,
        total,
    })
}

fn integer(value: &Value) -> Option<i64> {
    if let Some(value) = value.as_i64() {
        return Some(value);
    }
    let float = value.as_f64()?;
    if float.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(&float) {
        Some(float as i64)
    } else {
        None
    }
}

fn timestamp_ms(value: &Value) -> Option<i64> {
    let text = value.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.timestamp_millis())
}
