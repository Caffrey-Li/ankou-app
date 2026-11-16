//! 真实数据只读核对：只打印数量与合计，不打印标题、路径或任何对话正文。
//! 用法：cargo run -p wanxiang-adapter-dsh --example verify_dsh
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use wanxiang_adapter_dsh::{
    owner_kind, parse_session, read_session_bytes, read_usage_set, session_chunks, session_paths,
    usage_db_candidates, OwnerKind, BOT_SESSION_PATTERN, HARNESS_PATTERN,
};
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::env::var_os("HOME").expect("HOME");
    let root = std::path::PathBuf::from(home).join(".dsh");
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    let scope = |pattern: &str| PathScope { root: root.clone(), pattern: pattern.to_string() };
    guard.grant(ToolId::DeepSeekHarness, Capability::Usage, scope(HARNESS_PATTERN)).unwrap();
    guard.grant(ToolId::DeepSeekHarness, Capability::Conversation, scope(HARNESS_PATTERN)).unwrap();
    guard.grant(ToolId::DsBot, Capability::Conversation, scope(BOT_SESSION_PATTERN)).unwrap();

    let paths = session_paths(&root);
    let mut harness = Vec::new();
    let mut bot = Vec::new();
    let mut unreadable = 0;
    let mut unparsed = 0;
    for path in &paths {
        let Ok(bytes) = read_session_bytes(&guard, ToolId::DeepSeekHarness, Capability::Usage, path) else {
            unreadable += 1;
            continue;
        };
        match parse_session(&bytes) {
            Some(session) if session.is_bot() => {
                let via_bot = read_session_bytes(&guard, ToolId::DsBot, Capability::Conversation, path).is_ok();
                bot.push((session, via_bot, session_chunks(&bytes).len()));
            }
            Some(session) => {
                let chunks = session_chunks(&bytes).len();
                harness.push((session, chunks));
            }
            None => unparsed += 1,
        }
    }
    println!("会话文件 {}：Harness {}，Bot 工作区 {}，读不了 {}，解析失败 {}", paths.len(), harness.len(), bot.len(), unreadable, unparsed);
    let calls: usize = harness.iter().map(|(s, _)| s.calls.len()).sum();
    let tokens: i64 = harness.iter().flat_map(|(s, _)| s.calls.iter().map(|c| c.tokens)).sum();
    println!("Harness 调用 {calls} 次，合计 token {tokens}");
    let dups: usize = harness.iter().map(|(s, _)| s.duplicates).sum();
    println!("按消息编号去掉的重复记录 {dups}");
    let with_title = harness.iter().filter(|(s, _)| !s.title.is_empty()).count();
    let with_percent: Vec<u32> = harness.iter().filter_map(|(s, _)| s.context_percent()).collect();
    println!(
        "Harness 有标题 {with_title}/{}，有上下文比例 {}/{}，最高 {:?}%",
        harness.len(),
        with_percent.len(),
        harness.len(),
        with_percent.iter().max()
    );
    let empty_text = harness.iter().filter(|(_, chunks)| *chunks == 0).count();
    println!("Harness 交接正文段落为空的会话 {empty_text}");
    let bot_denied = bot.iter().filter(|(_, ok, _)| !ok).count();
    println!("Bot 会话经 DS Bot 授权可读 {}/{}", bot.len() - bot_denied, bot.len());
    let harness_by_bot = paths.iter().filter(|path| {
        read_session_bytes(&guard, ToolId::DsBot, Capability::Conversation, path)
            .ok()
            .and_then(|bytes| parse_session(&bytes))
            .is_some_and(|session| !session.is_bot())
    });
    println!("DS Bot 授权能读到的 Harness 会话 {}（应为 0）", harness_by_bot.count());

    // 与调用记录对照：同一会话在两处的调用次数
    let files = usage_db_candidates(&root);
    if files.is_empty() {
        println!("没有 bot/usage.sqlite，跳过对照");
        return;
    }
    let rows = read_usage_set(&files).expect("usage.sqlite");
    let mut first_db = BTreeMap::<String, i64>::new();
    for row in &rows {
        let at = first_db.entry(row.session.clone()).or_insert(row.time_ms);
        *at = (*at).min(row.time_ms);
    }
    let mut per_session = BTreeMap::<String, (usize, i64, bool)>::new();
    for row in &rows {
        let entry = per_session.entry(row.session.clone()).or_default();
        entry.0 += 1;
        entry.1 += row.input + row.cache_read + row.cache_write + row.output;
        entry.2 |= owner_kind(&row.owner) != OwnerKind::Other;
    }
    let harness_ids: HashSet<&str> = harness.iter().map(|(s, _)| s.id.as_str()).collect();
    let mut matched = 0;
    let mut file_calls = 0;
    let mut db_calls = 0;
    let mut file_tokens = 0;
    let mut db_tokens = 0;
    let mut owned = 0;
    for (session, _) in &harness {
        if let Some((count, sum, has_owner)) = per_session.get(&session.id) {
            matched += 1;
            let since = first_db.get(&session.id).copied().unwrap_or(0);
            let window: Vec<_> = session.calls.iter().filter(|c| c.time_ms >= since).collect();
            let last_db = rows
                .iter()
                .filter(|row| row.session == session.id)
                .map(|row| row.time_ms)
                .max()
                .unwrap_or(0);
            let within = window.iter().filter(|c| c.time_ms <= last_db + 60_000).count();
            println!(
                "  会话#{matched}: 文件 {} 次 / 记录 {} 次（记录时段内文件 {} 次）；token 文件 {} / 记录 {}",
                window.len(),
                count,
                within,
                window.iter().map(|c| c.tokens).sum::<i64>(),
                sum
            );
            file_calls += window.len();
            db_calls += count;
            file_tokens += window.iter().map(|c| c.tokens).sum::<i64>();
            db_tokens += sum;
            if *has_owner {
                owned += 1;
            }
        }
    }
    println!(
        "调用记录里的会话 {}，其中 Harness {}（带 Bot/群聊归属的 {}，应为 0）",
        per_session.len(),
        matched,
        owned
    );
    println!("同一批 Harness 会话（只比记录开始之后）：文件 {file_calls} 次 / 记录 {db_calls} 次；token 文件 {file_tokens} / 记录 {db_tokens}");

    let only_db = per_session.keys().filter(|id| !harness_ids.contains(id.as_str())).count();
    println!("只在调用记录里、不属于 Harness 的会话 {only_db}");
}
