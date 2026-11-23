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
  const { resolvedTheme } = await server.ssrLoadModule("/src/theme.ts");
  assert.equal(resolvedTheme("light", true), "light");
  assert.equal(resolvedTheme("dark", false), "dark");
  assert.equal(resolvedTheme("system", true), "dark");
  assert.equal(resolvedTheme("system", false), "light");

  const { fallbackSnapshot, emptyUsage } = await server.ssrLoadModule("/src/api.ts");
  const { toolMarks, vendorMarks } = await server.ssrLoadModule("/src/tool-status.tsx");
  const snapshot = fallbackSnapshot();
  const tools = snapshot.tools.map((tool) => ({
    ...tool,
    presence: tool.tool === "codex" || tool.tool === "claude" ? "authorized" : tool.presence,
  }));
  const marks = vendorMarks(toolMarks(tools, [], false));

  const usage = {
    ...emptyUsage,
    total: "3,264,494,995",
    cards: [
      { name: "ChatGPT · 5 小时窗口", value: "32%", note: "本机记录", percent: 32, reset: "12:07 重置", link: null },
      { name: "DeepSeek 账户", value: "¥8.28", note: "刚刚", percent: null, reset: null, link: null },
    ],
    details: [{ name: "Claude", tokens: "2,136,716,027", calls: "4,530", fee: "不提供", source: "本机记录" }],
    days: [
      { label: "11/22", codex: 10, bot: 0, harness: 0, claude: 30, antigravity: 0 },
      { label: "11/23", codex: 20, bot: 5, harness: 5, claude: 60, antigravity: 1 },
    ],
  };

  // 总览：8 个厂商方格；有百分比额度的画圆环；待处理列出最满的对话
  const { Console } = await server.ssrLoadModule("/src/Console.tsx");
  const hottest = { id: "a", tool: "codex", title: "Reviewed Codex session", updated: "", percent: 95, cwd: null, status: "", context: "", source: "" };
  const overview = renderToStaticMarkup(
    createElement(Console, {
      marks,
      usage,
      items: [],
      hottest,
      space: [{ vendor: "anthropic", bytes: 17.9e9, cleanable: 0 }],
      thresholdGb: 5,
      onVendor() {},
      onGo() {},
      onHandoff() {},
      onUsage() {},
    }),
  );
  assert.equal((overview.match(/class="c-tile"/g) ?? []).length, 8);
  assert.match(overview, /32%/);
  assert.match(overview, /32\.6 亿/);
  assert.match(overview, /对话上下文 95%/);
  assert.match(overview, /Claude <small>17\.9 GB<\/small>/);

  // 工具页：左边 8 个厂商，右边 Claude 的 token、对话与授权
  const { ToolsPage } = await server.ssrLoadModule("/src/ToolsPage.tsx");
  const tools_ = renderToStaticMarkup(
    createElement(ToolsPage, {
      marks,
      selected: "anthropic",
      onSelect() {},
      usage,
      rows: [{ ...hottest, tool: "claude", title: "重构缓存层", percent: 88 }],
      threshold: 80,
      snapshot,
      onChange() {},
      onReload() {},
      space: null,
      onHandoff() {},
      onClean() {},
    }),
  );
  assert.equal((tools_.match(/class="t-rail-item"/g) ?? []).length, 8);
  assert.match(tools_, /aria-current="page"[^>]*>[\s\S]*?Claude/);
  assert.match(tools_, /21\.4 亿/);
  assert.match(tools_, /重构缓存层/);
  assert.match(tools_, /授权 · 只读/);
  assert.match(tools_, /id="tool-anthropic"/);

  // 首次启动：一个厂商一张卡
  const { SetupPage } = await server.ssrLoadModule("/src/SetupPage.tsx");
  const setup = renderToStaticMarkup(createElement(SetupPage, { snapshot, onDone() {}, onReload() {} }));
  assert.equal((setup.match(/class="setup-vendor( on| missing)?"/g) ?? []).length, 8);
  assert.match(setup, /全部接入/);
} finally {
  await server.close();
}
