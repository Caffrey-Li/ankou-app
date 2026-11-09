use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection};

use crate::{Capability, Clock, GrantId, Guard, GuardError, PathScope, RootStamp, ToolId};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS grants (
    id INTEGER PRIMARY KEY,
    tool TEXT NOT NULL,
    capability TEXT NOT NULL,
    root TEXT NOT NULL,
    pattern TEXT NOT NULL,
    dev TEXT NOT NULL,
    ino TEXT NOT NULL,
    status TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS grants_identity
    ON grants(tool, capability, root, pattern);
CREATE TABLE IF NOT EXISTS usage_daily (
    day TEXT NOT NULL,
    tool TEXT NOT NULL,
    metric TEXT NOT NULL,
    amount INTEGER NOT NULL,
    PRIMARY KEY (day, tool, metric)
);
CREATE TABLE IF NOT EXISTS cleanup_history (
    id INTEGER PRIMARY KEY,
    created_at TEXT NOT NULL,
    summary TEXT NOT NULL
);
";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryMode {
    Ask,
    Local,
    Ai,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub retention_months: u32,
    pub launch_at_login: bool,
    pub notifications: bool,
    pub context_threshold_percent: u32,
    pub space_threshold_gb: u32,
    pub refresh_on_launch: bool,
    pub periodic_refresh: bool,
    pub summary_mode: SummaryMode,
    pub menu_bar_icon: bool,
    pub close_to_tray: bool,
    pub setup_complete: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retention_months: 12,
            launch_at_login: false,
            notifications: false,
            context_threshold_percent: 80,
            space_threshold_gb: 5,
            refresh_on_launch: true,
            periodic_refresh: true,
            summary_mode: SummaryMode::Ask,
            menu_bar_icon: true,
            close_to_tray: true,
            setup_complete: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantStatus {
    Active,
    NeedsReauth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPresence {
    Missing,
    Unauthorized,
    Authorized,
    NeedsReauth,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredGrant {
    pub id: i64,
    pub tool: ToolId,
    pub capability: Capability,
    #[serde(skip)]
    pub root: PathBuf,
    pub pattern: String,
    pub status: GrantStatus,
}

#[derive(Debug)]
pub enum StoreError {
    Guard(GuardError),
    Database,
    InvalidSetting,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Guard(err) => write!(f, "{err}"),
            Self::Database => f.write_str("数据库不可用"),
            Self::InvalidSetting => f.write_str("设置无效"),
        }
    }
}

impl std::error::Error for StoreError {}

pub struct Store {
    conn: Mutex<Connection>,
    guard: Arc<Guard>,
    clock: Arc<dyn Clock>,
    live: Mutex<HashMap<i64, GrantId>>,
}

impl Store {
    pub fn open(path: &Path, guard: Arc<Guard>, clock: Arc<dyn Clock>) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(|_| StoreError::Database)?;
        conn.execute_batch(SCHEMA)
            .map_err(|_| StoreError::Database)?;
        let store = Self {
            conn: Mutex::new(conn),
            guard,
            clock,
            live: Mutex::new(HashMap::new()),
        };
        store.restore_all()?;
        store.apply_retention()?;
        Ok(store)
    }

    pub fn guard(&self) -> &Guard {
        &self.guard
    }

    pub fn settings(&self) -> Result<Settings, StoreError> {
        let conn = self.conn.lock().expect("settings");
        Ok(read_settings(&conn)?)
    }

    pub fn set_settings(&self, settings: &Settings) -> Result<(), StoreError> {
        validate(settings)?;
        let conn = self.conn.lock().expect("settings");
        write_settings(&conn, settings)?;
        drop(conn);
        self.apply_retention()
    }

    pub fn grant(
        &self,
        tool: ToolId,
        capability: Capability,
        scope: PathScope,
    ) -> Result<StoredGrant, StoreError> {
        let id = self
            .guard
            .grant(tool, capability, scope.clone())
            .map_err(StoreError::Guard)?;
        let stamp = self.guard.stamp(id).map_err(StoreError::Guard)?;
        let conn = self.conn.lock().expect("grant");
        let row = upsert_grant(&conn, tool, capability, &stamp, &scope.pattern)?;
        self.live.lock().expect("live").insert(row, id);
        Ok(StoredGrant {
            id: row,
            tool,
            capability,
            root: stamp.path,
            pattern: scope.pattern,
            status: GrantStatus::Active,
        })
    }

    pub fn revoke(&self, id: i64) -> Result<(), StoreError> {
        let removed = self.live.lock().expect("live").remove(&id);
        if let Some(grant_id) = removed {
            match self.guard.revoke(grant_id) {
                Ok(()) | Err(GuardError::GrantNotFound) => {}
                Err(err) => return Err(StoreError::Guard(err)),
            }
        }
        let conn = self.conn.lock().expect("revoke");
        let n = conn
            .execute("DELETE FROM grants WHERE id = ?1", [id])
            .map_err(|_| StoreError::Database)?;
        if n == 0 {
            return Err(StoreError::Guard(GuardError::GrantNotFound));
        }
        Ok(())
    }

    pub fn grants(&self) -> Result<Vec<StoredGrant>, StoreError> {
        let conn = self.conn.lock().expect("grants");
        load_grants(&conn)
    }

    pub fn clear_history(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("history");
        conn.execute_batch("DELETE FROM usage_daily; DELETE FROM cleanup_history;")
            .map_err(|_| StoreError::Database)
    }

    pub fn apply_retention(&self) -> Result<(), StoreError> {
        let settings = self.settings()?;
        let today = utc_day(self.clock.now());
        let cutoff = cutoff_day(&today, settings.retention_months)?;
        let conn = self.conn.lock().expect("retention");
        conn.execute("DELETE FROM usage_daily WHERE day < ?1", [cutoff])
            .map_err(|_| StoreError::Database)?;
        Ok(())
    }

    fn restore_all(&self) -> Result<(), StoreError> {
        let rows = {
            let conn = self.conn.lock().expect("restore");
            load_rows(&conn)?
        };
        for row in rows {
            if row.status != GrantStatus::Active {
                continue;
            }
            let expected = RootStamp {
                path: row.root.clone(),
                dev: row.dev,
                ino: row.ino,
            };
            match self.guard.restore(
                row.tool,
                row.capability,
                PathScope {
                    root: row.root.clone(),
                    pattern: row.pattern.clone(),
                },
                &expected,
            ) {
                Ok(id) => {
                    self.live.lock().expect("live").insert(row.id, id);
                }
                Err(GuardError::NeedsReauth)
                | Err(GuardError::RootMissing)
                | Err(GuardError::RootNotDirectory) => {
                    let conn = self.conn.lock().expect("restore");
                    conn.execute(
                        "UPDATE grants SET status = 'needs_reauth' WHERE id = ?1",
                        [row.id],
                    )
                    .map_err(|_| StoreError::Database)?;
                }
                Err(err) => return Err(StoreError::Guard(err)),
            }
        }
        Ok(())
    }
}

