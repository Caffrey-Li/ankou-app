use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use wanxiang_core::{Capability, RefreshGate, Supervisor, ToolId, ToolScheduler, REFRESH_EVERY, STALE_AFTER};

fn sleeper() -> Command {
    let mut command = Command::new("sleep");
    command.arg("30").stdin(Stdio::null());
    command
}

fn alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[test]
fn refresh_interval_is_fifteen_minutes() {
    assert_eq!(REFRESH_EVERY, Duration::from_secs(15 * 60));
    assert_eq!(STALE_AFTER, REFRESH_EVERY);
    let supervisor = Supervisor::new();
    let gate = RefreshGate::new(supervisor.handle());
    assert!(gate.try_begin());
    assert!(!gate.try_begin());
    gate.end();
    assert!(gate.try_begin());
}

#[test]
fn shutdown_stops_the_refresh_loop_and_its_child() {
    let supervisor = Supervisor::new();
    let pid = supervisor.spawn_child(sleeper()).expect("子进程");
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    supervisor.spawn_task(move |stop| {
        wanxiang_core::wait_interval(&stop, REFRESH_EVERY);
        flag.store(stop.is_stopped(), Ordering::SeqCst);
    });
    let started = Instant::now();
    supervisor.shutdown();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(finished.load(Ordering::SeqCst));
    for _ in 0..20 {
        if !alive(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("子进程还在");
}

#[test]
fn revoking_codex_usage_stops_its_child() {
    let supervisor = Supervisor::new();
    let gate = RefreshGate::new(supervisor.handle());
    let pid = gate.supervisor().spawn_child(sleeper()).expect("子进程");
    gate.track_codex(pid);
    assert!(alive(pid));
    ToolScheduler::on_revoked(&gate, ToolId::Codex, Capability::Usage);
    for _ in 0..20 {
        if !alive(pid) {
            assert!(gate.is_blocked(ToolId::Codex, Capability::Usage));
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("撤销后子进程还在");
}
