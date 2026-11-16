//! 真实数据只读核对：ChatGPT 本机会话列表与交接正文，只打印数量，不打印标题与正文。
//! 用法：cargo run -p wanxiang-adapter-codex --example verify_codex
use std::sync::Arc;

use wanxiang_adapter_codex::{context_percent, list_jsonl, read_jsonl, rollout_paths};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::env::var_os("HOME").expect("HOME");
    let root = std::path::PathBuf::from(home).join(".codex");
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    let paths = rollout_paths(&root);
    println!("ChatGPT 会话文件 {}", paths.len());
    assert!(list_jsonl(&guard, &root).is_err(), "未授权时不应能列出");
    guard
        .grant(
            ToolId::Codex,
            Capability::Conversation,
            PathScope { root: root.clone(), pattern: "sessions/**/*.jsonl".to_string() },
        )
        .unwrap();
    let heads = match list_jsonl(&guard, &root) {
        Ok(heads) => heads,
        Err(err) => {
            println!("列出失败：{err:?}");
            return;
        }
    };
    let with_title = heads.iter().filter(|h| h.name.as_deref().is_some_and(|n| !n.trim().is_empty()) || !h.preview.trim().is_empty()).count();
    let percents: Vec<u32> = heads.iter().filter_map(|h| context_percent(h.used_tokens, h.window_tokens)).collect();
    println!("列出 {}，有标题或预览 {}，有上下文比例 {}，最高 {:?}%", heads.len(), with_title, percents.len(), percents.iter().max());
    let mut ok = 0;
    let mut empty = 0;
    let mut failed = 0;
    let mut truncated = 0;
    let mut empty_kinds = std::collections::BTreeMap::<String, usize>::new();
    for path in paths.iter().rev().take(20) {
        match read_jsonl(&guard, path) {
            Ok((text, cut)) => {
                if text.trim().is_empty() {
                    empty += 1;
                    let raw = std::fs::read_to_string(path).unwrap_or_default();
                    for line in raw.lines() {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                            let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                            let inner = value.pointer("/payload/type").and_then(|v| v.as_str()).unwrap_or("-");
                            *empty_kinds.entry(format!("{kind}/{inner}")).or_default() += 1;
                        }
                    }
                } else {
                    ok += 1;
                }
                if cut {
                    truncated += 1;
                }
            }
            Err(_) => failed += 1,
        }
    }
    println!("最近 20 个会话读取正文：有内容 {ok}，空 {empty}，失败 {failed}，截取最近部分 {truncated}");
    println!("正文为空的会话里出现的记录类型（只列类型名）：{empty_kinds:?}");
    let titled = heads.iter().filter(|h| !wanxiang_adapter_codex::title_of(h).starts_with("ChatGPT ")).count();
    println!("能取到首句作标题的 {titled}/{}", heads.len());
}
