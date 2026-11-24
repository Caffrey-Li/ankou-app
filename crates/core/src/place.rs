//! 交接放入：默认回到原应用开新对话继续。
//!
//! 只用各应用公开说明过的「打开新对话并预填内容、不自动发送」链接；其余应用复制后打开应用，
//! 由用户粘贴。链接前缀写死在这里，不接受调用方传入的网址。

use serde::Serialize;

/// 预填内容的字数上限：超过就改为复制（Claude 说明会截断到约 14000 字，这里留余量）
pub const PREFILL_CHARS: usize = 8_000;
/// 编码后的链接长度上限
pub const PREFILL_URL_CHARS: usize = 120_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub id: &'static str,
    pub name: &'static str,
    /// 打开新对话并预填的链接前缀，内容拼在后面
    pub prefix: Option<&'static str>,
    /// 复制后要打开的应用
    pub bundle: Option<&'static str>,
    /// 工作目录参数（原对话有工作目录时带上，让新对话落在同一个项目）
    pub folder: Option<&'static str>,
}

pub const TARGETS: &[Target] = &[
    // Claude Desktop 说明：claude://code/new?q= 打开 Claude Code 并预填，不自动发送
    Target { id: "claude", name: "Claude", prefix: Some("claude://code/new?q="), bundle: Some("com.anthropic.claudefordesktop"), folder: Some("&folder=") },
    // Claude Desktop 说明：claude://claude.ai/new?q= 打开新聊天并预填，不自动发送
    Target { id: "claude_chat", name: "Claude 聊天", prefix: Some("claude://claude.ai/new?q="), bundle: Some("com.anthropic.claudefordesktop"), folder: None },
    // ChatGPT / Codex 应用说明：codex://new?prompt= 打开新对话并预填，不自动发送
    Target { id: "codex", name: "ChatGPT", prefix: Some("codex://new?prompt="), bundle: Some("com.openai.codex"), folder: Some("&path=") },
    // Cursor 的 Agents 窗口不接收深链（实测），复制后打开
    Target { id: "cursor", name: "Cursor", prefix: None, bundle: Some("com.todesktop.230313mzl4w4u92"), folder: None },
    Target { id: "deepseek_harness", name: "DeepSeek Harness", prefix: None, bundle: Some("com.deepseek.dsh"), folder: None },
    Target { id: "antigravity", name: "Antigravity", prefix: None, bundle: Some("com.google.antigravity"), folder: None },
    Target { id: "gemini", name: "Gemini", prefix: None, bundle: Some("com.google.GeminiMacOS"), folder: None },
    Target { id: "workbuddy", name: "WorkBuddy", prefix: None, bundle: Some("com.tencent.workbuddy.mac"), folder: None },
    Target { id: "grok_bot", name: "Grok Bot", prefix: None, bundle: Some("com.anysphere.sand"), folder: None },
    Target { id: "muse", name: "Muse", prefix: None, bundle: Some("com.meta.endo"), folder: None },
    // 命令行和 Bot 没有可打开的应用：只复制
    Target { id: "ds_bot", name: "DS Bot", prefix: None, bundle: None, folder: None },
    Target { id: "cursor_cli", name: "Cursor 命令行", prefix: None, bundle: None, folder: None },
    Target { id: "codebuddy", name: "CodeBuddy 命令行", prefix: None, bundle: None, folder: None },
];

pub fn target(id: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|target| target.id == id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// 打开新对话并预填
    Prefill,
    /// 复制后打开应用
    Open,
    /// 只复制
    Copy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub method: Method,
    pub url: Option<String>,
    /// 因为太长没法预填
    pub too_long: bool,
}

/// 工作目录只接受绝对路径，不带 .. 和控制字符
pub fn usable_folder(folder: &str) -> bool {
    folder.starts_with('/')
        && !folder.split('/').any(|part| part == "..")
        && !folder.chars().any(char::is_control)
        && folder.len() <= 1024
}

