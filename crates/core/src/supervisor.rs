use std::io;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// 协作式任务等到这个时间还没结束，就不再阻塞退出。
/// ponytail: 忽略取消标记的线程只能随进程一起结束，Rust 不能安全地杀掉线程。
/// 升级路径是把后台任务放进可强制取消的作业对象。子进程不走这条路径，会直接终止。
const JOIN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct TaskStop(Arc<AtomicBool>);

impl TaskStop {
    pub fn running() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct Inner {
    stop: Arc<AtomicBool>,
    finished: bool,
    threads: Vec<JoinHandle<()>>,
    children: Vec<Child>,
}

struct Shared {
    inner: Mutex<Inner>,
    gate: Mutex<()>,
}

/// 可复制的句柄。丢弃它不会把还在运行的监督器关掉。
#[derive(Clone)]
pub struct SupervisorHandle {
    shared: Arc<Shared>,
}

pub struct TrackedChild {
    pub pid: u32,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

/// 跟踪后台任务和子进程，退出时一次停掉。
pub struct Supervisor {
    handle: SupervisorHandle,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            handle: SupervisorHandle {
                shared: Arc::new(Shared {
                    inner: Mutex::new(Inner {
                        stop: Arc::new(AtomicBool::new(false)),
                        finished: false,
                        threads: Vec::new(),
                        children: Vec::new(),
                    }),
                    gate: Mutex::new(()),
                }),
            },
        }
    }

    pub fn handle(&self) -> SupervisorHandle {
        self.handle.clone()
    }

    pub fn spawn_task<F>(&self, task: F)
    where
        F: FnOnce(TaskStop) + Send + 'static,
    {
        self.handle.spawn_task(task);
    }

    pub fn spawn_child(&self, command: Command) -> io::Result<u32> {
        self.handle.spawn_child(command)
    }

    pub fn shutdown(&self) {
        self.handle.shutdown();
    }
}

impl SupervisorHandle {
    pub fn spawn_task<F>(&self, task: F)
    where
        F: FnOnce(TaskStop) + Send + 'static,
    {
        let mut inner = self.shared.inner.lock().expect("supervisor lock");
        if inner.finished {
            return;
        }
        let stop = TaskStop(Arc::clone(&inner.stop));
        inner.threads.push(thread::spawn(move || task(stop)));
    }

    pub fn spawn_child(&self, command: Command) -> io::Result<u32> {
        Ok(self.spawn_tracked(command)?.pid)
    }

    pub fn spawn_tracked(&self, mut command: Command) -> io::Result<TrackedChild> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let pid = child.id();
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("缺少标准输入"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("缺少标准输出"))?;
        let mut inner = self.shared.inner.lock().expect("supervisor lock");
        if inner.finished {
            terminate_child(&mut child);
            return Err(io::Error::other("supervisor already stopped"));
        }
        inner.children.push(child);
        Ok(TrackedChild { pid, stdin, stdout })
    }

    pub fn stop_child(&self, pid: u32) {
        let mut child = {
            let mut inner = self.shared.inner.lock().expect("supervisor lock");
            let Some(pos) = inner.children.iter().position(|child| child.id() == pid) else {
                return;
            };
            inner.children.remove(pos)
        };
        terminate_child(&mut child);
    }

    pub fn shutdown(&self) {
        let _gate = self.shared.gate.lock().expect("shutdown gate");
        let (threads, children) = {
            let mut inner = self.shared.inner.lock().expect("supervisor lock");
            if inner.finished {
                return;
            }
            inner.finished = true;
            inner.stop.store(true, Ordering::SeqCst);
            (
                std::mem::take(&mut inner.threads),
                std::mem::take(&mut inner.children),
            )
        };
        for mut child in children {
            terminate_child(&mut child);
        }
        for handle in threads {
            join_with_timeout(handle, JOIN_TIMEOUT);
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn terminate_child(child: &mut Child) {
    let pid = child.id();
    #[cfg(unix)]
    {
        signal_group(pid, "-TERM");
        let _ = child.kill();
        thread::sleep(Duration::from_millis(100));
        if matches!(child.try_wait(), Ok(None)) {
            signal_group(pid, "-KILL");
            let _ = child.kill();
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[cfg(unix)]
fn signal_group(pid: u32, signal: &str) {
    // 子进程以独立进程组启动，负 pid 只作用于那一组，不会打到本进程。
    let _ = Command::new("kill")
        .args([signal, &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn join_with_timeout(handle: JoinHandle<()>, timeout: Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = handle.join();
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(timeout);
}
