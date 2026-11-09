use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use wanxiang_core::{SupervisorHandle, UsageError, WindowSample};

use crate::rollout::parse_app_server_result;

const READ_TIMEOUT: Duration = Duration::from_secs(10);

pub fn query_rate_limits(
    handle: &SupervisorHandle,
    command: Command,
    register: impl FnOnce(u32),
) -> Result<Vec<WindowSample>, UsageError> {
    let tracked = handle.spawn_tracked(command).map_err(|_| UsageError::AppServer)?;
    let pid = tracked.pid;
    register(pid);
    let result = talk(tracked.stdin, tracked.stdout);
    handle.stop_child(pid);
    result
}

fn talk(mut stdin: ChildStdin, stdout: impl std::io::Read + Send + 'static) -> Result<Vec<WindowSample>, UsageError> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if tx.send(std::mem::take(&mut line)).is_err() {
                break;
            }
        }
    });
    write_line(
        &mut stdin,
        r#"{"method":"initialize","id":1,"params":{"clientInfo":{"name":"wanxiang","version":"0.0.0"}}}"#,
    )?;
    let ready = next_id(&rx, 1)?;
    if ready.get("error").is_some() {
        return Err(UsageError::AppServer);
    }
    write_line(&mut stdin, r#"{"method":"initialized"}"#)?;
    write_line(&mut stdin, r#"{"method":"account/rateLimits/read","id":2}"#)?;
    let response = next_id(&rx, 2)?;
    if response.get("error").is_some() {
        return Err(UsageError::AppServer);
    }
    Ok(parse_app_server_result(&response))
}

fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<(), UsageError> {
    writeln!(stdin, "{line}").map_err(|_| UsageError::AppServer)?;
    stdin.flush().map_err(|_| UsageError::AppServer)
}

fn next_id(rx: &mpsc::Receiver<String>, id: i64) -> Result<Value, UsageError> {
    let deadline = std::time::Instant::now() + READ_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(UsageError::AppServer);
        }
        let line = rx.recv_timeout(left).map_err(|_| UsageError::AppServer)?;
        let value: Value = serde_json::from_str(line.trim()).map_err(|_| UsageError::AppServer)?;
        if value.get("id").and_then(|item| item.as_i64()) == Some(id) {
            return Ok(value);
        }
    }
}