pub fn plan(target: &Target, text: &str, folder: Option<&str>) -> Plan {
    let fallback = if target.bundle.is_some() { Method::Open } else { Method::Copy };
    let Some(prefix) = target.prefix else {
        return Plan { method: fallback, url: None, too_long: false };
    };
    if text.trim().is_empty() || text.chars().count() > PREFILL_CHARS {
        return Plan { method: fallback, url: None, too_long: !text.trim().is_empty() };
    }
    let mut url = format!("{prefix}{}", encode(text));
    if let (Some(param), Some(folder)) = (target.folder, folder.filter(|folder| usable_folder(folder))) {
        url.push_str(param);
        url.push_str(&encode(folder));
    }
    if url.len() > PREFILL_URL_CHARS {
        return Plan { method: fallback, url: None, too_long: true };
    }
    Plan { method: Method::Prefill, url: Some(url), too_long: false }
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 确认内容：目标和全文一起绑定
pub fn place_bytes(target: &Target, text: &str, folder: Option<&str>) -> Vec<u8> {
    let mut out = target.id.as_bytes().to_vec();
    out.push(0);
    out.extend_from_slice(folder.unwrap_or("").as_bytes());
    out.push(0);
    out.extend_from_slice(text.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_unique_and_prefixes_are_fixed_schemes() {
        let mut ids = std::collections::HashSet::new();
        for target in TARGETS {
            assert!(ids.insert(target.id), "{}", target.id);
            if let Some(prefix) = target.prefix {
                assert!(prefix.starts_with("claude://") || prefix.starts_with("codex://"), "{prefix}");
                assert!(prefix.ends_with('='), "{prefix}");
            }
        }
    }

    #[test]
    fn prefill_encodes_everything_into_the_query() {
        let claude = target("claude").unwrap();
        let plan = plan(claude, "继续 & a=b?#\n", None);
        assert_eq!(plan.method, Method::Prefill);
        let url = plan.url.unwrap();
        assert!(url.starts_with("claude://code/new?q="));
        let tail = &url["claude://code/new?q=".len()..];
        assert!(!tail.contains('&') && !tail.contains('=') && !tail.contains('#') && !tail.contains('?'));
        assert!(tail.contains("%26") && tail.contains("%0A"));
    }

    #[test]
    fn folder_is_added_only_when_safe() {
        let codex = target("codex").unwrap();
        let url = plan(codex, "x", Some("/Users/me/项目 a")).url.unwrap();
        assert!(url.ends_with("&path=%2FUsers%2Fme%2F%E9%A1%B9%E7%9B%AE%20a"), "{url}");
        for bad in ["relative/dir", "/a/../b", "/a\nb"] {
            let url = plan(codex, "x", Some(bad)).url.unwrap();
            assert!(!url.contains("&path="), "{bad}");
        }
        let chat = plan(target("claude_chat").unwrap(), "x", Some("/tmp")).url.unwrap();
        assert!(!chat.contains("folder"));
    }

    #[test]
    fn too_long_or_no_link_falls_back() {
        let codex = target("codex").unwrap();
        let long = "字".repeat(PREFILL_CHARS + 1);
        let plan_long = plan(codex, &long, None);
        assert_eq!(plan_long.method, Method::Open);
        assert!(plan_long.too_long);
        assert_eq!(plan(target("cursor").unwrap(), "x", None).method, Method::Open);
        assert_eq!(plan(target("cursor_cli").unwrap(), "x", None).method, Method::Copy);
        assert_eq!(plan(codex, "  ", None).method, Method::Open);
    }

    #[test]
    fn confirm_bytes_bind_target_and_text() {
        let a = place_bytes(target("claude").unwrap(), "x", None);
        let b = place_bytes(target("codex").unwrap(), "x", None);
        assert_ne!(a, b);
        assert_ne!(a, place_bytes(target("claude").unwrap(), "x", Some("/tmp")));
    }
}
