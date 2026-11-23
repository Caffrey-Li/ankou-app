import type { ConversationRow, UsageCard, UsageView } from "./api";
import { vendors, vendorOf, type Vendor, type VendorId } from "./catalog";
import { groupDetails } from "./UsagePage";

/** 额度卡属于哪个厂商：按卡名开头判断。 */
const cardPrefix: Partial<Record<VendorId, string[]>> = {
  deepseek: ["DeepSeek"],
  cursor: ["Cursor"],
  openai: ["ChatGPT"],
  anthropic: ["Claude"],
  grok: ["Grok Bot"],
};

export function vendorCards(usage: UsageView, vendor: VendorId): UsageCard[] {
  const prefixes = cardPrefix[vendor] ?? [];
  return usage.cards.filter((card) => prefixes.some((prefix) => card.name.startsWith(prefix)));
}

/** 「1,234,567」或数字 → 「123.5 万」「21.4 亿」；不是数字时原样返回。 */
export function cnCount(value: string | number) {
  const number = typeof value === "number" ? value : Number(String(value).replace(/,/g, ""));
  if (!Number.isFinite(number) || String(value).trim() === "") return String(value);
  if (number >= 1e8) return `${(number / 1e8).toFixed(1)} 亿`;
  if (number >= 1e4) return `${(number / 1e4).toFixed(number >= 1e6 ? 0 : 1)} 万`;
  return number.toLocaleString("en-US");
}

export type VendorHeadline =
  | { kind: "ring"; percent: number; label: string; sub: string }
  | { kind: "value"; value: string; label: string; sub: string };

/** 厂商方格里最重要的一个数：有百分比额度就画圆环，其次余额，再其次本月 token。 */
export function vendorHeadline(usage: UsageView, vendor: Vendor): VendorHeadline {
  const cards = vendorCards(usage, vendor.id);
  const ring = cards.find((card) => card.percent != null);
  if (ring && ring.percent != null) {
    return {
      kind: "ring",
      percent: ring.percent,
      label: ring.name.split(" · ")[1] ?? "额度",
      sub: ring.reset ?? ring.note,
    };
  }
  const balance = cards.find((card) => card.name === "DeepSeek 账户");
  if (balance && balance.value !== "—") {
    return { kind: "value", value: balance.value, label: "余额", sub: balance.note };
  }
  const group = groupDetails(usage.details).find((item) => item.id === vendor.id);
  const tokens = group?.row.tokens ?? "";
  if (/^[\d,]+$/.test(tokens) && tokens !== "0") {
    return { kind: "value", value: cnCount(tokens), label: "本月 token", sub: `${group?.row.calls ?? "—"} 次调用` };
  }
  if (balance) return { kind: "value", value: "—", label: "余额", sub: balance.note };
  return { kind: "value", value: "—", label: tokens && tokens !== "—" ? tokens : "暂不提供", sub: "" };
}

/** 对话属于哪个厂商。 */
export function vendorOfRow(row: ConversationRow) {
  return vendorOf(row.tool);
}

export function vendorRows(rows: ConversationRow[], vendor: VendorId) {
  return rows.filter((row) => vendorOfRow(row)?.id === vendor);
}

/** 本月用量图：每天按厂商堆叠。 */
export type StackDay = { label: string; parts: { vendor: VendorId; value: number }[]; total: number };

export function stackDays(usage: UsageView): StackDay[] {
  return usage.days.map((day) => {
    const parts: { vendor: VendorId; value: number }[] = [
      { vendor: "anthropic", value: day.claude ?? 0 },
      { vendor: "openai", value: day.codex ?? 0 },
      { vendor: "deepseek", value: (day.bot ?? 0) + (day.harness ?? 0) },
      { vendor: "google", value: day.antigravity ?? 0 },
    ];
    return { label: day.label, parts, total: parts.reduce((sum, part) => sum + part.value, 0) };
  });
}

export const chartColor: Partial<Record<VendorId, string>> = {
  anthropic: "var(--chart-1)",
  openai: "var(--chart-2)",
  deepseek: "var(--chart-3)",
  google: "var(--chart-4)",
};

export function vendorName(id: VendorId) {
  return vendors.find((vendor) => vendor.id === id)?.name ?? id;
}

/** 各端扫描结果按厂商合计。 */
export function spaceTotals(
  scans: Record<string, { protected_bytes: number; cleanable_bytes: number; cancelled?: boolean } | null>,
) {
  const totals = new Map<VendorId, { vendor: VendorId; bytes: number; cleanable: number }>();
  for (const [tool, scan] of Object.entries(scans)) {
    if (!scan || scan.cancelled) continue;
    const vendor = vendorOf(tool);
    if (!vendor) continue;
    const item = totals.get(vendor.id) ?? { vendor: vendor.id, bytes: 0, cleanable: 0 };
    item.bytes += scan.protected_bytes + scan.cleanable_bytes;
    item.cleanable += scan.cleanable_bytes;
    totals.set(vendor.id, item);
  }
  return [...totals.values()];
}
