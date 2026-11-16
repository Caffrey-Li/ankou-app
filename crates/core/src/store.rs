use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

use crate::{
    calendar_cutoff, local_calendar_day, system_offset_secs, unix_secs, Capability, Clock, GrantId,
    Guard, GuardError, PathScope, RootStamp, ToolId, UsageSnapshot, UsageState, WindowSample,
};

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
    updated_at TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (day, tool, metric)
);
CREATE TABLE IF NOT EXISTS usage_report (
    tool TEXT NOT NULL,
    metric TEXT NOT NULL,
    state TEXT NOT NULL,
    reported_at TEXT NOT NULL,
    PRIMARY KEY (tool, metric)
);
CREATE TABLE IF NOT EXISTS account_links (
    tool TEXT PRIMARY KEY NOT NULL,
    capability TEXT NOT NULL,
    status TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS cleanup_history (
    id INTEGER PRIMARY KEY,
    created_at TEXT NOT NULL,
    tool TEXT NOT NULL,
    entry_count INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS codex_file (
    path TEXT PRIMARY KEY NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    samples INTEGER NOT NULL,
    partial INTEGER NOT NULL,
    limits_at INTEGER NOT NULL,
    w0_minutes INTEGER NOT NULL,
    w0_percent INTEGER NOT NULL,
    w0_resets INTEGER NOT NULL,
    w1_minutes INTEGER NOT NULL,
    w1_percent INTEGER NOT NULL,
    w1_resets INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS codex_file_day (
    path TEXT NOT NULL,
    day TEXT NOT NULL,
    tokens INTEGER NOT NULL,
    PRIMARY KEY (path, day)
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
    #[serde(default)]
    pub codex_app_server: bool,
    #[serde(default)]
    pub cursor_deeplink: bool,
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
            codex_app_server: false,
            cursor_deeplink: false,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexFile {
    pub samples: i64,
    pub partial: bool,
    pub days: Vec<(String, i64)>,
    pub limits_at: i64,
    pub windows: Vec<(i64, i64, i64)>,
}

pub struct Store {
    conn: Mutex<Connection>,
    guard: Arc<Guard>,
    clock: Arc<dyn Clock>,
    live: Mutex<HashMap<i64, GrantId>>,
    offset_secs: i32,
}

impl Store {
    pub fn open(path: &Path, guard: Arc<Guard>, clock: Arc<dyn Clock>) -> Result<Self, StoreError> {
        let offset = system_offset_secs(clock.now());
        Self::open_with_offset(path, guard, clock, offset)
    }

    pub fn open_with_offset(
        path: &Path,
        guard: Arc<Guard>,
        clock: Arc<dyn Clock>,
        offset_secs: i32,
    ) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(|_| StoreError::Database)?;
        conn.execute_batch(SCHEMA)
            .map_err(|_| StoreError::Database)?;
        migrate(&conn)?;
        let store = Self {
            conn: Mutex::new(conn),
            guard,
            clock,
            live: Mutex::new(HashMap::new()),
            offset_secs,
        };
        store.restore_account_link()?;
        store.restore_all()?;
        store.apply_retention()?;
        Ok(store)
    }

    pub fn local_today(&self) -> String {
        local_calendar_day(self.clock.now(), self.offset_secs)
    }

    pub fn offset_secs(&self) -> i32 {
        self.offset_secs
    }

    pub fn now_unix(&self) -> i64 {
        unix_secs(self.clock.now())
    }

    pub fn guard(&self) -> &Guard {
        &self.guard
    }

    pub fn guard_handle(&self) -> Arc<Guard> {
        Arc::clone(&self.guard)
    }

    pub fn settings(&self) -> Result<Settings, StoreError> {
        let conn = self.conn.lock().expect("settings");
        Ok(read_settings(&conn)?)
    }

    /// 是否已保存 DeepSeek Key 的本地记号。读这个不碰钥匙串，避免启动时弹出钥匙串确认。
    pub fn key_saved_flag(&self) -> Result<Option<bool>, StoreError> {
        let conn = self.conn.lock().expect("key flag");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'meta.deepseek_key_saved'",
                [],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(StoreError::Database),
            })?;
        Ok(value.map(|value| value == "1"))
    }

    pub fn set_key_saved_flag(&self, saved: bool) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("key flag");
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('meta.deepseek_key_saved', ?1)",
            params![if saved { "1" } else { "0" }],
        )
        .map_err(|_| StoreError::Database)?;
        Ok(())
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

    pub fn cleanup_history(&self) -> Result<Vec<(String, u64, u64)>, StoreError> {
        let conn = self.conn.lock().expect("cleanup history");
        let mut stmt = conn
            .prepare("SELECT tool, entry_count, total_bytes FROM cleanup_history ORDER BY id")
            .map_err(|_| StoreError::Database)?;
        let rows = stmt
            .query_map([], |row| {
                let count: i64 = row.get(1)?;
                let bytes: i64 = row.get(2)?;
                Ok((row.get(0)?, count as u64, bytes as u64))
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn record_cleanup(
        &self,
        tool: ToolId,
        entry_count: u64,
        total_bytes: u64,
    ) -> Result<(), StoreError> {
        let text = unix_secs(self.clock.now()).to_string();
        let conn = self.conn.lock().expect("cleanup");
        conn.execute(
            "INSERT INTO cleanup_history (created_at, tool, entry_count, total_bytes) VALUES (?1, ?2, ?3, ?4)",
            params![text, enum_name(tool), entry_count as i64, total_bytes as i64],
        )
        .map(|_| ())
        .map_err(|_| StoreError::Database)
    }

    pub fn clear_history(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("history");
        conn.execute_batch(
            "DELETE FROM usage_daily; DELETE FROM usage_report; DELETE FROM cleanup_history; DELETE FROM codex_file; DELETE FROM codex_file_day;",
        )
        .map_err(|_| StoreError::Database)
    }

    pub fn apply_retention(&self) -> Result<(), StoreError> {
        let settings = self.settings()?;
        let today = self.local_today();
        let cutoff =
            calendar_cutoff(&today, settings.retention_months).ok_or(StoreError::InvalidSetting)?;
        let conn = self.conn.lock().expect("retention");
        conn.execute("DELETE FROM usage_daily WHERE day < ?1", [cutoff])
            .map_err(|_| StoreError::Database)?;
        Ok(())
    }

    pub fn grant_account_link(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("account");
        conn.execute(
            "INSERT INTO account_links (tool, capability, status) VALUES ('deepseek_harness', 'account_link', 'active')
             ON CONFLICT(tool) DO UPDATE SET capability = 'account_link', status = 'active'",
            [],
        )
        .map_err(|_| StoreError::Database)?;
        drop(conn);
        self.guard.set_account_link(true);
        Ok(())
    }

    pub fn revoke_account_link(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("account");
        conn.execute(
            "INSERT INTO account_links (tool, capability, status) VALUES ('deepseek_harness', 'account_link', 'revoked')
             ON CONFLICT(tool) DO UPDATE SET status = 'revoked'",
            [],
        )
        .map_err(|_| StoreError::Database)?;
        drop(conn);
        self.guard.revoke_account_link();
        Ok(())
    }

    pub fn account_link_status(&self) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("account");
        let mut stmt = conn
            .prepare("SELECT status FROM account_links WHERE tool = 'deepseek_harness'")
            .map_err(|_| StoreError::Database)?;
        let mut rows = stmt.query([]).map_err(|_| StoreError::Database)?;
        match rows.next().map_err(|_| StoreError::Database)? {
            Some(row) => Ok(Some(row.get(0).map_err(|_| StoreError::Database)?)),
            None => Ok(None),
        }
    }

    pub fn write_snapshot(
        &self,
        tool: ToolId,
        metric: &str,
        snapshot: &UsageSnapshot,
    ) -> Result<(), StoreError> {
        let today = self.local_today();
        let cutoff = calendar_cutoff(&today, self.settings()?.retention_months)
            .ok_or(StoreError::InvalidSetting)?;
        let stamp = unix_secs(self.clock.now()).to_string();
        let conn = self.conn.lock().expect("usage");
        for (day, amount) in &snapshot.tokens {
            upsert_daily(&conn, day, tool, "tokens", *amount, &stamp, &cutoff)?;
        }
        for (day, amount) in &snapshot.calls {
            upsert_daily(&conn, day, tool, "calls", *amount, &stamp, &cutoff)?;
        }
        for balance in &snapshot.balances {
            upsert_daily(
                &conn,
                &today,
                tool,
                balance.currency.metric(),
                balance.total_minor,
                &stamp,
                &cutoff,
            )?;
        }
        for (index, window) in snapshot.windows.iter().enumerate() {
            let name = window_metric(index, window);
            upsert_daily(
                &conn,
                &today,
                tool,
                &name,
                i64::from(window.used_percent) * 100,
                &stamp,
                &cutoff,
            )?;
            if let Some(resets) = window.resets_at {
                upsert_daily(
                    &conn,
                    &today,
                    tool,
                    &format!("{name}_reset"),
                    resets,
                    &stamp,
                    &cutoff,
                )?;
            }
        }
        for (day, amount) in &snapshot.excluded_call_days {
            upsert_daily(&conn, day, tool, "excluded_calls", *amount, &stamp, &cutoff)?;
        }
        if snapshot.excluded_call_days.is_empty() && snapshot.excluded_calls > 0 {
            upsert_daily(
                &conn,
                &today,
                tool,
                "excluded_calls",
                i64::try_from(snapshot.excluded_calls).unwrap_or(i64::MAX),
                &stamp,
                &cutoff,
            )?;
        }
        if snapshot.manage_session_counts {
            put_count(
                &conn,
                &today,
                tool,
                "excluded_sessions",
                i64::try_from(snapshot.excluded_sessions).unwrap_or(i64::MAX),
                &stamp,
                &cutoff,
            )?;
            put_count(
                &conn,
                &today,
                tool,
                "partial_sessions",
                i64::try_from(snapshot.partial_sessions).unwrap_or(i64::MAX),
                &stamp,
                &cutoff,
            )?;
        } else if snapshot.excluded_sessions > 0 {
            upsert_daily(
                &conn,
                &today,
                tool,
                "excluded_sessions",
                i64::try_from(snapshot.excluded_sessions).unwrap_or(i64::MAX),
                &stamp,
                &cutoff,
            )?;
        }
        if !snapshot.windows.is_empty() {
            upsert_daily(
                &conn,
                &today,
                tool,
                "window_source",
                if snapshot.window_live { 1 } else { 2 },
                &stamp,
                &cutoff,
            )?;
        }
        set_report(&conn, tool, metric, UsageState::Ok, &stamp)?;
        Ok(())
    }

    pub fn mark_usage(
        &self,
        tool: ToolId,
        metric: &str,
        state: UsageState,
    ) -> Result<(), StoreError> {
        let stamp = unix_secs(self.clock.now()).to_string();
        let conn = self.conn.lock().expect("report");
        set_report(&conn, tool, metric, state, &stamp)
    }

    pub fn daily_amount(
        &self,
        day: &str,
        tool: ToolId,
        metric: &str,
    ) -> Result<Option<i64>, StoreError> {
        let conn = self.conn.lock().expect("daily");
        let mut stmt = conn
            .prepare("SELECT amount FROM usage_daily WHERE day = ?1 AND tool = ?2 AND metric = ?3")
            .map_err(|_| StoreError::Database)?;
        let mut rows = stmt
            .query(params![day, enum_name(tool), metric])
            .map_err(|_| StoreError::Database)?;
        match rows.next().map_err(|_| StoreError::Database)? {
            Some(row) => Ok(Some(row.get(0).map_err(|_| StoreError::Database)?)),
            None => Ok(None),
        }
    }

    pub fn usage_rows(&self) -> Result<Vec<(String, String, String, i64)>, StoreError> {
        let conn = self.conn.lock().expect("rows");
        let mut stmt = conn
            .prepare("SELECT day, tool, metric, amount FROM usage_daily ORDER BY day, tool, metric")
            .map_err(|_| StoreError::Database)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn usage_reports(&self) -> Result<Vec<(String, String, String, String)>, StoreError> {
        let conn = self.conn.lock().expect("reports");
        let mut stmt = conn
            .prepare(
                "SELECT tool, metric, state, reported_at FROM usage_report ORDER BY tool, metric",
            )
            .map_err(|_| StoreError::Database)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn usage_state(
        &self,
        tool: ToolId,
        metric: &str,
    ) -> Result<Option<UsageState>, StoreError> {
        let conn = self.conn.lock().expect("state");
        let mut stmt = conn
            .prepare("SELECT state FROM usage_report WHERE tool = ?1 AND metric = ?2")
            .map_err(|_| StoreError::Database)?;
        let mut rows = stmt
            .query(params![enum_name(tool), metric])
            .map_err(|_| StoreError::Database)?;
        match rows.next().map_err(|_| StoreError::Database)? {
            Some(row) => {
                let text: String = row.get(0).map_err(|_| StoreError::Database)?;
                Ok(UsageState::parse(&text))
            }
            None => Ok(None),
        }
    }

    pub fn codex_cached(
        &self,
        path: &str,
        size: i64,
        mtime: i64,
    ) -> Result<Option<CodexFile>, StoreError> {
        let conn = self.conn.lock().expect("codex cache");
        let mut stmt = conn
            .prepare(
                "SELECT samples, partial, limits_at, w0_minutes, w0_percent, w0_resets, w1_minutes, w1_percent, w1_resets
                 FROM codex_file WHERE path = ?1 AND size = ?2 AND mtime = ?3",
            )
            .map_err(|_| StoreError::Database)?;
        let mut rows = stmt
            .query(params![path, size, mtime])
            .map_err(|_| StoreError::Database)?;
        let Some(row) = rows.next().map_err(|_| StoreError::Database)? else {
            return Ok(None);
        };
        let samples = row.get(0).map_err(|_| StoreError::Database)?;
        let partial = row.get::<_, i64>(1).map_err(|_| StoreError::Database)? != 0;
        let limits_at = row.get(2).map_err(|_| StoreError::Database)?;
        let windows = windows_from_row(&row)?;
        drop(rows);
        drop(stmt);
        let mut days_stmt = conn
            .prepare("SELECT day, tokens FROM codex_file_day WHERE path = ?1 ORDER BY day")
            .map_err(|_| StoreError::Database)?;
        let days = days_stmt
            .query_map(params![path], |day| Ok((day.get(0)?, day.get(1)?)))
            .map_err(|_| StoreError::Database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)?;
        Ok(Some(CodexFile {
            samples,
            partial,
            days,
            limits_at,
            windows,
        }))
    }

    pub fn save_codex_file(
        &self,
        path: &str,
        size: i64,
        mtime: i64,
        file: &CodexFile,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("codex save");
        let (w0, w1) = windows_to_row(&file.windows);
        conn.execute(
            "INSERT INTO codex_file (
                path, size, mtime, samples, partial, limits_at,
                w0_minutes, w0_percent, w0_resets, w1_minutes, w1_percent, w1_resets
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(path) DO UPDATE SET
                size = excluded.size, mtime = excluded.mtime, samples = excluded.samples,
                partial = excluded.partial, limits_at = excluded.limits_at,
                w0_minutes = excluded.w0_minutes, w0_percent = excluded.w0_percent, w0_resets = excluded.w0_resets,
                w1_minutes = excluded.w1_minutes, w1_percent = excluded.w1_percent, w1_resets = excluded.w1_resets",
            params![
                path,
                size,
                mtime,
                file.samples,
                if file.partial { 1 } else { 0 },
                file.limits_at,
                w0.0,
                w0.1,
                w0.2,
                w1.0,
                w1.1,
                w1.2
            ],
        )
        .map_err(|_| StoreError::Database)?;
        conn.execute("DELETE FROM codex_file_day WHERE path = ?1", params![path])
            .map_err(|_| StoreError::Database)?;
        for (day, tokens) in &file.days {
            conn.execute(
                "INSERT INTO codex_file_day (path, day, tokens) VALUES (?1, ?2, ?3)",
                params![path, day, tokens],
            )
            .map_err(|_| StoreError::Database)?;
        }
        Ok(())
    }

    pub fn forget_codex_files(&self, seen: &[String]) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("codex forget");
        let mut stmt = conn
            .prepare("SELECT path FROM codex_file")
            .map_err(|_| StoreError::Database)?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| StoreError::Database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)?;
        for path in paths {
            if seen.iter().any(|item| item == &path) {
                continue;
            }
            conn.execute("DELETE FROM codex_file WHERE path = ?1", params![path])
                .map_err(|_| StoreError::Database)?;
            conn.execute("DELETE FROM codex_file_day WHERE path = ?1", params![path])
                .map_err(|_| StoreError::Database)?;
        }
        Ok(())
    }

    fn restore_account_link(&self) -> Result<(), StoreError> {
        if self.account_link_status()? == Some("active".to_string()) {
            self.guard.set_account_link(true);
        }
        Ok(())
    }

    fn restore_all(&self) -> Result<(), StoreError> {
        let rows = {
            let conn = self.conn.lock().expect("restore");
            load_rows(&conn)?
        };
        for row in rows {
            // 「需要重新授权」只由这里的启动核对写入（撤销会直接删除记录），所以每次启动
            // 都对有效和待重新授权的记录重新核对：目录没变（路径与 inode 相同）就自动恢复。
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
                Ok((id, stamp)) => {
                    self.live.lock().expect("live").insert(row.id, id);
                    if stamp.dev != row.dev || row.status != GrantStatus::Active {
                        let conn = self.conn.lock().expect("restore");
                        conn.execute(
                            "UPDATE grants SET dev = ?1, status = 'active' WHERE id = ?2",
                            params![stamp.dev.to_string(), row.id],
                        )
                        .map_err(|_| StoreError::Database)?;
                    }
                }
                Err(GuardError::NeedsReauth)
                | Err(GuardError::RootMissing)
                | Err(GuardError::RootNotDirectory) => {
                    if row.status != GrantStatus::NeedsReauth {
                        let conn = self.conn.lock().expect("restore");
                        conn.execute(
                            "UPDATE grants SET status = 'needs_reauth' WHERE id = ?1",
                            [row.id],
                        )
                        .map_err(|_| StoreError::Database)?;
                    }
                }
                // 已经是「需要重新授权」的记录再核对出别的错误，保持原状，不能让整个存储打不开。
                Err(_) if row.status == GrantStatus::NeedsReauth => {}
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
        ("codex_app_server", bit(settings.codex_app_server)),
        ("cursor_deeplink", bit(settings.cursor_deeplink)),
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
        "codex_app_server" => settings.codex_app_server = value == "1",
        "cursor_deeplink" => settings.cursor_deeplink = value == "1",
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

fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('usage_daily') WHERE name = 'updated_at'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StoreError::Database)?;
    if exists == 0 {
        conn.execute_batch(
            "ALTER TABLE usage_daily ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''",
        )
        .map_err(|_| StoreError::Database)?;
    }
    let summary: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('cleanup_history') WHERE name = 'summary'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StoreError::Database)?;
    if summary > 0 {
        conn.execute_batch(
            "CREATE TABLE cleanup_history_next (
                id INTEGER PRIMARY KEY,
                created_at TEXT NOT NULL,
                tool TEXT NOT NULL,
                entry_count INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL
            );
            DROP TABLE cleanup_history;
            ALTER TABLE cleanup_history_next RENAME TO cleanup_history;",
        )
        .map_err(|_| StoreError::Database)?;
    }
    Ok(())
}

fn put_count(
    conn: &Connection,
    day: &str,
    tool: ToolId,
    metric: &str,
    amount: i64,
    stamp: &str,
    cutoff: &str,
) -> Result<(), StoreError> {
    if amount == 0 {
        conn.execute(
            "DELETE FROM usage_daily WHERE tool = ?1 AND metric = ?2",
            params![enum_name(tool), metric],
        )
        .map(|_| ())
        .map_err(|_| StoreError::Database)
    } else {
        upsert_daily(conn, day, tool, metric, amount, stamp, cutoff)
    }
}

fn windows_from_row(row: &rusqlite::Row<'_>) -> Result<Vec<(i64, i64, i64)>, StoreError> {
    let mut windows = Vec::new();
    for base in [3_usize, 6] {
        let minutes: i64 = row.get(base).map_err(|_| StoreError::Database)?;
        if minutes == -2 {
            continue;
        }
        windows.push((
            minutes,
            row.get(base + 1).map_err(|_| StoreError::Database)?,
            row.get(base + 2).map_err(|_| StoreError::Database)?,
        ));
    }
    Ok(windows)
}

fn windows_to_row(windows: &[(i64, i64, i64)]) -> ((i64, i64, i64), (i64, i64, i64)) {
    let blank = (-2, -1, -1);
    (
        windows.first().copied().unwrap_or(blank),
        windows.get(1).copied().unwrap_or(blank),
    )
}

fn upsert_daily(
    conn: &Connection,
    day: &str,
    tool: ToolId,
    metric: &str,
    amount: i64,
    stamp: &str,
    cutoff: &str,
) -> Result<(), StoreError> {
    if day < cutoff {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO usage_daily (day, tool, metric, amount, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(day, tool, metric) DO UPDATE SET amount = excluded.amount, updated_at = excluded.updated_at",
        params![day, enum_name(tool), metric, amount, stamp],
    )
    .map(|_| ())
    .map_err(|_| StoreError::Database)
}

fn set_report(
    conn: &Connection,
    tool: ToolId,
    metric: &str,
    state: UsageState,
    stamp: &str,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO usage_report (tool, metric, state, reported_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(tool, metric) DO UPDATE SET state = excluded.state, reported_at = excluded.reported_at",
        params![enum_name(tool), metric, state.as_str(), stamp],
    )
    .map(|_| ())
    .map_err(|_| StoreError::Database)
}

fn window_metric(index: usize, window: &WindowSample) -> String {
    match window.minutes {
        Some(minutes) => format!("window_{minutes}_{index}_bps"),
        None => format!("window_unknown_{index}_bps"),
    }
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
