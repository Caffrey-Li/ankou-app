export type ToolId =
  | "deepseek_harness"
  | "ds_bot"
  | "cursor"
  | "codex"
  | "claude"
  | "gemini"
  | "antigravity"
  | "workbuddy"
  | "grok_bot"
  | "muse"
  | "codebuddy"
  | "cursor_cli";
export type CapabilityId =
  | "usage"
  | "conversation"
  | "account_link"
  | "create_conversation"
  | "insert_content"
  | "protected_location"
  | "cleanup"
  | "space_scan";

export type Presence = "missing" | "unauthorized" | "authorized" | "needs_reauth";

export type CapRow = {
  capability: CapabilityId;
  title: string;
  detail: string;
  badge: "未验证" | "暂不提供" | "已实测" | "复制后粘贴";
};

export type ToolTag = {
  label: string;
  capability: CapabilityId;
  badge: "未验证" | "暂不提供" | "已实测" | "复制后粘贴";
};

export type ToolSpec = {
  id: ToolId;
  letter: string;
  name: string;
  /** 这一端覆盖哪些形态（应用、命令行、IDE），在厂商里区分各端时显示。 */
  ends?: string;
  caps: CapRow[];
  tags: ToolTag[];
};


export const toolSpecs: ToolSpec[] = [
  {
    id: "deepseek_harness",
    letter: "H",
    name: "DeepSeek Harness",
    ends: "应用与 dsh 命令行",
    caps: [
      { capability: "usage", title: "用量", detail: "读取本机会话里的调用用量，不含 Bot 工作区", badge: "已实测" },
      { capability: "conversation", title: "对话", detail: "列出本机会话，交接时读取所选对话", badge: "已实测" },
      {
        capability: "account_link",
        title: "DeepSeek 账户 · 余额",
        detail: "用你保存的 API 密钥查询余额",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间",
        detail: "统计本机目录大小，不读取内容",
        badge: "已实测",
      },
      {
        capability: "cleanup",
        title: "清理",
        detail: "每次清理前逐项列出并确认",
        badge: "已实测",
      },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "用量", capability: "usage", badge: "已实测" },
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "DeepSeek 账户 · 余额", capability: "account_link", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "清理", capability: "cleanup", badge: "已实测" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "ds_bot",
    letter: "B",
    name: "DS Bot",
    ends: "Bot 与群聊",
    caps: [
      { capability: "usage", title: "用量", detail: "读取 Bot 本机用量记录", badge: "已实测" },
      {
        capability: "account_link",
        title: "DeepSeek 账户 · 余额",
        detail: "用你保存的 API 密钥查询余额",
        badge: "已实测",
      },
      { capability: "conversation", title: "Bot 与群聊", detail: "列出 Bot 与群聊，交接时读取所选会话", badge: "已实测" },
      { capability: "insert_content", title: "交接方式", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "用量", capability: "usage", badge: "已实测" },
      { label: "DeepSeek 账户 · 余额", capability: "account_link", badge: "已实测" },
      { label: "Bot 与群聊", capability: "conversation", badge: "已实测" },
      { label: "交接方式", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "cursor",
    letter: "C",
    name: "Cursor",
    ends: "桌面应用（编辑器）；Cursor BYOK 本机无记录",
    caps: [
      {
        capability: "conversation",
        title: "对话",
        detail: "列出对话与上下文占用，交接时读取所选对话 · 只读",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计本机目录大小，不读取内容",
        badge: "已实测",
      },
      {
        capability: "cleanup",
        title: "清理",
        detail: "Cursor 的缓存清理暂不提供",
        badge: "暂不提供",
      },
      {
        capability: "usage",
        title: "用量与额度",
        detail: "个人账户的可靠来源尚未确认",
        badge: "暂不提供",
      },
      {
        capability: "create_conversation",
        title: "放入新对话",
        detail: "实测 Cursor 的 Agents 窗口不接收深链，交接改为复制后粘贴",
        badge: "暂不提供",
      },
      {
        capability: "protected_location",
        title: "访问受系统保护的位置",
        detail: "当前读取的位置不受系统保护，无需授予",
        badge: "暂不提供",
      },
    ],
    tags: [
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "用量", capability: "usage", badge: "暂不提供" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "codex",
    letter: "Cx",
    name: "ChatGPT",
    ends: "ChatGPT 桌面应用、Codex 命令行与 IDE 插件共用记录",
    caps: [
      {
        capability: "usage",
        title: "用量与额度窗口",
        detail: "读取本机会话记录里的用量与额度",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计本机目录大小，不读取内容",
        badge: "已实测",
      },
      { capability: "conversation", title: "对话", detail: "列出本机会话，交接时读取所选对话", badge: "已实测" },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "用量与额度窗口", capability: "usage", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "claude",
    letter: "Cl",
    name: "Claude",
    ends: "桌面应用（额度）与 Claude Code（对话、用量）",
    caps: [
      {
        capability: "usage",
        title: "用量与额度",
        detail: "读取本机会话记录里的用量，以及桌面应用记下的额度百分比",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计桌面应用目录大小，不读取内容",
        badge: "已实测",
      },
      { capability: "conversation", title: "对话", detail: "列出本机会话，交接时读取所选对话", badge: "已实测" },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "用量与额度", capability: "usage", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "antigravity",
    letter: "Ag",
    name: "Antigravity",
    ends: "应用与 agy 命令行",
    caps: [
      {
        capability: "usage",
        title: "用量",
        detail: "读取本机对话步骤里记下的 token 用量",
        badge: "已实测",
      },
      {
        capability: "conversation",
        title: "对话",
        detail: "列出本机对话与上下文占用，交接时读取所选对话 · 只读",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计本机数据目录大小，不读取内容",
        badge: "已实测",
      },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "用量", capability: "usage", badge: "已实测" },
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "gemini",
    letter: "G",
    name: "Gemini",
    ends: "桌面应用，对话在云端",
    caps: [
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计桌面应用目录大小，不读取内容",
        badge: "已实测",
      },
      {
        capability: "conversation",
        title: "对话",
        detail: "对话保存在云端，本机没有可读的记录",
        badge: "暂不提供",
      },
      {
        capability: "usage",
        title: "用量与额度",
        detail: "本机没有用量记录",
        badge: "暂不提供",
      },
    ],
    tags: [
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "对话", capability: "conversation", badge: "暂不提供" },
      { label: "用量", capability: "usage", badge: "暂不提供" },
    ],
  },
  {
    id: "workbuddy",
    letter: "W",
    name: "WorkBuddy",
    ends: "桌面应用",
    caps: [
      {
        capability: "conversation",
        title: "对话",
        detail: "列出本机会话与上下文占用，交接时读取所选会话 · 只读",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计本机数据目录大小，不读取内容",
        badge: "已实测",
      },
      {
        capability: "usage",
        title: "用量与积分",
        detail: "本机没有 token 记录",
        badge: "暂不提供",
      },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "用量", capability: "usage", badge: "暂不提供" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "grok_bot",
    letter: "Gk",
    name: "Grok Bot",
    ends: "桌面应用",
    caps: [
      {
        capability: "usage",
        title: "每周额度",
        detail: "读取应用缓存里的每周额度百分比与重置时间",
        badge: "已实测",
      },
      {
        capability: "conversation",
        title: "会话",
        detail: "列出应用缓存里的会话，交接时读取所选会话缓存的最近消息 · 只读",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计应用数据目录大小，不读取内容",
        badge: "已实测",
      },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "每周额度", capability: "usage", badge: "已实测" },
      { label: "会话", capability: "conversation", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "muse",
    letter: "M",
    name: "Muse",
    ends: "桌面应用，对话在云端",
    caps: [
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计桌面应用目录大小，不读取内容",
        badge: "已实测",
      },
      {
        capability: "conversation",
        title: "对话",
        detail: "对话保存在云端，本机没有可读的记录",
        badge: "暂不提供",
      },
      {
        capability: "usage",
        title: "用量",
        detail: "本机没有用量记录",
        badge: "暂不提供",
      },
    ],
    tags: [
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "对话", capability: "conversation", badge: "暂不提供" },
      { label: "用量", capability: "usage", badge: "暂不提供" },
    ],
  },
  {
    id: "cursor_cli",
    letter: "C>",
    name: "Cursor 命令行",
    ends: "cursor-agent 命令行",
    caps: [
      {
        capability: "conversation",
        title: "对话",
        detail: "列出命令行对话，交接时读取所选对话的文字 · 只读",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计 ~/.cursor 目录大小，不读取内容",
        badge: "已实测",
      },
      { capability: "usage", title: "用量", detail: "本机没有用量记录", badge: "暂不提供" },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "用量", capability: "usage", badge: "暂不提供" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
  {
    id: "codebuddy",
    letter: "CB",
    name: "CodeBuddy",
    ends: "命令行；CodeBuddy CN 应用本机无记录",
    caps: [
      {
        capability: "conversation",
        title: "对话",
        detail: "列出命令行会话，交接时读取所选会话 · 只读",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计 ~/.codebuddy 目录大小，不读取内容",
        badge: "已实测",
      },
      { capability: "usage", title: "用量", detail: "本机没有 token 记录", badge: "暂不提供" },
      { capability: "insert_content", title: "放入", detail: "交接时复制摘要，由你手动粘贴", badge: "复制后粘贴" },
    ],
    tags: [
      { label: "对话", capability: "conversation", badge: "已实测" },
      { label: "空间", capability: "space_scan", badge: "已实测" },
      { label: "用量", capability: "usage", badge: "暂不提供" },
      { label: "放入", capability: "insert_content", badge: "复制后粘贴" },
    ],
  },
];

export type VendorId = "deepseek" | "cursor" | "openai" | "anthropic" | "google" | "tencent" | "grok" | "meta";

export type Vendor = {
  id: VendorId;
  name: string;
  /** 厂商图标借用哪一端的应用图标 */
  icon: ToolId;
  members: ToolId[];
};

/** 按厂商合并：一个厂商一行，点进去看各端。 */
export const vendors: Vendor[] = [
  { id: "deepseek", name: "DeepSeek", icon: "deepseek_harness", members: ["deepseek_harness", "ds_bot"] },
  { id: "cursor", name: "Cursor", icon: "cursor", members: ["cursor", "cursor_cli"] },
  { id: "openai", name: "ChatGPT", icon: "codex", members: ["codex"] },
  { id: "anthropic", name: "Claude", icon: "claude", members: ["claude"] },
  { id: "google", name: "Google", icon: "gemini", members: ["antigravity", "gemini"] },
  { id: "tencent", name: "腾讯", icon: "workbuddy", members: ["workbuddy", "codebuddy"] },
  { id: "grok", name: "Grok Bot", icon: "grok_bot", members: ["grok_bot"] },
  { id: "meta", name: "Muse", icon: "muse", members: ["muse"] },
];

export function vendorOf(tool: string): Vendor | undefined {
  return vendors.find((vendor) => vendor.members.includes(tool as ToolId));
}

export function specOf(tool: string): ToolSpec | undefined {
  return toolSpecs.find((spec) => spec.id === tool);
}

export function presenceLabel(presence: Presence) {
  switch (presence) {
    case "unauthorized":
      return "未授权";
    case "authorized":
      return "已授权";
    case "needs_reauth":
      return "需要重新授权";
    default:
      return "未发现";
  }
}