pub fn presence(installed: bool, statuses: &[GrantStatus]) -> ToolPresence {
    if !installed {
        return ToolPresence::Missing;
    }
    if statuses.contains(&GrantStatus::NeedsReauth) {
        return ToolPresence::NeedsReauth;
    }
    if statuses.contains(&GrantStatus::Active) {
        return ToolPresence::Authorized;
    }
    ToolPresence::Unauthorized
}

struct Row {
    id: i64,
    tool: ToolId,
    capability: Capability,
    root: PathBuf,
    pattern: String,
    dev: u64,
    ino: u64,
    status: GrantStatus,
}

fn load_rows(conn: &Connection) -> Result<Vec<Row>, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, tool, capability, root, pattern, dev, ino, status FROM grants ORDER BY id",
        )
        .map_err(|_| StoreError::Database)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|_| StoreError::Database)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, tool, capability, root, pattern, dev, ino, status) =
            row.map_err(|_| StoreError::Database)?;
        out.push(Row {
            id,
            tool: parse_enum(&tool)?,
            capability: parse_enum(&capability)?,
            root: PathBuf::from(root),
            pattern,
            dev: dev.parse().map_err(|_| StoreError::Database)?,
            ino: ino.parse().map_err(|_| StoreError::Database)?,
            status: parse_enum(&status)?,
        });
    }
    Ok(out)
}

fn load_grants(conn: &Connection) -> Result<Vec<StoredGrant>, StoreError> {
    Ok(load_rows(conn)?
        .into_iter()
        .map(|row| StoredGrant {
            id: row.id,
            tool: row.tool,
            capability: row.capability,
            root: row.root,
            pattern: row.pattern,
            status: row.status,
        })
        .collect())
}

fn upsert_grant(
    conn: &Connection,
    tool: ToolId,
    capability: Capability,
    stamp: &RootStamp,
    pattern: &str,
) -> Result<i64, StoreError> {
    conn.execute(
        "INSERT INTO grants (tool, capability, root, pattern, dev, ino, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active')
         ON CONFLICT(tool, capability, root, pattern) DO UPDATE SET
            dev = excluded.dev,
            ino = excluded.ino,
            status = 'active'",
        params![
            enum_name(tool),
            enum_name(capability),
            stamp.path.to_string_lossy(),
            pattern,
            stamp.dev.to_string(),
            stamp.ino.to_string(),
        ],
    )
    .map_err(|_| StoreError::Database)?;
    conn.query_row(
        "SELECT id FROM grants WHERE tool = ?1 AND capability = ?2 AND root = ?3 AND pattern = ?4",
        params![
            enum_name(tool),
            enum_name(capability),
            stamp.path.to_string_lossy(),
            pattern,
        ],
        |row| row.get(0),
    )
    .map_err(|_| StoreError::Database)
}

