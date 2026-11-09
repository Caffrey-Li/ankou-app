use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use wanxiang_core::{
    presence, Capability, Clock, GrantStatus, Guard, PathScope, Settings, Store, StoreError,
    ToolId, ToolPresence, ToolScheduler,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let raw = std::env::temp_dir().join(format!("wanxiang-m1b-{}-{}", std::process::id(), n));
        fs::create_dir_all(&raw).unwrap();
        Self {
            path: fs::canonicalize(&raw).unwrap(),
        }
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Rec;

impl ToolScheduler for Rec {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

struct FixedClock(SystemTime);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

fn clock_at(day: &str) -> Arc<FixedClock> {
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
    let time = date.and_hms_opt(12, 0, 0).unwrap().and_utc();
    Arc::new(FixedClock(
        SystemTime::UNIX_EPOCH + Duration::from_secs(time.timestamp() as u64),
    ))
}

fn guard() -> Arc<Guard> {
    Arc::new(Guard::new(Arc::new(Rec), clock_at("2026-11-09")))
}

fn open(tmp: &Tmp, guard: Arc<Guard>) -> Store {
    Store::open(
        &tmp.path.join("wanxiang.sqlite"),
        guard,
        clock_at("2026-11-09"),
    )
    .unwrap()
}

fn root(tmp: &Tmp, name: &str) -> PathBuf {
    let path = tmp.path.join(name);
    fs::create_dir_all(&path).unwrap();
    fs::canonicalize(&path).unwrap()
}

fn grant_usage(store: &Store, root: &std::path::Path) -> i64 {
    store
        .grant(
            ToolId::Cursor,
            Capability::Usage,
            PathScope {
                root: root.to_path_buf(),
                pattern: "**".to_string(),
            },
        )
        .unwrap()
        .id
}

fn insert_usage(path: &std::path::Path, day: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "INSERT INTO usage_daily (day, tool, metric, amount) VALUES (?1, 'cursor', 'calls', 1)",
        [day],
    )
    .unwrap();
}

fn insert_cleanup(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "INSERT INTO cleanup_history (created_at, summary) VALUES ('2026-11-01', 'fixture')",
        [],
    )
    .unwrap();
}

fn count(path: &std::path::Path, table: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn grant_survives_a_restart() {
    let tmp = Tmp::new();
    let data = root(&tmp, "data");
    fs::write(data.join("note.txt"), b"fixture").unwrap();
    let db = tmp.path.join("wanxiang.sqlite");
    {
        let store = open(&tmp, guard());
        grant_usage(&store, &data);
    }
    let store = open(&tmp, guard());
    let grants = store.grants().unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].status, GrantStatus::Active);
    assert_eq!(store.guard().grants().len(), 1);
    let mut read = store
        .guard()
        .open_read(ToolId::Cursor, Capability::Usage, &data.join("note.txt"))
        .unwrap();
    let mut buf = [0u8; 8];
    let n = read.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"fixture");
    assert!(db.is_file());
}

#[test]
fn replaced_root_needs_reauth_and_is_not_granted() {
    let tmp = Tmp::new();
    let data = root(&tmp, "data");
    fs::write(data.join("note.txt"), b"fixture").unwrap();
    {
        let store = open(&tmp, guard());
        grant_usage(&store, &data);
    }
    let moved = tmp.path.join("data-old");
    fs::rename(&data, &moved).unwrap();
    fs::create_dir(&data).unwrap();
    fs::write(data.join("note.txt"), b"other").unwrap();
    let store = open(&tmp, guard());
    let grants = store.grants().unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].status, GrantStatus::NeedsReauth);
    assert!(store.guard().grants().is_empty());
    let err = store
        .guard()
        .open_read(ToolId::Cursor, Capability::Usage, &data.join("note.txt"))
        .unwrap_err();
    assert_ne!(format!("{err}"), "");
    let again = open(&tmp, guard());
    assert_eq!(again.grants().unwrap()[0].status, GrantStatus::NeedsReauth);
    assert!(again.guard().grants().is_empty());
    let text = format!(
        "{}",
        StoreError::Guard(wanxiang_core::GuardError::NeedsReauth)
    );
    assert!(!text.contains("wanxiang-m1b"));
    assert!(!text.contains(&tmp.path.display().to_string()));
}

#[test]
fn clear_history_leaves_settings_and_grants() {
    let tmp = Tmp::new();
    let data = root(&tmp, "data");
    let db = tmp.path.join("wanxiang.sqlite");
    let store = open(&tmp, guard());
    grant_usage(&store, &data);
    let mut settings = store.settings().unwrap();
    settings.retention_months = 6;
    store.set_settings(&settings).unwrap();
    insert_usage(&db, "2026-11-01");
    insert_cleanup(&db);
    store.clear_history().unwrap();
    assert_eq!(count(&db, "usage_daily"), 0);
    assert_eq!(count(&db, "cleanup_history"), 0);
    assert_eq!(store.grants().unwrap().len(), 1);
    assert_eq!(store.settings().unwrap().retention_months, 6);
}

