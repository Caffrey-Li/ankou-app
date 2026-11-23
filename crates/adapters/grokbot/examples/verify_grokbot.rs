//! 真实数据只读核对：只打印数量，不打印名字、编号、账号或任何对话正文。
//! 用法：cargo run -p wanxiang-adapter-grokbot --example verify_grokbot
use std::sync::Arc;

use wanxiang_adapter_grokbot as grok;
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let root = grok::support_root(&home);
    let slices = grok::slices(&root);
    let transcripts = slices.iter().filter(|(slice, _)| matches!(slice, grok::Slice::Transcript(_))).count();
    println!("认得的缓存文件 {}（会话缓存 {transcripts}）", slices.len());
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    println!("未授权时读取被拒：{}", grok::agents(&guard, &root).is_err());
    for capability in [Capability::Conversation, Capability::Usage] {
        guard
            .grant(ToolId::GrokBot, capability, PathScope { root: root.clone(), pattern: grok::PERSIST_PATTERN.to_string() })
            .unwrap();
    }
    match grok::agents(&guard, &root) {
        Ok(list) => {
            let named = list.iter().filter(|agent| !agent.name.is_empty()).count();
            let timed = list.iter().filter(|agent| agent.updated_ms > 1_600_000_000_000).count();
            let (mut cached, mut chunks) = (0, 0);
            for agent in &list {
                if let Ok(Some(found)) = grok::transcript(&guard, &root, &agent.id) {
                    cached += 1;
                    chunks += found.len();
                }
            }
            println!("会话 {}，有名字 {named}，时间可读 {timed}，有缓存消息 {cached}，消息块合计 {chunks}", list.len());
        }
        Err(_) => println!("会话列表读取失败"),
    }
    match grok::weekly_usage(&guard, &root) {
        Ok(Some(usage)) => println!(
            "每周额度：有百分比，有重置时间 {}，有读取时间 {}",
            usage.resets_ms.is_some(),
            usage.read_ms.is_some()
        ),
        Ok(None) => println!("每周额度：没有缓存"),
        Err(_) => println!("每周额度读取失败"),
    }
}
