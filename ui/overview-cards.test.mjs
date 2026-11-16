import assert from "node:assert/strict";
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
  const { OverviewTalkCard, OverviewSpaceCard } = await server.ssrLoadModule("/src/OverviewCards.tsx");
  const row = (id, percent, tool = "cursor") => ({
    id, tool, title: `对话${id}`, updated: "", percent, cwd: null, status: "", context: "", source: "",
  });

  // 取上下文占用最高的一条；超过阈值用赭色；另计 60% 以上的其他对话
  const talk = renderToStaticMarkup(
    createElement(OverviewTalkCard, { rows: [row("a", 61), row("b", 84), row("c", 67, "codex"), row("d", null)], threshold: 80, onOpen() {} }),
  );
  assert.match(talk, /<strong class="warn">84<\/strong>/);
  assert.match(talk, /对话b · Cursor/);
  assert.match(talk, /另有 2 条超过 60%/);
  assert.match(talk, /提醒阈值 80%/);

  const unknown = renderToStaticMarkup(createElement(OverviewTalkCard, { rows: [row("x", null)], threshold: 80, onOpen() {} }));
  assert.match(unknown, /对话的上下文比例未知/);
  assert.equal(unknown.includes(">0<"), false);

  const space = renderToStaticMarkup(
    createElement(OverviewSpaceCard, {
      summary: { tool: "DeepSeek Harness", known: 8_200_000_000, protectedBytes: 6_200_000_000, cleanable: 2_000_000_000 },
      thresholdGb: 5,
      onOpen() {},
    }),
  );
  assert.match(space, /<strong>8.2<\/strong><small>GB<\/small>/);
  assert.match(space, /DeepSeek Harness · 阈值 5 GB/);
  assert.match(space, /class="mark over"/);
  assert.match(space, /受保护 6.2 GB/);

  const none = renderToStaticMarkup(createElement(OverviewSpaceCard, { summary: null, thresholdGb: 5, onOpen() {} }));
  assert.match(none, /尚未扫描/);
} finally {
  await server.close();
}
