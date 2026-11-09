use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use wanxiang_core::UsageError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallRow {
    pub time_ms: i64,
    pub session: String,
    pub model: String,
    pub owner: String,
    pub name: String,
    pub input: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub output: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerKind {
    Bot,
    Group,
    Agent,
    Other,
}

impl OwnerKind {
    pub fn counted(self) -> bool {
        matches!(self, Self::Bot | Self::Group)
    }
}

pub fn owner_kind(owner: &str) -> OwnerKind {
    if owner.starts_with("room:") {
        OwnerKind::Group
    } else if owner.starts_with("agent:") {
        OwnerKind::Agent
    } else if owner.is_empty() || owner == "other" {
        OwnerKind::Other
    } else {
        OwnerKind::Bot
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupTotal {
    pub key: String,
    pub name: String,
    pub kind: OwnerKind,
    pub tokens: i64,
    pub calls: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSummary {
    pub tokens: i64,
    pub calls: i64,
    pub excluded: u64,
    pub groups: Vec<GroupTotal>,
}

pub fn summarize_calls(rows: &[CallRow], since_ms: i64) -> CallSummary {
    let mut tokens = 0;
    let mut calls = 0;
    let mut excluded = 0;
    let mut groups: Vec<GroupTotal> = Vec::new();
    for row in rows {
        if row.time_ms < since_ms {
            continue;
        }
        let kind = owner_kind(&row.owner);
        if !kind.counted() {
            excluded += 1;
            continue;
        }
        let amount = row.input + row.cache_read + row.cache_write + row.output;
        tokens += amount;
        calls += 1;
        if let Some(group) = groups.iter_mut().find(|group| group.key == row.owner) {
            group.tokens += amount;
            group.calls += 1;
            if group.name.is_empty() {
                group.name = row.name.clone();
            }
        } else {
            groups.push(GroupTotal {
                key: row.owner.clone(),
                name: row.name.clone(),
                kind,
                tokens: amount,
                calls: 1,
            });
        }
    }
    CallSummary {
        tokens,
        calls,
        excluded,
        groups,
    }
}

pub fn usage_db_candidates(root: &Path) -> Vec<PathBuf> {
    ["bot/usage.sqlite", "bot/usage.sqlite-wal", "bot/usage.sqlite-shm"]
        .into_iter()
        .map(|name| root.join(name))
        .filter(|path| path.is_file())
        .collect()
}

pub fn daily_counts(rows: &[CallRow], offset_secs: i32) -> Vec<DayCount> {
    use std::collections::BTreeMap;
    use std::time::{Duration, UNIX_EPOCH};
    let mut days: BTreeMap<String, DayCount> = BTreeMap::new();
    for row in rows {
        let instant = UNIX_EPOCH + Duration::from_millis(row.time_ms.max(0) as u64);
        let day = wanxiang_core::local_calendar_day(instant, offset_secs);
        let bucket = days.entry(day.clone()).or_insert_with(|| DayCount {
            day,
            tokens: 0,
            calls: 0,
            excluded: 0,
        });
        if !owner_kind(&row.owner).counted() {
            bucket.excluded += 1;
            continue;
        }
        bucket.tokens += row.input + row.cache_read + row.cache_write + row.output;
        bucket.calls += 1;
    }
    days.into_values().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayCount {
    pub day: String,
    pub tokens: i64,
    pub calls: i64,
    pub excluded: u64,
}

pub fn read_usage_set(files: &[PathBuf]) -> Result<Vec<CallRow>, UsageError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "wanxiang-dsh-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        fs::create_dir_all(&dir).map_err(|_| UsageError::Unreadable)?;
        for file in files {
            let name = file.file_name().ok_or(UsageError::Unreadable)?;
            fs::copy(file, dir.join(name)).map_err(|_| UsageError::Unreadable)?;
        }
        read_usage_db(&dir.join("usage.sqlite"))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

pub fn read_usage_copy(source: &Path) -> Result<Vec<CallRow>, UsageError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = std::env::temp_dir().join(format!(
        "wanxiang-dsh-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        fs::copy(source, &temp).map_err(|_| UsageError::Unreadable)?;
        read_usage_db(&temp)
    })();
    let _ = fs::remove_file(&temp);
    result
}

fn read_usage_db(path: &Path) -> Result<Vec<CallRow>, UsageError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| UsageError::Unreadable)?;
    let mut stmt = conn
        .prepare(
            "SELECT c.time, c.session, c.model, COALESCE(o.owner, 'other'), COALESCE(o.name, ''),
                    c.input, c.cache_read, c.cache_write, c.output
             FROM calls c
             LEFT JOIN owners o ON o.session = c.session
             ORDER BY c.time",
        )
        .map_err(|_| UsageError::Unreadable)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CallRow {
                time_ms: row.get(0)?,
                session: row.get(1)?,
                model: row.get(2)?,
                owner: row.get(3)?,
                name: row.get(4)?,
                input: row.get(5)?,
                cache_read: row.get(6)?,
                cache_write: row.get(7)?,
                output: row.get(8)?,
            })
        })
        .map_err(|_| UsageError::Unreadable)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| UsageError::Unreadable)
}
