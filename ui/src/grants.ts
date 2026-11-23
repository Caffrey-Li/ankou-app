import type { CapabilityId, CapRow, ToolId } from "./catalog";

type GrantLike = { tool: ToolId; capability: CapabilityId; status: string };

export function measuredKind(tool: ToolId, capability: CapabilityId, title: string) {
  if (tool === "ds_bot" && capability === "usage" && title === "用量") return "ds_bot_usage";
  if (tool === "codex" && capability === "usage" && title === "用量与额度窗口") return "codex_usage";
  if (capability === "account_link" && title === "DeepSeek 账户 · 余额") return "deepseek_balance";
  if (tool === "deepseek_harness" && capability === "usage" && title === "用量") return "harness_usage";
  if (tool === "deepseek_harness" && capability === "conversation" && title === "对话") return "harness_conversation";
  if (tool === "deepseek_harness" && capability === "space_scan" && title === "空间") return "harness_space";
  if (tool === "deepseek_harness" && capability === "cleanup" && title === "清理") return "harness_cleanup";
  if (tool === "ds_bot" && capability === "conversation" && title === "Bot 与群聊") return "ds_bot_conversation";
  if (tool === "codex" && capability === "conversation" && title === "对话") return "codex_conversation";
  if (tool === "cursor" && capability === "conversation" && title === "对话") return "cursor_conversation";
  if (tool === "cursor" && capability === "space_scan" && title === "空间扫描") return "cursor_space";
  if (tool === "codex" && capability === "space_scan" && title === "空间扫描") return "codex_space";
  if (tool === "claude" && capability === "usage") return "claude_usage";
  if (tool === "claude" && capability === "conversation") return "claude_conversation";
  if (tool === "claude" && capability === "space_scan") return "claude_space";
  return null;
}

/** 只有已实测的能力才能勾选；未验证、暂不提供一律禁用。 */
export function grantableKind(tool: ToolId, cap: CapRow) {
  if (cap.badge !== "已实测") return null;
  return measuredKind(tool, cap.capability, cap.title);
}

export function capChecked(cap: CapRow, grants: GrantLike[]) {
  return grants.some((grant) => grant.capability === cap.capability && grant.status === "active");
}

export function grantedCount(tool: ToolId, caps: CapRow[], grants: GrantLike[]) {
  const own = grants.filter((grant) => grant.tool === tool);
  return caps.filter((cap) => grantableKind(tool, cap) && capChecked(cap, own)).length;
}
