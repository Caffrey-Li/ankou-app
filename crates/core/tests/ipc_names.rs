use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use wanxiang_core::{Capability, GrantStatus, SummaryMode, ToolId, ToolPresence};

#[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
struct IpcNames {
    tool_id: Vec<String>,
    capability: Vec<String>,
    grant_status: Vec<String>,
    tool_presence: Vec<String>,
    summary_mode: Vec<String>,
}

fn json_name<T: Serialize>(value: T) -> String {
    match serde_json::to_value(value).unwrap() {
        serde_json::Value::String(name) => name,
        other => panic!("枚举应序列化成字符串，实际是 {other}"),
    }
}

fn sorted<T: Serialize + Copy>(items: &[T]) -> Vec<String> {
    let mut names = items.iter().copied().map(json_name).collect::<Vec<_>>();
    names.sort();
    names
}

fn all_tools() -> Vec<ToolId> {
    let tools = vec![
        ToolId::DeepSeekHarness,
        ToolId::DsBot,
        ToolId::Cursor,
        ToolId::Codex,
        ToolId::Claude,
    ];
    for tool in &tools {
        match tool {
            ToolId::DeepSeekHarness
            | ToolId::DsBot
            | ToolId::Cursor
            | ToolId::Codex
            | ToolId::Claude => {}
        }
    }
    tools
}

fn all_capabilities() -> Vec<Capability> {
    let caps = vec![
        Capability::Usage,
        Capability::Conversation,
        Capability::AccountLink,
        Capability::CreateConversation,
        Capability::InsertContent,
        Capability::ProtectedLocation,
        Capability::Cleanup,
        Capability::SpaceScan,
    ];
    for cap in &caps {
        match cap {
            Capability::Usage
            | Capability::Conversation
            | Capability::AccountLink
            | Capability::CreateConversation
            | Capability::InsertContent
            | Capability::ProtectedLocation
            | Capability::Cleanup
            | Capability::SpaceScan => {}
        }
    }
    caps
}

#[test]
fn ipc_names_match_the_ui_contract() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/__fixtures__/ipc-names.json");
    let expected: IpcNames = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let actual = IpcNames {
        tool_id: sorted(&all_tools()),
        capability: sorted(&all_capabilities()),
        grant_status: sorted(&[GrantStatus::Active, GrantStatus::NeedsReauth]),
        tool_presence: sorted(&[
            ToolPresence::Missing,
            ToolPresence::Unauthorized,
            ToolPresence::Authorized,
            ToolPresence::NeedsReauth,
        ]),
        summary_mode: sorted(&[SummaryMode::Ask, SummaryMode::Local, SummaryMode::Ai]),
    };
    assert_eq!(actual, expected);
    assert!(expected.tool_id.iter().any(|name| name == "codex"));
    assert!(expected.tool_id.iter().all(|name| name != "chatgpt"));
}
