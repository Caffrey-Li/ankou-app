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

export function finishSetup() {
  if (!inTauri()) return Promise.resolve();
  return call<void>("complete_setup");
}
