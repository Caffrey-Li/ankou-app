import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./src/tokens.css", import.meta.url), "utf8").toLowerCase();

const required = [
  "#1d1d1f",
  "#4a6fa0",
  "#b9733f",
  "#6e6e73",
  "#a1a1a6",
  "#e5e5ea",
  "#f5f5f7",
  "#f7f7f8",
  "#ffffff",
  "#f9f9fa",
  "#f4f4f6",
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
