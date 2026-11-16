use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wanxiang_adapter_codex::{find_codex_cli, parse_rollout, query_rate_limits, rollout_paths};
use wanxiang_adapter_dsh::{
    daily_counts, fetch_balance, parse_session, read_session_bytes, read_usage_set, session_paths,
    usage_db_candidates,
};
use wanxiang_core::{
    local_calendar_day, run_collect, wait_interval, Capability, CodexFile, GrantStatus, Guard,
    RefreshGate, SecretError, SecretStore, Store, TaskStop, ToolId, UsageError, UsageSnapshot,
    UsageSource, WindowSample, REFRESH_EVERY,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshKind {
    Launch,
    Manual,
    Periodic,
}

impl RefreshKind {
    fn app_server(self) -> bool {
        !matches!(self, Self::Periodic)
    }
}

pub fn due(now: SystemTime, last: SystemTime) -> bool {
    now.duration_since(last).unwrap_or_default() >= REFRESH_EVERY
}

pub trait ServerProbe: Sync {
    fn start(&self, gate: &RefreshGate) -> Result<Vec<WindowSample>, UsageError>;
}

struct LiveServer;

impl ServerProbe for LiveServer {
    fn start(&self, gate: &RefreshGate) -> Result<Vec<WindowSample>, UsageError> {
        let path = std::env::var("PATH").unwrap_or_default();
        let Some(bin) = find_codex_cli(&path) else {
            return Err(UsageError::AppServer);
        };
        let mut command = Command::new(bin);
        command.arg("app-server");
        let windows = query_rate_limits(&gate.supervisor(), command, |pid| gate.track_codex(pid))?;
        gate.clear_codex();
        Ok(windows)
    }
}

#[cfg(debug_assertions)]
fn refresh_skipped() -> bool {
    std::env::var_os("WANXIANG_SKIP_REFRESH").is_some()
}

#[cfg(not(debug_assertions))]
fn refresh_skipped() -> bool {
    false
}

pub fn spawn_refresh(
    store: Arc<Store>,
    gate: Arc<RefreshGate>,
    emit: Arc<dyn Fn(&str) + Send + Sync>,
) {
    if refresh_skipped() {
        return;
    }
    gate.supervisor().spawn_task(move |stop| {
        let mut marked = SystemTime::now();
        let launch = store
            .settings()
            .map(|settings| settings.refresh_on_launch)
            .unwrap_or(false);
        if launch {
            refresh_blocking(
                &store,
                &gate,
                &stop,
                RefreshKind::Launch,
                &LiveServer,
                &emit,
            );
        }
        while !stop.is_stopped() {
            let periodic = store
                .settings()
                .map(|settings| settings.periodic_refresh)
                .unwrap_or(false);
            if !periodic {
                wait_interval(&stop, Duration::from_secs(1));
                continue;
            }
            wait_interval(&stop, REFRESH_EVERY);
            if stop.is_stopped() || !due(SystemTime::now(), marked) {
                continue;
            }
            marked = SystemTime::now();
            refresh_blocking(
                &store,
                &gate,
                &stop,
                RefreshKind::Periodic,
                &LiveServer,
                &emit,
            );
        }
    });
}

pub fn start_manual(
    store: Arc<Store>,
    gate: Arc<RefreshGate>,
    emit: Arc<dyn Fn(&str) + Send + Sync>,
) -> bool {
    if !gate.try_begin() {
        return false;
    }
    emit("started");
    gate.supervisor().spawn_task(move |stop| {
        let _end = End(Arc::clone(&gate), Arc::clone(&emit));
        refresh_body(
            &store,
            &gate,
            &stop,
            RefreshKind::Manual,
            &LiveServer,
            emit.as_ref(),
        );
    });
    true
}

fn refresh_blocking(
    store: &Store,
    gate: &Arc<RefreshGate>,
    stop: &TaskStop,
    kind: RefreshKind,
    server: &dyn ServerProbe,
    emit: &Arc<dyn Fn(&str) + Send + Sync>,
) {
    if !gate.try_begin() {
        return;
    }
    emit("started");
    let _end = End(Arc::clone(gate), Arc::clone(emit));
    refresh_body(store, gate, stop, kind, server, emit.as_ref());
}

struct End(Arc<RefreshGate>, Arc<dyn Fn(&str) + Send + Sync>);

impl Drop for End {
    fn drop(&mut self) {
        (self.1)("done");
        self.0.end();
    }
}

fn refresh_body(
    store: &Store,
    gate: &RefreshGate,
    stop: &TaskStop,
    kind: RefreshKind,
    server: &dyn ServerProbe,
    emit: &(dyn Fn(&str) + Sync),
) {
    if stop.is_stopped() {
        return;
    }
    let Ok(settings) = store.settings() else {
        return;
    };
    let Ok(grants) = store.grants() else {
        return;
    };
    let active = |tool, capability| {
        grants.iter().any(|grant| {
            grant.tool == tool
                && grant.capability == capability
                && grant.status == GrantStatus::Active
        })
    };
    let want_server =
        kind.app_server() && settings.codex_app_server && active(ToolId::Codex, Capability::Usage);
    let bot = grants.iter().find(|grant| {
        grant.tool == ToolId::DsBot
            && grant.capability == Capability::Usage
            && grant.status == GrantStatus::Active
    });
    let codex = grants.iter().find(|grant| {
        grant.tool == ToolId::Codex
            && grant.capability == Capability::Usage
            && grant.status == GrantStatus::Active
    });
    let harness = grants.iter().find(|grant| {
        grant.tool == ToolId::DeepSeekHarness
            && grant.capability == Capability::Usage
            && grant.status == GrantStatus::Active
    });
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // Harness 与 DS Bot 同属 ~/.dsh，按顺序读，界面上一起显示「刷新中」
            if let Some(root) = harness {
                let _ = run_collect(
                    store,
                    gate,
                    stop,
                    &HarnessSessions {
                        root: root.root.clone(),
                        offset: store.offset_secs(),
                        guard: store.guard_handle(),
                    },
                );
            }
            if let Some(root) = bot {
                let _ = run_collect(
                    store,
                    gate,
                    stop,
                    &DsBotSource {
                        root: root.root.clone(),
                        offset: store.offset_secs(),
                        guard: store.guard_handle(),
                    },
                );
            }
            emit("ds_bot");
        });
        scope.spawn(|| {
            if store.guard().account_link_active(ToolId::DeepSeekHarness) {
                let _ = run_collect(store, gate, stop, &BalanceSource);
            }
            emit("balance");
        });
        scope.spawn(|| {
            // 先读本机记录（含额度窗口），再问 app-server；app-server 失败时保留本机记录里的窗口
            if let Some(root) = codex {
                let _ = run_collect(
                    store,
                    gate,
                    stop,
                    &CodexFiles {
                        root: root.root.clone(),
                        offset: store.offset_secs(),
                        guard: store.guard_handle(),
                        store,
                        write_windows: true,
                    },
                );
            }
            emit("codex");
            if want_server {
                match server.start(gate) {
                    Ok(windows) => {
                        let mut snapshot = UsageSnapshot::empty();
                        snapshot.windows = windows;
                        snapshot.window_live = true;
                        let _ = store.write_snapshot(ToolId::Codex, "windows", &snapshot);
                    }
                    Err(err) => {
                        let _ = store.mark_usage(ToolId::Codex, "windows", err.state());
                    }
                }
            }
            emit("windows");
        });
    });
}