fn read_settings(conn: &Connection) -> Result<Settings, StoreError> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings")
        .map_err(|_| StoreError::Database)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| StoreError::Database)?;
    let mut settings = Settings::default();
    for row in rows {
        let (key, value) = row.map_err(|_| StoreError::Database)?;
        apply_setting(&mut settings, &key, &value);
    }
    Ok(settings)
}

fn write_settings(conn: &Connection, settings: &Settings) -> Result<(), StoreError> {
    let pairs = [
        ("retention_months", settings.retention_months.to_string()),
        ("launch_at_login", bit(settings.launch_at_login)),
        ("notifications", bit(settings.notifications)),
        (
            "context_threshold_percent",
            settings.context_threshold_percent.to_string(),
        ),
        (
            "space_threshold_gb",
            settings.space_threshold_gb.to_string(),
        ),
        ("refresh_on_launch", bit(settings.refresh_on_launch)),
        ("periodic_refresh", bit(settings.periodic_refresh)),
        ("summary_mode", enum_name(settings.summary_mode)),
        ("menu_bar_icon", bit(settings.menu_bar_icon)),
        ("close_to_tray", bit(settings.close_to_tray)),
        ("setup_complete", bit(settings.setup_complete)),
    ];
    for (key, value) in pairs {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|_| StoreError::Database)?;
    }
    Ok(())
}

fn apply_setting(settings: &mut Settings, key: &str, value: &str) {
    match key {
        "retention_months" => {
            if let Ok(months) = value.parse() {
                settings.retention_months = months;
            }
        }
        "launch_at_login" => settings.launch_at_login = value == "1",
        "notifications" => settings.notifications = value == "1",
        "context_threshold_percent" => {
            if let Ok(value) = value.parse() {
                settings.context_threshold_percent = value;
            }
        }
        "space_threshold_gb" => {
            if let Ok(value) = value.parse() {
                settings.space_threshold_gb = value;
            }
        }
        "refresh_on_launch" => settings.refresh_on_launch = value == "1",
        "periodic_refresh" => settings.periodic_refresh = value == "1",
        "summary_mode" => {
            settings.summary_mode = match value {
                "local" => SummaryMode::Local,
                "ai" => SummaryMode::Ai,
                _ => SummaryMode::Ask,
            }
        }
        "menu_bar_icon" => settings.menu_bar_icon = value == "1",
        "close_to_tray" => settings.close_to_tray = value == "1",
        "setup_complete" => settings.setup_complete = value == "1",
        _ => {}
    }
}

fn validate(settings: &Settings) -> Result<(), StoreError> {
    if !(1..=120).contains(&settings.retention_months) {
        return Err(StoreError::InvalidSetting);
    }
    if !(1..=100).contains(&settings.context_threshold_percent) {
        return Err(StoreError::InvalidSetting);
    }
    if !(1..=10_000).contains(&settings.space_threshold_gb) {
        return Err(StoreError::InvalidSetting);
    }
    Ok(())
}

fn cutoff_day(today: &str, months: u32) -> Result<String, StoreError> {
    let date =
        NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|_| StoreError::InvalidSetting)?;
    let months_i = i32::try_from(months).map_err(|_| StoreError::InvalidSetting)?;
    let mut month = date.month() as i32 - months_i;
    let mut year = date.year();
    while month <= 0 {
        month += 12;
        year -= 1;
    }
    let day = date.day().min(days_in_month(year, month as u32));
    let cutoff =
        NaiveDate::from_ymd_opt(year, month as u32, day).ok_or(StoreError::InvalidSetting)?;
    Ok(cutoff.format("%Y-%m-%d").to_string())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next.0, next.1, 1)
        .and_then(|date| date.pred_opt())
        .map(|date| date.day())
        .unwrap_or(28)
}

fn utc_day(now: std::time::SystemTime) -> String {
    Utc.timestamp_opt(
        now.duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        0,
    )
    .single()
    .map(|time| time.format("%Y-%m-%d").to_string())
    .unwrap_or_else(|| "1970-01-01".to_string())
}

fn bit(value: bool) -> String {
    if value { "1" } else { "0" }.to_string()
}

fn enum_name<T: serde::Serialize>(value: T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => panic!("枚举应序列化成字符串"),
    }
}

fn parse_enum<T: serde::de::DeserializeOwned>(name: &str) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(name.to_string()))
        .map_err(|_| StoreError::Database)
}
