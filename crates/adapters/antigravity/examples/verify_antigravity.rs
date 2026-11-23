//! 真实数据只读核对：只打印数量与范围，不打印标题、编号、路径或任何对话正文。
//! 用法：cargo run -p wanxiang-adapter-antigravity --example verify_antigravity
use std::sync::Arc;

use wanxiang_adapter_antigravity as ag;
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let roots = ag::data_roots(&home);
    println!("数据目录 {} 个", roots.len());
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    for (index, root) in roots.iter().enumerate() {
        println!("目录 {index}：未授权时读取被拒：{}", ag::list_conversations(&guard, Capability::Conversation, root).is_err());
        for (capability, pattern) in [
            (Capability::Conversation, ag::SUMMARY_PATTERN),
            (Capability::Conversation, ag::CONVERSATIONS_PATTERN),
            (Capability::Usage, ag::CONVERSATIONS_PATTERN),
        ] {
            guard
                .grant(ToolId::Antigravity, capability, PathScope { root: root.clone(), pattern: pattern.to_string() })
                .unwrap();
        }
        match ag::list_conversations(&guard, Capability::Conversation, root) {
            Ok(list) => {
                let top = list.iter().filter(|item| !item.nested).count();
                let titled = list.iter().filter(|item| !item.nested && !item.title.is_empty()).count();
                let timed = list.iter().filter(|item| item.updated_ms > 1_600_000_000_000).count();
                let mut with_text = 0;
                for item in list.iter().filter(|item| !item.nested) {
                    if let Ok(Some(chunks)) = ag::transcript(&guard, root, &item.id) {
                        with_text += usize::from(!chunks.is_empty());
                    }
                }
                println!("目录 {index}：对话 {}（主对话 {top}，有标题 {titled}，时间可读 {timed}，有原文 {with_text}）", list.len());
            }
            Err(_) => println!("目录 {index}：对话列表读取失败"),
        }
        let paths = ag::conversation_paths(root);
        let (mut calls, mut tokens, mut readable, mut with_context, mut max_percent) = (0, 0_i64, 0, 0, 0);
        let mut days = std::collections::BTreeSet::new();
        for path in &paths {
            if let Ok(found) = ag::calls(&guard, Capability::Usage, path) {
                readable += 1;
                calls += found.len();
                tokens += found.iter().map(|call| call.tokens).sum::<i64>();
                for call in &found {
                    days.insert(call.time_ms / 86_400_000);
                }
            }
            if let Ok(Some((used, limit))) = ag::context(&guard, path) {
                with_context += 1;
                max_percent = max_percent.max(ag::percent(used, limit).unwrap_or(0));
            }
        }
        println!(
            "目录 {index}：步骤库 {}，可读 {readable}，调用 {calls} 次，token 合计 {tokens}，覆盖 {} 天，有上下文 {with_context}，最高占用 {max_percent}%",
            paths.len(),
            days.len()
        );
    }
}
