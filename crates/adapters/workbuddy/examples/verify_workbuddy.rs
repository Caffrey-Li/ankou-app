//! 真实数据只读核对：只打印数量，不打印标题、编号、路径或任何对话正文。
//! WorkBuddy 自己的会话目录还没有记录时，用同一格式的 CodeBuddy 命令行记录核对解析。
//! 用法：cargo run -p wanxiang-adapter-workbuddy --example verify_workbuddy
use std::sync::Arc;

use wanxiang_adapter_workbuddy as wb;
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn check(guard: &Guard, label: &str, root: &std::path::Path) {
    let paths = wb::session_paths(root);
    if let Some(first) = paths.first() {
        println!("{label}：未授权时读取被拒：{}", wb::read_file(guard, Capability::Conversation, first).is_err());
    }
    guard
        .grant(ToolId::WorkBuddy, Capability::Conversation, PathScope { root: root.to_path_buf(), pattern: wb::PROJECTS_PATTERN.to_string() })
        .unwrap();
    let (mut readable, mut messages, mut titled, mut timed, mut spoken) = (0, 0, 0, 0, 0);
    for path in &paths {
        let Ok(bytes) = wb::read_file(guard, Capability::Conversation, path) else { continue };
        readable += 1;
        let id = path.file_stem().unwrap().to_string_lossy().into_owned();
        let session = wb::parse_session(&id, &bytes);
        messages += session.messages;
        titled += usize::from(!session.title().is_empty());
        timed += usize::from(session.updated_ms > 1_600_000_000_000);
        spoken += wb::transcript(&bytes).len();
    }
    println!("{label}：会话文件 {}，可读 {readable}，消息 {messages}，有标题 {titled}，时间可读 {timed}，原文块 {spoken}", paths.len());
}

fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let config = wb::config_root(&home);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    check(&guard, "WorkBuddy", &config);
    if config.join("workbuddy.db").is_file() {
        println!("会话表未授权时读取被拒：{}", wb::session_meta(&guard, &config).is_err());
        guard
            .grant(ToolId::WorkBuddy, Capability::Conversation, PathScope { root: config.clone(), pattern: wb::DB_PATTERN.to_string() })
            .unwrap();
        match wb::session_meta(&guard, &config) {
            Ok(meta) => println!(
                "会话表：{} 条，已删除 {}，有上下文 {}",
                meta.len(),
                meta.values().filter(|item| item.deleted).count(),
                meta.values().filter(|item| item.context.is_some()).count()
            ),
            Err(_) => println!("会话表读取失败"),
        }
    }
    let sample = home.join(".codebuddy");
    if sample.join("projects").is_dir() {
        check(&Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>), "同格式样本（CodeBuddy 命令行）", &sample);
    }
}