struct DsBotSource {
    root: PathBuf,
    offset: i32,
    guard: Arc<Guard>,
}

impl UsageSource for DsBotSource {
    fn tool(&self) -> ToolId {
        ToolId::DsBot
    }
    fn capability(&self) -> Capability {
        Capability::Usage
    }
    fn metric(&self) -> &'static str {
        "tokens"
    }
    fn collect(&self, stop: &TaskStop) -> Result<UsageSnapshot, UsageError> {
        if stop.is_stopped() {
            return Err(UsageError::Stopped);
        }
        let files: Vec<PathBuf> = readable_paths(&self.root)
            .into_iter()
            .filter(|path| {
                matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("usage.sqlite" | "usage.sqlite-wal" | "usage.sqlite-shm")
                )
            })
            .collect();
        if !files
            .iter()
            .any(|path| path.file_name().and_then(|name| name.to_str()) == Some("usage.sqlite"))
        {
            return Err(UsageError::Unreadable);
        }
        for file in &files {
            admit(&self.guard, ToolId::DsBot, file)?;
        }
        let rows = read_usage_set(&files)?;
        let mut snapshot = UsageSnapshot::empty();
        for day in daily_counts(&rows, self.offset) {
            snapshot.tokens.push((day.day.clone(), day.tokens));
            snapshot.calls.push((day.day.clone(), day.calls));
            if day.excluded > 0 {
                snapshot
                    .excluded_call_days
                    .push((day.day, i64::try_from(day.excluded).unwrap_or(i64::MAX)));
            }
        }
        Ok(snapshot)
    }
}

