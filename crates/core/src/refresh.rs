use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::{
    Capability, Store, SupervisorHandle, TaskStop, ToolId, ToolScheduler, UsageError, UsageSnapshot,
};

pub struct RefreshGate {
    revoked: Mutex<HashSet<(ToolId, Capability)>>,
    supervisor: SupervisorHandle,
    codex_pid: Mutex<Option<u32>>,
    busy: AtomicBool,
}

impl RefreshGate {
    pub fn new(supervisor: SupervisorHandle) -> Self {
        Self {
            revoked: Mutex::new(HashSet::new()),
            supervisor,
            codex_pid: Mutex::new(None),
            busy: AtomicBool::new(false),
        }
    }

    pub fn try_begin(&self) -> bool {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn end(&self) {
        self.busy.store(false, Ordering::Release);
    }

    pub fn is_blocked(&self, tool: ToolId, capability: Capability) -> bool {
        self.revoked
            .lock()
            .expect("revoked")
            .contains(&(tool, capability))
    }

    pub fn track_codex(&self, pid: u32) {
        *self.codex_pid.lock().expect("codex pid") = Some(pid);
    }

    pub fn clear_codex(&self) {
        *self.codex_pid.lock().expect("codex pid") = None;
    }

    pub fn supervisor(&self) -> SupervisorHandle {
        self.supervisor.clone()
    }
}

impl ToolScheduler for RefreshGate {
    fn on_revoked(&self, tool: ToolId, capability: Capability) {
        self.revoked
            .lock()
            .expect("revoked")
            .insert((tool, capability));
        if tool == ToolId::Codex && capability == Capability::Usage {
            if let Some(pid) = self.codex_pid.lock().expect("codex pid").take() {
                self.supervisor.stop_child(pid);
            }
        }
    }
}

pub trait UsageSource: Send + Sync {
    fn tool(&self) -> ToolId;
    fn capability(&self) -> Capability;
    fn metric(&self) -> &'static str;
    fn collect(&self, stop: &TaskStop) -> Result<UsageSnapshot, UsageError>;
}

pub fn permitted(store: &Store, gate: &RefreshGate, tool: ToolId, capability: Capability) -> bool {
    if gate.is_blocked(tool, capability) {
        return false;
    }
    match capability {
        Capability::AccountLink => store.guard().account_link_active(tool),
        _ => store
            .guard()
            .grants()
            .iter()
            .any(|grant| grant.tool == tool && grant.capability == capability),
    }
}

pub fn run_collect(
    store: &Store,
    gate: &RefreshGate,
    stop: &TaskStop,
    source: &dyn UsageSource,
) -> Result<(), UsageError> {
    if stop.is_stopped() {
        return Err(UsageError::Stopped);
    }
    if !permitted(store, gate, source.tool(), source.capability()) {
        return Ok(());
    }
    let result = source.collect(stop);
    if stop.is_stopped() || !permitted(store, gate, source.tool(), source.capability()) {
        return Ok(());
    }
    match result {
        Ok(snapshot) => store
            .write_snapshot(source.tool(), source.metric(), &snapshot)
            .map_err(|_| UsageError::Unreadable),
        Err(err) => {
            let _ = store.mark_usage(source.tool(), source.metric(), err.state());
            Err(err)
        }
    }
}

pub fn wait_interval(stop: &TaskStop, total: Duration) {
    let slice = Duration::from_secs(1).min(total);
    let mut left = total;
    while left > Duration::ZERO {
        if stop.is_stopped() {
            return;
        }
        let step = slice.min(left);
        thread::sleep(step);
        left = left.saturating_sub(step);
    }
}
