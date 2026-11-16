import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

const server = await createServer({
  configFile: new URL("./vite.config.ts", import.meta.url).pathname,
  server: { middlewareMode: true },
  appType: "custom",
  logLevel: "error",
});

try {
  const { presetSpan, orderSpan, spanLabel, daysBetween, monthCells, DateRangePopover } = await server.ssrLoadModule(
    "/src/DateRange.tsx",
  );
  const { RangeControl } = await server.ssrLoadModule("/src/UsagePage.tsx");

  // 切到「自定义」时沿用当前范围，不再两端都是今天
  assert.deepEqual(presetSpan("month", "2026-11-12"), { start: "2026-11-01", end: "2026-11-12" });
  assert.deepEqual(presetSpan("7", "2026-11-12"), { start: "2026-11-06", end: "2026-11-12" });
  assert.deepEqual(presetSpan("30", "2026-11-12"), { start: "2026-10-14", end: "2026-11-12" });
  assert.deepEqual(presetSpan("today", "2026-11-12"), { start: "2026-11-12", end: "2026-11-12" });

  // 先后颠倒自动对调，不能晚于今天
  assert.deepEqual(orderSpan("2026-11-09", "2026-11-03", "2026-11-12"), { start: "2026-11-03", end: "2026-11-09" });
  assert.deepEqual(orderSpan("2026-11-03", "2026-11-20", "2026-11-12"), { start: "2026-11-03", end: "2026-11-12" });

  assert.equal(spanLabel("2026-11-01", "2026-11-12"), "11月1日 – 11月12日");
  assert.equal(spanLabel("2025-12-30", "2026-01-02"), "2025年12月30日 – 2026年1月2日");
  assert.equal(daysBetween("2026-11-01", "2026-11-12"), 12);

  // 月历从周一开始：2026-11-01 是周日
  const cells = monthCells(2026, 10);
  assert.equal(cells.indexOf("2026-11-01"), 6);
  assert.equal(cells.length % 7, 0);

  const popover = renderToStaticMarkup(
    createElement(DateRangePopover, {
      start: "2026-11-01",
      end: "2026-11-12",
      today: "2026-11-12",
      onCancel() {},
      onApply() {},
    }),
  );
  assert.match(popover, /role="dialog"/);
  assert.match(popover, /data-day="2026-11-13" tabindex="-1" disabled=""/);
  assert.match(popover, /11月1日 – 11月12日 · 共 12 天/);
  assert.equal(popover.includes('type="date"'), false);

  const custom = renderToStaticMarkup(
    createElement(RangeControl, {
      range: "custom",
      start: "2026-11-01",
      end: "2026-11-12",
      today: "2026-11-12",
      onRange() {},
      onCustom() {},
    }),
  );
  assert.match(custom, /date-range-pill/);
  assert.match(custom, /11月1日 – 11月12日/);
  assert.equal(custom.includes('type="date"'), false);
} finally {
  await server.close();
}

const css = readFileSync(new URL("./src/date-range.css", import.meta.url), "utf8");
assert.match(css, /border-radius:\s*var\(--radius-popover\)/);
assert.match(css, /\.date-cell\.edge button[^{]*\{[^}]*background:\s*var\(--color-mist\)/);
