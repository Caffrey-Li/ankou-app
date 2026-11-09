export type ToolId = "deepseek_harness" | "ds_bot" | "cursor" | "codex";
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
  badge: "未验证" | "暂不提供" | "已实测";
};

export type ToolTag = {
  label: string;
  badge: "未验证" | "暂不提供" | "已实测";
};

export type ToolSpec = {
  id: ToolId;
  letter: string;
  name: string;
  caps: CapRow[];
  tags: ToolTag[];
};

const unverified = "未验证" as const;

export const toolSpecs: ToolSpec[] = [
  {
    id: "deepseek_harness",
    letter: "H",
    name: "DeepSeek Harness",
    caps: [
      { capability: "usage", title: "用量", detail: "读取本地用量记录", badge: unverified },
      { capability: "conversation", title: "对话", detail: "读取对话以准备交接", badge: unverified },
      {
        capability: "account_link",
        title: "DeepSeek 账户 · 余额",
        detail: "已实测 2026-11-09",
        badge: "已实测",
      },
      {
        capability: "space_scan",
        title: "空间",
        detail: "统计本机目录大小，不读取内容",
        badge: unverified,
      },
      {
        capability: "cleanup",
        title: "清理",
        detail: "每次清理前逐项列出并确认",
        badge: unverified,
      },
      { capability: "insert_content", title: "放入", detail: "尚未实测", badge: unverified },
    ],
    tags: [
      { label: "用量", badge: unverified },
      { label: "对话", badge: unverified },
      { label: "DeepSeek 账户 · 余额", badge: "已实测" },
      { label: "空间", badge: unverified },
      { label: "清理", badge: unverified },
      { label: "放入", badge: unverified },
    ],
  },
  {
    id: "ds_bot",
    letter: "B",
    name: "DS Bot",
    caps: [
      { capability: "usage", title: "用量", detail: "已实测 2026-11-09", badge: "已实测" },
      {
        capability: "account_link",
        title: "DeepSeek 账户 · 余额",
        detail: "已实测 2026-11-09",
        badge: "已实测",
      },
      { capability: "conversation", title: "Bot 与群聊", detail: "尚未实测", badge: unverified },
      { capability: "insert_content", title: "交接方式", detail: "尚未实测", badge: unverified },
    ],
    tags: [
      { label: "用量", badge: "已实测" },
      { label: "DeepSeek 账户 · 余额", badge: "已实测" },
      { label: "Bot 与群聊", badge: unverified },
      { label: "交接方式", badge: unverified },
    ],
  },
  {
    id: "cursor",
    letter: "C",
    name: "Cursor",
    caps: [
      {
        capability: "conversation",
        title: "对话列表与上下文占用",
        detail: "标题、更新时间、上下文比例 · 只读",
        badge: unverified,
      },
      {
        capability: "conversation",
        title: "读取对话内容",
        detail: "仅在你发起交接时读取所选对话",
        badge: unverified,
      },
      {
        capability: "space_scan",
        title: "空间扫描",
        detail: "统计本机目录大小，不读取内容",
        badge: unverified,
      },
      {
        capability: "cleanup",
        title: "清理",
        detail: "每次清理前逐项列出并确认",
        badge: unverified,
      },
      {
        capability: "usage",
        title: "用量与额度",
        detail: "个人账户的可靠来源尚未确认",
        badge: "暂不提供",
      },
      {
        capability: "insert_content",
        title: "放入新对话",
        detail: "官方深链预填，尚未实测；验证前改用复制",
        badge: unverified,
      },
      {
        capability: "protected_location",
        title: "访问受系统保护的位置",
        detail: "macOS 需在「隐私与安全性」中授予",
        badge: unverified,
      },
    ],
    tags: [
      { label: "对话列表", badge: unverified },
      { label: "读取内容", badge: unverified },
      { label: "空间", badge: unverified },
      { label: "用量", badge: "暂不提供" },
      { label: "放入", badge: unverified },
    ],
  },
  {
    id: "codex",
    letter: "Cx",
    name: "Codex",
    caps: [
      {
        capability: "usage",
        title: "用量与额度窗口",
        detail: "已实测 2026-11-09",
        badge: "已实测",
      },
      { capability: "conversation", title: "对话", detail: "尚未实测", badge: unverified },
      { capability: "insert_content", title: "放入", detail: "尚未实测", badge: unverified },
    ],
    tags: [
      { label: "用量与额度窗口", badge: "已实测" },
      { label: "对话", badge: unverified },
      { label: "放入", badge: unverified },
    ],
  },
];

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
