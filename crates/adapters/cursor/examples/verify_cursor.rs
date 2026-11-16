//! 真实数据只读核对：只打印数量与分布，不打印标题、编号、路径或任何对话正文。
//! 用法：cargo run -p wanxiang-adapter-cursor --example verify_cursor
use std::sync::Arc;
use std::time::Instant;

use wanxiang_adapter_cursor::{chat_chunks, list_chats, STATE_PATTERN};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::env::var_os("HOME").expect("HOME");
    let root = std::path::PathBuf::from(home)
        .join("Library")
        .join(format!("{} {}", "Application", "Support"))
        .join("Cursor");
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    println!("未授权时读取被拒：{}", list_chats(&guard, &root).is_err());
    guard
        .grant(
            ToolId::Cursor,
            Capability::Conversation,
            PathScope { root: root.clone(), pattern: STATE_PATTERN.to_string() },
        )
        .unwrap();
    let started = Instant::now();
    let chats = match list_chats(&guard, &root) {
        Ok(chats) => chats,
        Err(err) => {
            println!("读取失败：{err:?}");
            std::process::exit(1);
        }
    };
    let elapsed = started.elapsed().as_millis();
    let titled = chats.iter().filter(|chat| !chat.title.is_empty()).count();
    let percents: Vec<u32> = chats.iter().filter_map(|chat| chat.percent).collect();
    let messages: usize = chats.iter().map(|chat| chat.messages).sum();
    let sorted = chats.windows(2).all(|pair| pair[0].updated_ms >= pair[1].updated_ms);
    println!("对话 {} 条，有标题 {}，有上下文占用 {}，消息气泡合计 {}", chats.len(), titled, percents.len(), messages);
    if !percents.is_empty() {
        println!(
            "上下文占用 最小 {}% 最大 {}%，≥80% 的 {} 条",
            percents.iter().min().unwrap(),
            percents.iter().max().unwrap(),
            percents.iter().filter(|value| **value >= 80).count()
        );
    }
    println!("按更新时间倒序：{sorted}；列表耗时 {elapsed} ms");
    if let Some(first) = chats.first() {
        let chunks = chat_chunks(&guard, &root, &first.id).ok().flatten().unwrap_or_default();
        let users = chunks.iter().filter(|chunk| chunk.starts_with("用户：")).count();
        println!(
            "最新一条：正文段 {}（用户 {}、助手 {}），气泡 {}",
            chunks.len(),
            users,
            chunks.len() - users,
            first.messages
        );
    }
    println!("不存在的编号返回空：{}", matches!(chat_chunks(&guard, &root, "00000000-0000-0000-0000-000000000000"), Ok(None)));
}
