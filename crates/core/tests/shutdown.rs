use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use wanxiang_core::Supervisor;

fn sleeper() -> Command {
    #[cfg(unix)]
    {
        let mut command = Command::new("sleep");
        command.arg("30").stdin(Stdio::null());
        command
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .stdin(Stdio::null());
        command.creation_flags(0x0800_0000);
        command
    }
}

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("ps")
            .args(["-p", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        match output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
}

fn wait_until(mut probe: impl FnMut() -> bool) -> bool {
    for _ in 0..20 {
        if probe() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn shutdown_stops_child_process() {
    let supervisor = Supervisor::new();
    let pid = supervisor.spawn_child(sleeper()).expect("spawn sleeper");
    assert!(wait_until(|| alive(pid)), "sleeper did not start");
    supervisor.shutdown();
    assert!(
        wait_until(|| !alive(pid)),
        "sleeper {pid} still running after shutdown"
    );
}

#[test]
fn shutdown_stops_background_task() {
    let supervisor = Supervisor::new();
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    supervisor.spawn_task(move |stop| {
        while !stop.is_stopped() {
            thread::sleep(Duration::from_millis(10));
        }
        flag.store(true, Ordering::SeqCst);
    });
    supervisor.shutdown();
    assert!(finished.load(Ordering::SeqCst));
}

#[test]
fn shutdown_can_be_called_twice() {
    let supervisor = Supervisor::new();
    supervisor.shutdown();
    supervisor.shutdown();
}
