use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use wanxiang_adapter_codex::{
    combine_rollouts, find_codex_cli, parse_rollout, query_rate_limits, rollout_paths,
};
use wanxiang_adapter_dsh::{daily_counts, fetch_balance, read_usage_set, usage_db_candidates};
use wanxiang_core::{
    local_calendar_day, run_collect, wait_interval, Capability, GrantStatus, Guard, RefreshGate,
    SecretError, SecretStore, Store, TaskStop, ToolId, UsageError, UsageSnapshot, UsageSource,
    UsageState, REFRESH_EVERY,
};

pub fn spawn_refresh(store: Arc<Store>, gate: Arc<RefreshGate>) {
    if std::env::var_os("WANXIANG_SKIP_REFRESH").is_some() {
        return;
    }
    gate.supervisor().spawn_task(move |stop| {
        let launch = store
            .settings()
            .map(|settings| settings.refresh_on_launch)
            .unwrap_or(false);
        if launch {
            refresh_once(&store, &gate, &stop);
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
            if stop.is_stopped() {
                break;
            }
            refresh_once(&store, &gate, &stop);
        }
    });
}

pub fn refresh_once(store: &Store, gate: &Arc<RefreshGate>, stop: &TaskStop) {
    if !gate.try_begin() {
        return;
    }
    let _busy = Busy(gate);
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
            grant.tool == tool && grant.capability == capability && grant.status == GrantStatus::Active
        })
    };
    if let Some(root) = grants.iter().find(|grant| {
        grant.tool == ToolId::DsBot && grant.capability == Capability::Usage && grant.status == GrantStatus::Active
    }) {
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
    if store.guard().account_link_active(ToolId::DeepSeekHarness) {
        let _ = run_collect(store, gate, stop, &BalanceSource);
    }
    if let Some(root) = grants.iter().find(|grant| {
        grant.tool == ToolId::Codex && grant.capability == Capability::Usage && grant.status == GrantStatus::Active
    }) {
        let _ = run_collect(
            store,
            gate,
            stop,
            &CodexFiles {
                root: root.root.clone(),
                offset: store.offset_secs(),
                guard: store.guard_handle(),
            },
        );
        if settings.codex_app_server && active(ToolId::Codex, Capability::Usage) {
            let _ = run_collect(
                store,
                gate,
                stop,
                &CodexServer {
                    gate: Arc::clone(gate),
                },
            );
        } else {
            let _ = store.mark_usage(ToolId::Codex, "windows", UsageState::AppOff);
        }
    }
}

struct Busy<'a>(&'a RefreshGate);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.end();
    }
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
        if !files.iter().any(|path| path.file_name().and_then(|name| name.to_str()) == Some("usage.sqlite")) {
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

struct CodexFiles {
    root: PathBuf,
    offset: i32,
    guard: Arc<Guard>,
}

impl UsageSource for CodexFiles {
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
        let mut parsed = Vec::new();
        for path in readable_paths(&self.root) {
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let text = read_text(&self.guard, ToolId::Codex, &path)?;
            parsed.push(parse_rollout(&text));
        }
        let mut snapshot = UsageSnapshot::empty();
        if let Some((_, excluded)) = combine_rollouts(&parsed) {
            snapshot.excluded_sessions = excluded;
        } else if !parsed.is_empty() {
            snapshot.excluded_sessions = u64::try_from(parsed.len()).unwrap_or(0);
            return Ok(snapshot);
        }
        let mut days = std::collections::BTreeMap::<String, i64>::new();
        for row in &parsed {
            for (at, tokens) in row.at_ms.iter().zip(&row.events) {
                let instant = UNIX_EPOCH + Duration::from_millis((*at).max(0) as u64);
                let day = local_calendar_day(instant, self.offset);
                *days.entry(day).or_default() += tokens;
            }
        }
        snapshot.tokens = days.into_iter().collect();
        if let Some(newest) = parsed
            .iter()
            .filter(|row| !row.windows.is_empty())
            .max_by_key(|row| row.limits_at.unwrap_or(0))
        {
            snapshot.windows = newest.windows.clone();
        }
        Ok(snapshot)
    }
}

struct CodexServer {
    gate: Arc<RefreshGate>,
}

impl UsageSource for CodexServer {
    fn tool(&self) -> ToolId {
        ToolId::Codex
    }
    fn capability(&self) -> Capability {
        Capability::Usage
    }
    fn metric(&self) -> &'static str {
        "windows"
    }
    fn collect(&self, stop: &TaskStop) -> Result<UsageSnapshot, UsageError> {
        if stop.is_stopped() {
            return Err(UsageError::Stopped);
        }
        let path = std::env::var("PATH").unwrap_or_default();
        let Some(bin) = find_codex_cli(&path) else {
            return Err(UsageError::AppServer);
        };
        let mut command = Command::new(bin);
        command.arg("app-server");
        let gate = Arc::clone(&self.gate);
        let windows = query_rate_limits(&self.gate.supervisor(), command, move |pid| {
            gate.track_codex(pid);
        })?;
        self.gate.clear_codex();
        let mut snapshot = UsageSnapshot::empty();
        snapshot.windows = windows;
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
        let read = opened.read(&mut chunk).map_err(|_| UsageError::Unreadable)?;
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
}