/// Harness 用量：本机会话文件里每次调用的 token，排除 Bot 工作区的会话（那部分归 DS Bot）。
struct HarnessSessions {
    root: PathBuf,
    offset: i32,
    guard: Arc<Guard>,
}

impl UsageSource for HarnessSessions {
    fn tool(&self) -> ToolId {
        ToolId::DeepSeekHarness
    }
    fn capability(&self) -> Capability {
        Capability::Usage
    }
    fn metric(&self) -> &'static str {
        "tokens"
    }
    fn collect(&self, stop: &TaskStop) -> Result<UsageSnapshot, UsageError> {
        let mut tokens = BTreeMap::<String, i64>::new();
        let mut calls = BTreeMap::<String, i64>::new();
        let mut readable = 0_usize;
        let paths = session_paths(&self.root);
        for path in &paths {
            if stop.is_stopped() {
                return Err(UsageError::Stopped);
            }
            let Ok(bytes) =
                read_session_bytes(&self.guard, ToolId::DeepSeekHarness, Capability::Usage, path)
            else {
                continue;
            };
            readable += 1;
            let Some(session) = parse_session(&bytes) else {
                continue;
            };
            if session.is_bot() {
                continue;
            }
            for call in &session.calls {
                let instant = UNIX_EPOCH + Duration::from_millis(call.time_ms.max(0) as u64);
                let day = local_calendar_day(instant, self.offset);
                *tokens.entry(day.clone()).or_default() += call.tokens;
                *calls.entry(day).or_default() += 1;
            }
        }
        if readable == 0 && !paths.is_empty() {
            return Err(UsageError::Unreadable);
        }
        let mut snapshot = UsageSnapshot::empty();
        snapshot.tokens = tokens.into_iter().collect();
        snapshot.calls = calls.into_iter().collect();
        Ok(snapshot)
    }
}

struct BalanceSource;

impl UsageSource for BalanceSource {
    fn tool(&self) -> ToolId {
        ToolId::DeepSeekHarness
    }
    fn capability(&self) -> Capability {
        Capability::AccountLink
    }
    fn metric(&self) -> &'static str {
        "balance"
    }
    fn collect(&self, stop: &TaskStop) -> Result<UsageSnapshot, UsageError> {
        if stop.is_stopped() {
            return Err(UsageError::Stopped);
        }
        let key = SecretStore::new().load().map_err(|err| match err {
            SecretError::Empty => UsageError::NoKey,
            SecretError::Unavailable => UsageError::QueryFailed,
        })?;
        let balances = fetch_balance("https://api.deepseek.com", &key);
        drop(key);
        balances.map(|balances| {
            let mut snapshot = UsageSnapshot::empty();
            snapshot.balances = balances;
            snapshot
        })
    }
}

struct CodexFiles<'a> {
    root: PathBuf,
    offset: i32,
    guard: Arc<Guard>,
    store: &'a Store,
    write_windows: bool,
}

