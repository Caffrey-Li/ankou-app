import { toolSpecs, type CapabilityId, type Presence, type ToolId } from "./catalog";

export type SummaryMode = "ask" | "local" | "ai";
export type GrantStatus = "active" | "needs_reauth";

export type Settings = {
  retention_months: number;
  launch_at_login: boolean;
  notifications: boolean;
  context_threshold_percent: number;
  space_threshold_gb: number;
  refresh_on_launch: boolean;
  periodic_refresh: boolean;
  summary_mode: SummaryMode;
  menu_bar_icon: boolean;
  close_to_tray: boolean;
  setup_complete: boolean;
  codex_app_server: boolean;
};

export type StoredGrant = {
  id: number;
  tool: ToolId;
  capability: CapabilityId;
  pattern: string;
  status: GrantStatus;
};

export type ToolSnapshot = {
  tool: ToolId;
  installed: boolean;
  presence: Presence;
};

export type Snapshot = {
  settings: Settings;
  tools: ToolSnapshot[];
  grants: StoredGrant[];
  keySaved: boolean;
};

export const defaultSettings: Settings = {
  retention_months: 12,
  launch_at_login: false,
  notifications: false,
  context_threshold_percent: 80,
  space_threshold_gb: 5,
  refresh_on_launch: true,
  periodic_refresh: true,
  summary_mode: "ask",
  menu_bar_icon: true,
  close_to_tray: true,
  setup_complete: false,
  codex_app_server: false,
};

export function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function fallbackSnapshot(): Snapshot {
  return {
    settings: defaultSettings,
    tools: toolSpecs.map((tool) => ({
      tool: tool.id,
      installed: false,
      presence: "missing",
    })),
    grants: [],
    keySaved: false,
  };
}

async function call<T>(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function loadSnapshot(): Promise<Snapshot> {
  if (!inTauri()) return fallbackSnapshot();
  const [settings, tools, grants, keySaved] = await Promise.all([
    call<Settings>("get_settings"),
    call<ToolSnapshot[]>("discover_tools"),
    call<StoredGrant[]>("list_grants"),
    call<boolean>("deepseek_key_saved"),
  ]);
  return { settings, tools, grants, keySaved };
}

export function saveSettings(settings: Settings) {
  if (!inTauri()) return Promise.resolve();
  return call<void>("set_settings", { settings });
}

export function saveKey(key: string) {
  if (!inTauri()) return Promise.resolve();
  return call<void>("save_deepseek_key", { key });
}

export function deleteKey() {
  if (!inTauri()) return Promise.resolve();
  return call<void>("delete_deepseek_key");
}

export function clearHistory() {
  if (!inTauri()) return Promise.resolve();
  return call<void>("clear_history");
}

export function revokeGrant(id: number) {
  if (!inTauri()) return Promise.resolve();
  return call<void>("revoke", { id });
}

export function setCapability(kind: string, on: boolean) {
  if (!inTauri()) return Promise.resolve();
  return call<void>("set_capability", { kind, on });
}

export type UsageCard = {
  name: string;
  value: string;
  note: string;
  percent: number | null;
  reset: string | null;
  link: string | null;
};

export type UsageDay = {
  label: string;
  codex: number | null;
  bot: number | null;
};

export type UsageDetail = {
  name: string;
  tokens: string;
  calls: string;
  fee: string;
  source: string;
};

export type UsageView = {
  stamp: string;
  banner: string[];
  cards: UsageCard[];
  days: UsageDay[];
  details: UsageDetail[];
  caption: string;
  empty_note: string;
  range_label: string;
  total: string;
  total_reason: string;
};

function monthSpan(date = new Date()) {
  return `${date.getMonth() + 1} 月 1 日 – ${date.getDate()} 日`;
}

export const emptyUsage: UsageView = {
  stamp: "—",
  banner: [
    "Cursor 用量暂不提供，未计入",
    "Harness 用量暂不提供，未计入",
    "Codex 未授权，未计入",
    "DS Bot 未授权，未计入",
  ],
  cards: [
    { name: "Codex", value: "—", note: "未授权", percent: null, reset: null, link: "去设置授权" },
    { name: "Codex · 窗口", value: "—", note: "未授权", percent: null, reset: null, link: "去设置授权" },
    { name: "DeepSeek 账户", value: "—", note: "未保存密钥", percent: null, reset: null, link: null },
    { name: "Cursor", value: "—", note: "暂不提供", percent: null, reset: null, link: null },
  ],
  days: [],
  details: [
    { name: "Codex", tokens: "未授权", calls: "未授权", fee: "暂不提供", source: "未授权" },
    { name: "DeepSeek Harness", tokens: "暂不提供", calls: "暂不提供", fee: "暂不提供", source: "暂不提供" },
    { name: "DS Bot", tokens: "未授权", calls: "未授权", fee: "暂不提供", source: "未授权" },
    { name: "Cursor", tokens: "暂不提供", calls: "暂不提供", fee: "暂不提供", source: "暂不提供" },
  ],
  caption: "按各工具自身周期，不受上方日期影响",
  empty_note: "更早没有本机记录，不补算",
  range_label: monthSpan(),
  total: "—",
  total_reason: "未授权",
};

export function loadUsage(range = "month", start?: string, end?: string) {
  if (!inTauri()) return Promise.resolve(emptyUsage);
  return call<UsageView>("get_usage", { range, start, end });
}

export function refreshUsage() {
  if (!inTauri()) return Promise.resolve(false);
  return call<boolean>("refresh_usage");
}

export function finishSetup() {
  if (!inTauri()) return Promise.resolve();
  return call<void>("complete_setup");
}
