import type { UsageView } from "./api";

export async function loadDevFixture(): Promise<UsageView | null> {
  if (!import.meta.env.DEV) return null;
  const params = new URLSearchParams(window.location.search);
  if (params.get("compare") !== "1" || params.get("fixture") !== "m2") return null;
  const mod = await import("./fixture-m2.json");
  return mod.default as UsageView;
}
