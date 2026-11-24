import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

// 交接目标：界面里的表必须和 Rust 的 TARGETS 一致（名字、放入方式）
const rust = readFileSync(new URL("../crates/core/src/place.rs", import.meta.url), "utf8");
const fromRust = [...rust.matchAll(/Target \{ id: "([a-z_]+)", name: "([^"]+)", prefix: (Some|None)[^,]*, bundle: (Some|None)/g)].map(
  ([, id, name, prefix, bundle]) => ({ id, name, how: prefix === "Some" ? "prefill" : bundle === "Some" ? "open" : "copy" }),
);
assert.ok(fromRust.length >= 10);

const server = await createServer({
  configFile: new URL("./vite.config.ts", import.meta.url).pathname,
  server: { middlewareMode: true },
  appType: "custom",
  logLevel: "error",
});
try {
  const { placeTargets, homeTarget, placeButton, HandoffPage } = await server.ssrLoadModule("/src/HandoffPage.tsx");
  assert.deepEqual(placeTargets.map((item) => ({ ...item })), fromRust);
  // 每个端默认回到自己的应用
  assert.equal(homeTarget("codex").id, "codex");
  assert.equal(homeTarget("claude").id, "claude");
  assert.equal(homeTarget("cursor").id, "cursor");
  assert.equal(homeTarget("cursor_cli").id, "cursor_cli");
  assert.equal(homeTarget("deepseek_harness").id, "deepseek_harness");
  assert.equal(homeTarget("antigravity").id, "antigravity");
  assert.equal(placeButton(homeTarget("codex")), "放入 ChatGPT 新对话");
  assert.equal(placeButton(homeTarget("cursor")), "复制并打开 Cursor");
  assert.equal(placeButton(homeTarget("codex"), 12803), "复制并打开 ChatGPT（超过 8000 字）");
  const rustLimit = Number(/PREFILL_CHARS: usize = ([\d_]+)/.exec(rust)[1].replaceAll("_", ""));
  const { PREFILL_CHARS } = await server.ssrLoadModule("/src/HandoffPage.tsx");
  assert.equal(PREFILL_CHARS, rustLimit);

  // ChatGPT 的对话：推荐项是回到 ChatGPT，而不是 Cursor
  const row = { id: "r1", tool: "codex", title: "测试对话", updated: "", percent: 90, status: "", cwd: null, context: "", source: "jsonl" };
  const html = renderToStaticMarkup(
    createElement(HandoffPage, {
      rows: [row],
      sources: {},
      threshold: 80,
      filter: "all",
      onFilter() {},
      query: "",
      onQuery() {},
      onSettings() {},
      sheet: "",
      previewRequest: 0,
      searchRequest: 0,
    }),
  );
  assert.match(html, /在 ChatGPT 新对话中继续/);
  assert.doesNotMatch(html, /在 Cursor 新对话中继续/);
} finally {
  await server.close();
}