impl UsageSource for CodexFiles<'_> {
    fn tool(&self) -> ToolId {
        ToolId::Codex
    }
    fn capability(&self) -> Capability {
        Capability::Usage
    }
    fn metric(&self) -> &'static str {
        "tokens"
    }
    fn collect(&self, stop: &TaskStop) -> Result<UsageSnapshot, UsageError> {
        if stop.is_stopped() {
            return Err(UsageError::Stopped);
        }
        let mut snapshot = UsageSnapshot::empty();
        snapshot.manage_session_counts = true;
        let mut seen = Vec::new();
        let mut days = BTreeMap::<String, i64>::new();
        let mut excluded = 0_u64;
        let mut partial = 0_u64;
        let mut counted = false;
        let mut newest: Option<(i64, Vec<WindowSample>)> = None;
        for path in rollout_paths(&self.root) {
            if stop.is_stopped() {
                return Err(UsageError::Stopped);
            }
            let meta = std::fs::metadata(&path).map_err(|_| UsageError::Unreadable)?;
            let key = path.to_string_lossy().into_owned();
            let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
            let mtime = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|time| i64::try_from(time.as_secs()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            let file = match self
                .store
                .codex_cached(&key, size, mtime)
                .map_err(|_| UsageError::Unreadable)?
            {
                Some(hit) => hit,
                None => {
                    let text = read_text(&self.guard, ToolId::Codex, &path)?;
                    let row = parse_rollout(&text);
                    let mut bucket = BTreeMap::<String, i64>::new();
                    for (at, tokens) in row.at_ms.iter().zip(&row.events) {
                        let instant = UNIX_EPOCH + Duration::from_millis((*at).max(0) as u64);
                        let day = local_calendar_day(instant, self.offset);
                        *bucket.entry(day).or_default() += tokens;
                    }
                    let file = CodexFile {
                        samples: i64::from(row.samples),
                        partial: row.partial,
                        days: bucket.into_iter().collect(),
                        limits_at: row.limits_at.unwrap_or(-1),
                        windows: row
                            .windows
                            .iter()
                            .take(2)
                            .map(|window| {
                                (
                                    window.minutes.unwrap_or(-1),
                                    i64::from(window.used_percent),
                                    window.resets_at.unwrap_or(-1),
                                )
                            })
                            .collect(),
                    };
                    self.store
                        .save_codex_file(&key, size, mtime, &file)
                        .map_err(|_| UsageError::Unreadable)?;
                    file
                }
            };
            seen.push(key);
            if file.samples == 0 {
                excluded += 1;
                continue;
            }
            counted = true;
            if file.partial {
                partial += 1;
            }
            for (day, tokens) in file.days {
                *days.entry(day).or_default() += tokens;
            }
            if file.limits_at >= 0
                && newest
                    .as_ref()
                    .map(|(at, _)| file.limits_at >= *at)
                    .unwrap_or(true)
            {
                let windows = file
                    .windows
                    .iter()
                    .map(|(minutes, percent, resets)| WindowSample {
                        minutes: (*minutes >= 0).then_some(*minutes),
                        used_percent: i32::try_from(*percent).unwrap_or(0),
                        resets_at: (*resets >= 0).then_some(*resets),
                    })
                    .collect();
                newest = Some((file.limits_at, windows));
            }
        }
        self.store
            .forget_codex_files(&seen)
            .map_err(|_| UsageError::Unreadable)?;
        snapshot.excluded_sessions = excluded;
        snapshot.partial_sessions = partial;
        if counted {
            snapshot.tokens = days.into_iter().collect();
            if self.write_windows {
                if let Some((_, windows)) = newest {
                    snapshot.windows = windows;
                }
            }
        }
        Ok(snapshot)
    }
}

pub fn readable_paths(root: &Path) -> Vec<PathBuf> {
    usage_db_candidates(root)
        .into_iter()
        .chain(rollout_paths(root))
        .collect()
}

fn admit(guard: &Guard, tool: ToolId, path: &Path) -> Result<(), UsageError> {
    let mut opened = guard
        .open_read(tool, Capability::Usage, path)
        .map_err(|_| UsageError::Unreadable)?;
    let mut byte = [0u8; 1];
    let _ = opened.read(&mut byte).map_err(|_| UsageError::Unreadable)?;
    Ok(())
}

