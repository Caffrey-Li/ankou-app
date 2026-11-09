use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use wanxiang_core::{
    run_collect, Capability, Clock, Guard, GuardError, RefreshGate, Store, Supervisor, TaskStop,
    ToolId, ToolScheduler, UsageError, UsageSnapshot, UsageSource, UsageState,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let raw = std::env::temp_dir().join(format!("wanxiang-m2-link-{}-{}", std::process::id(), n));
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

struct Fixed(std::time::SystemTime);

impl Clock for Fixed {
    fn now(&self) -> std::time::SystemTime {
        self.0
    }
}

fn clock() -> Arc<Fixed> {
    Arc::new(Fixed(
        std::time::UNIX_EPOCH + Duration::from_secs(1_762_700_000),
    ))
}

struct Counting {
    hits: Arc<AtomicUsize>,
    start: Option<Arc<Barrier>>,
    release: Option<Arc<Barrier>>,
    snapshot: UsageSnapshot,
}

impl UsageSource for Counting {
    fn tool(&self) -> ToolId {
        ToolId::DeepSeekHarness
    }
    fn capability(&self) -> Capability {
        Capability::AccountLink
    }
    fn metric(&self) -> &'static str {
        "balance"
    }
    fn collect(&self, _stop: &TaskStop) -> Result<UsageSnapshot, UsageError> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        if let Some(start) = &self.start {
            start.wait();
        }
        if let Some(release) = &self.release {
            release.wait();
        }
        Ok(self.snapshot.clone())
    }
}

fn opened() -> (Tmp, Store, Arc<RefreshGate>, Supervisor) {
    let tmp = Tmp::new();
    let supervisor = Supervisor::new();
    let gate = Arc::new(RefreshGate::new(supervisor.handle()));
    let guard = Arc::new(Guard::new(gate.clone() as Arc<dyn ToolScheduler>, clock()));
    let store = Store::open(&tmp.path.join("wanxiang.sqlite"), guard, clock()).unwrap();
    (tmp, store, gate, supervisor)
}

#[test]
fn open_read_rejects_account_link() {
    let (_tmp, store, _gate, _supervisor) = opened();
    let file = _tmp.path.join("note.txt");
    fs::write(&file, b"fixture").unwrap();
    store.grant_account_link().unwrap();
    assert!(store.guard().account_link_active(ToolId::DeepSeekHarness));
    assert!(store.guard().account_link_active(ToolId::DsBot));
    assert!(!store.guard().account_link_active(ToolId::Cursor));
    let err = store
        .guard()
        .open_read(ToolId::DeepSeekHarness, Capability::AccountLink, &file)
        .unwrap_err();
    assert_eq!(err, GuardError::NotGranted);
    let text = format!("{err} {err:?}");
    assert!(!text.contains("note.txt"));
    assert!(!text.contains(&file.display().to_string()));
}

#[test]
fn revoked_account_link_does_not_send_the_balance_request() {
    let (_tmp, store, gate, _supervisor) = opened();
    store.grant_account_link().unwrap();
    store.revoke_account_link().unwrap();
    assert_eq!(store.account_link_status().unwrap().as_deref(), Some("revoked"));
    assert!(!store.guard().account_link_active(ToolId::DeepSeekHarness));
    let hits = Arc::new(AtomicUsize::new(0));
    let source = Counting {
        hits: Arc::clone(&hits),
        start: None,
        release: None,
        snapshot: UsageSnapshot::empty(),
    };
    run_collect(&store, &gate, &TaskStop::running(), &source).unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[test]
fn revoked_during_balance_request_does_not_persist() {
    let (_tmp, store, gate, _supervisor) = opened();
    store.grant_account_link().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let mut snapshot = UsageSnapshot::empty();
    snapshot.balances.push(wanxiang_core::BalanceAmount {
        currency: wanxiang_core::Currency::Cny,
        total_minor: 1234,
        granted_minor: 234,
        topped_up_minor: 1000,
    });
    let source = Counting {
        hits: Arc::clone(&hits),
        start: Some(Arc::clone(&start)),
        release: Some(Arc::clone(&release)),
        snapshot,
    };
    let store = Arc::new(store);
    let worker_store = Arc::clone(&store);
    let worker_gate = Arc::clone(&gate);
    let handle = thread::spawn(move || {
        run_collect(&worker_store, &worker_gate, &TaskStop::running(), &source)
    });
    start.wait();
    store.revoke_account_link().unwrap();
    release.wait();
    handle.join().unwrap().unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .daily_amount("2026-11-09", ToolId::DeepSeekHarness, "balance_cny_fen")
            .unwrap(),
        None
    );
    let rows = store.usage_rows().unwrap();
    assert!(rows.iter().all(|row| row.3 != 0 || row.2 != "balance_cny_fen"));
    assert!(rows.is_empty());
}

#[test]
fn failed_refresh_keeps_the_previous_amount() {
    let (_tmp, store, _gate, _supervisor) = opened();
    let mut snapshot = UsageSnapshot::empty();
    snapshot.tokens.push(("2026-11-09".to_string(), 300));
    store
        .write_snapshot(ToolId::DsBot, "tokens", &snapshot)
        .unwrap();
    store
        .mark_usage(ToolId::DsBot, "tokens", UsageState::Network)
        .unwrap();
    assert_eq!(
        store
            .daily_amount("2026-11-09", ToolId::DsBot, "tokens")
            .unwrap(),
        Some(300)
    );
    assert_eq!(
        store.usage_state(ToolId::DsBot, "tokens").unwrap(),
        Some(UsageState::Network)
    );
    assert!(store
        .usage_rows()
        .unwrap()
        .iter()
        .all(|row| row.3 != 0));
}

#[test]
fn account_link_is_one_row_for_both_tools() {
    let (tmp, store, _gate, _supervisor) = opened();
    store.grant_account_link().unwrap();
    store.grant_account_link().unwrap();
    let conn = rusqlite::Connection::open(tmp.path.join("wanxiang.sqlite")).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM account_links", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let capability: String = conn
        .query_row("SELECT capability FROM account_links", [], |row| row.get(0))
        .unwrap();
    assert_eq!(capability, "account_link");
    store.revoke_account_link().unwrap();
    assert!(!store.guard().account_link_active(ToolId::DsBot));
}
