import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./src/tokens.css", import.meta.url), "utf8").toLowerCase();

const required = [
  // 浅色
  "#1d1d1f",
  "#2f5d9a",
  "#b5652a",
  "#5f6370",
  "#f2f3f5",
  "#ffffff",
  // 深色
  'html[data-theme="dark"]',
  "#ececf1",
  "#7fa6e0",
  "#e39a5e",
  "#a1a4ad",
  "#111317",
  "#1a1d23",
  "--radius-window: 24px",
  "--radius-popover: 22px",
  "--radius-card: 16px",
  "--radius-inset: 12px",
  "--radius-control: 999px",
  "--space-1: 4px",
  "--space-2: 8px",
  "--space-3: 12px",
  "--space-4: 16px",
  "--space-5: 20px",
  "--space-6: 24px",
  "--space-7: 28px",
  "--font-size-display: 28px",
  "--font-size-title: 22px",
  "--font-size-section: 15px",
  "--font-size-body: 13px",
  "--font-size-caption: 12px",
  "--font-size-figure: 40px",
  "pingfang sc",
  "microsoft yahei ui",
  "tabular-nums",
];

for (const token of required) {
  assert.ok(css.includes(token), `missing token ${token}`);
}