#[test]
fn retention_drops_days_older_than_the_window() {
    let tmp = Tmp::new();
    let db = tmp.path.join("wanxiang.sqlite");
    let store = open(&tmp, guard());
    insert_usage(&db, "2025-11-08");
    insert_usage(&db, "2025-11-09");
    insert_usage(&db, "2026-11-09");
    store.apply_retention().unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut days = conn
        .prepare("SELECT day FROM usage_daily ORDER BY day")
        .unwrap();
    let got: Vec<String> = days
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|d| d.unwrap())
        .collect();
    assert_eq!(
        got,
        vec!["2025-11-09".to_string(), "2026-11-09".to_string()]
    );

    let mut settings = Settings::default();
    settings.retention_months = 1;
    store.set_settings(&settings).unwrap();
    insert_usage(&db, "2026-10-08");
    insert_usage(&db, "2026-10-09");
    store.apply_retention().unwrap();
    let mut days = conn
        .prepare("SELECT day FROM usage_daily ORDER BY day")
        .unwrap();
    let got: Vec<String> = days
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|d| d.unwrap())
        .collect();
    assert_eq!(
        got,
        vec!["2026-10-09".to_string(), "2026-11-09".to_string(),]
    );
}

#[test]
fn retention_cutoff_uses_the_local_day_when_utc_has_moved_ahead() {
    let tmp = Tmp::new();
    let now = wanxiang_core::clock_at_utc(2026, 11, 10, 5, 0, 0);
    assert_eq!(
        wanxiang_core::local_calendar_day(now, -6 * 3600),
        "2026-11-09"
    );
    assert_eq!(wanxiang_core::local_calendar_day(now, 0), "2026-11-10");
    let clock = Arc::new(FixedClock(now));
    let guard = Arc::new(Guard::new(Arc::new(Rec), clock.clone()));
    let store = Store::open_with_offset(
        &tmp.path.join("wanxiang.sqlite"),
        guard,
        clock,
        -6 * 3600,
    )
    .unwrap();
    let db = tmp.path.join("wanxiang.sqlite");
    insert_usage(&db, "2025-11-08");
    insert_usage(&db, "2025-11-09");
    insert_usage(&db, "2025-11-10");
    store.apply_retention().unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut days = conn
        .prepare("SELECT day FROM usage_daily ORDER BY day")
        .unwrap();
    let got: Vec<String> = days
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|day| day.unwrap())
        .collect();
    assert_eq!(
        got,
        vec!["2025-11-09".to_string(), "2025-11-10".to_string()]
    );
}

#[test]
fn default_settings_keep_history_for_twelve_months_and_leave_residency_off() {
    let tmp = Tmp::new();
    let store = open(&tmp, guard());
    let settings = store.settings().unwrap();
    assert_eq!(settings.retention_months, 12);
    assert!(!settings.launch_at_login);
    assert!(!settings.notifications);
    assert!(!settings.setup_complete);
}

#[test]
fn history_tables_do_not_store_conversation_text() {
    let tmp = Tmp::new();
    let db = tmp.path.join("wanxiang.sqlite");
    let _store = open(&tmp, guard());
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut names = Vec::new();
    for table in ["usage_daily", "cleanup_history", "grants", "settings"] {
        let mut stmt = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|c| c.unwrap())
            .collect();
        names.extend(cols);
    }
    let joined = names.join(" ").to_ascii_lowercase();
    for banned in ["body", "content", "message", "prompt", "transcript"] {
        assert!(!joined.contains(banned), "{joined}");
    }
}

#[test]
fn presence_uses_install_and_grant_state() {
    assert_eq!(presence(false, &[]), ToolPresence::Missing);
    assert_eq!(
        presence(true, &[GrantStatus::Active]),
        ToolPresence::Authorized
    );
    assert_eq!(presence(true, &[]), ToolPresence::Unauthorized);
    assert_eq!(
        presence(true, &[GrantStatus::Active, GrantStatus::NeedsReauth]),
        ToolPresence::NeedsReauth
    );
}

#[test]
fn store_error_text_has_no_path() {
    let tmp = Tmp::new();
    let err = match Store::open(
        &tmp.path.join("missing").join("db"),
        guard(),
        clock_at("2026-11-09"),
    ) {
        Err(err) => err,
        Ok(_) => panic!("缺少父目录时应该失败"),
    };
    let text = format!("{err:?} {err}");
    assert!(!text.contains("wanxiang-m1b"));
    assert!(!text.contains(&tmp.path.display().to_string()));
}
