//! Cursor 命令行对话的真实数据只读核对：只打印数量，不打印标题、编号、路径或正文。
//! 用法：cargo run -p wanxiang-adapter-cursor --example verify_cursor_cli
use std::sync::Arc;

use wanxiang_adapter_cursor::{cli_chunks, cli_root, list_cli_chats, CLI_META_PATTERN, CLI_STORE_PATTERN};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let root = cli_root(&home);
    if !root.join("chats").is_dir() {
        println!("本机没有 Cursor 命令行对话目录，跳过");
        return;
    }
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    println!("未授权时读取被拒：{}", list_cli_chats(&guard, &root).is_err());
    for pattern in [CLI_META_PATTERN, CLI_STORE_PATTERN] {
        guard
            .grant(ToolId::CursorCli, Capability::Conversation, PathScope { root: root.clone(), pattern: pattern.to_string() })
            .unwrap();
    }
    let chats = match list_cli_chats(&guard, &root) {
        Ok(chats) => chats,
        Err(err) => {
            println!("读取失败：{err:?}");
            std::process::exit(1);
        }
    };
    let titled = chats.iter().filter(|chat| !chat.title.is_empty()).count();
    let messages: usize = chats.iter().map(|chat| chat.messages).sum();
    let sorted = chats.windows(2).all(|pair| pair[0].updated_ms >= pair[1].updated_ms);
    println!("命令行对话 {} 条，有标题 {}，文字消息合计 {}，按时间倒序：{sorted}", chats.len(), titled, messages);
    if let Some(first) = chats.first() {
        let chunks = cli_chunks(&guard, &root, &first.id).ok().flatten().unwrap_or_default();
        let users = chunks.iter().filter(|chunk| chunk.starts_with("用户：")).count();
        println!("最新一条：正文段 {}（用户 {}、助手 {}）", chunks.len(), users, chunks.len() - users);
    }
}
