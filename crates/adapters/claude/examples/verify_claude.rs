//! 真实数据只读核对：只打印数量与范围，不打印标题、编号、路径或任何对话正文。
//! 用法：cargo run -p wanxiang-adapter-claude --example verify_claude
use std::sync::Arc;

use wanxiang_adapter_claude as claude;
use wanxiang_core::{Capability, Clock, Guard, PathScope, SystemClock, ToolId, ToolScheduler};

struct Nop;
impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let config = claude::config_root(&home);
    let support = claude::support_root(&home);
    let guard = Guard::new(Arc::new(Nop), Arc::new(SystemClock) as Arc<dyn Clock>);
    let paths = claude::session_paths(&config);
    println!("会话文件 {} 个", paths.len());
    if let Some(first) = paths.first() {
        println!("未授权时读取被拒：{}", claude::read_file(&guard, Capability::Usage, first).is_err());
    }
    guard
        .grant(
            ToolId::Claude,
            Capability::Usage,
            PathScope { root: config.clone(), pattern: claude::PROJECTS_PATTERN.to_string() },
        )
        .unwrap();
    let (mut readable, mut calls, mut tokens, mut titled, mut with_user) = (0, 0, 0_i64, 0, 0);
    let mut days = std::collections::BTreeSet::new();
    for path in &paths {
        let Ok(bytes) = claude::read_file(&guard, Capability::Usage, path) else { continue };
        readable += 1;
        let id = path.file_stem().unwrap().to_string_lossy().into_owned();
        let session = claude::parse_session(&id, &bytes);
        calls += session.calls.len();
        tokens += session.calls.iter().map(|call| call.tokens).sum::<i64>();
        titled += usize::from(session.custom_title.is_some());
        with_user += usize::from(session.first_user.is_some());
        for call in &session.calls {
            days.insert(call.time_ms / 86_400_000);
        }
        let chunks = claude::transcript(&bytes);
        let _ = chunks.len();
    }
    println!("可读 {readable}，调用 {calls} 次，token 合计 {tokens}，覆盖 {} 天", days.len());
    println!("有自定义标题 {titled}，有用户首句 {with_user}");
    let plan_path = support.join(claude::PLAN_PATTERN);
    println!("额度历史存在：{}", plan_path.is_file());
    if plan_path.is_file() {
        println!("未授权时读取额度历史被拒：{}", claude::read_file(&guard, Capability::Usage, &plan_path).is_err());
        guard
            .grant(
                ToolId::Claude,
                Capability::Usage,
                PathScope { root: support.clone(), pattern: claude::PLAN_PATTERN.to_string() },
            )
            .unwrap();
        match claude::read_file(&guard, Capability::Usage, &plan_path).ok().and_then(|b| claude::latest_plan(&b)) {
            Some(plan) => {
                let age_min = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64
                    - plan.at_ms)
                    / 60_000;
                println!(
                    "最新额度：5 小时 {:?}%，每周 {:?}%，是 {age_min} 分钟前记下的",
                    plan.five_hour, plan.seven_day
                );
            }
            None => println!("额度历史里没有可用记录"),
        }
    }
    let desktop = claude::desktop_session_paths(&support);
    guard
        .grant(
            ToolId::Claude,
            Capability::Conversation,
            PathScope { root: support.clone(), pattern: claude::DESKTOP_SESSIONS_PATTERN.to_string() },
        )
        .ok();
    let named = desktop
        .iter()
        .filter_map(|path| claude::read_file(&guard, Capability::Conversation, path).ok())
        .filter_map(|bytes| claude::desktop_session(&bytes))
        .filter(|session| !session.title.is_empty())
        .count();
    println!("桌面应用会话标题文件 {} 个，有标题 {named}", desktop.len());
    let matched = desktop
        .iter()
        .filter_map(|path| claude::read_file(&guard, Capability::Conversation, path).ok())
        .filter_map(|bytes| claude::desktop_session(&bytes))
        .filter(|session| claude::find_session(&config, &session.cli_session_id).is_some())
        .count();
    println!("能对上会话文件的标题 {matched}");
}