fn read_text(guard: &Guard, tool: ToolId, path: &Path) -> Result<String, UsageError> {
    let mut opened = guard
        .open_read(tool, Capability::Usage, path)
        .map_err(|_| UsageError::Unreadable)?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = opened
            .read(&mut chunk)
            .map_err(|_| UsageError::Unreadable)?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    String::from_utf8(buf).map_err(|_| UsageError::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::readable_paths;

    #[test]
    fn readers_do_not_open_credential_files() {
        let root = std::env::temp_dir().join(format!("wanxiang-collect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sessions")).unwrap();
        std::fs::create_dir_all(root.join("bot")).unwrap();
        let auth = format!("{}{}", "auth", ".json");
        let creds = format!(".{}{}", "cred", "entials.yaml");
        let database = format!("{}{}", "state", ".vscdb");
        std::fs::write(root.join(&auth), b"hidden").unwrap();
        std::fs::write(root.join(creds), b"hidden").unwrap();
        std::fs::write(root.join(database), b"hidden").unwrap();
        std::fs::write(root.join("models.json"), b"hidden").unwrap();
        std::fs::write(root.join("sessions/demo.jsonl"), b"{}\n").unwrap();
        let names: Vec<String> = readable_paths(&root)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["demo.jsonl".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant, SystemTime};

    use wanxiang_core::{
        Capability, Clock, Guard, PathScope, RefreshGate, Store, Supervisor, SystemClock, TaskStop,
        ToolId, ToolScheduler, UsageError, UsageSource, WindowSample, REFRESH_EVERY,
    };

    use super::{due, refresh_body, RefreshKind, ServerProbe};

    struct StepClock(Mutex<SystemTime>);

    impl Clock for StepClock {
        fn now(&self) -> SystemTime {
            *self.0.lock().expect("clock")
        }
    }

    struct FakeServer<'a>(&'a AtomicU32);

    impl ServerProbe for FakeServer<'_> {
        fn start(&self, _gate: &RefreshGate) -> Result<Vec<WindowSample>, UsageError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(vec![])
        }
    }

    fn open_store(dir: &std::path::Path) -> Store {
        let supervisor = Supervisor::new();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let guard = Arc::new(Guard::new(
            Arc::clone(&gate) as Arc<dyn ToolScheduler>,
            Arc::clone(&clock),
        ));
        Store::open(&dir.join("wanxiang.sqlite"), guard, clock).unwrap()
    }

    #[test]
    fn periodic_rounds_do_not_start_app_server() {
        let dir = std::env::temp_dir().join(format!("wanxiang-periods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let codex = dir.join("codex");
        std::fs::create_dir_all(codex.join("sessions")).unwrap();
        let store = open_store(&dir);
        store
            .grant(
                ToolId::Codex,
                Capability::Usage,
                PathScope {
                    root: codex,
                    pattern: "sessions/**/*.jsonl".to_string(),
                },
            )
            .unwrap();
        let mut settings = store.settings().unwrap();
        settings.codex_app_server = true;
        store.set_settings(&settings).unwrap();
        let supervisor = Supervisor::new();
        let gate = Arc::new(RefreshGate::new(supervisor.handle()));
        let starts = AtomicU32::new(0);
        let server = FakeServer(&starts);
        let clock = StepClock(Mutex::new(
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        ));
        let mut marked = clock.now();
        for _ in 0..3 {
            *clock.0.lock().expect("clock") += REFRESH_EVERY;
            assert!(due(clock.now(), marked));
            marked = clock.now();
            refresh_body(
                &store,
                &gate,
                &TaskStop::running(),
                RefreshKind::Periodic,
                &server,
                &|_| {},
            );
        }
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        refresh_body(
            &store,
            &gate,
            &TaskStop::running(),
            RefreshKind::Manual,
            &server,
            &|_| {},
        );
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn second_codex_scan_is_at_least_five_times_faster() {
        let dir = std::env::temp_dir().join(format!("wanxiang-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("codex");
        let sessions = root.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let line = concat!(
            r#"{"timestamp":"2026-11-02T00:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":1,"total_tokens":5},"last_token_usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":1,"total_tokens":5}}}}"#,
            "\n"
        );
        let body = line.repeat(20);
        for index in 0..2000 {
            std::fs::write(sessions.join(format!("{index}.jsonl")), &body).unwrap();
        }
        let store = open_store(&dir);
        store
            .grant(
                ToolId::Codex,
                Capability::Usage,
                PathScope {
                    root: root.clone(),
                    pattern: "sessions/**/*.jsonl".to_string(),
                },
            )
            .unwrap();
        let guard = store.guard_handle();
        let scan = || {
            let started = Instant::now();
            let source = super::CodexFiles {
                root: root.clone(),
                offset: 0,
                guard: Arc::clone(&guard),
                store: &store,
                write_windows: false,
            };
            UsageSource::collect(&source, &TaskStop::running()).unwrap();
            started.elapsed()
        };
        let first = scan();
        let second = scan();
        println!(
            "codex-scan first_ms={} second_ms={}",
            first.as_millis(),
            second.as_millis()
        );
        assert!(
            second.as_secs_f64() * 5.0 < first.as_secs_f64(),
            "first {first:?} second {second:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
