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
  const { toolMarks, toolTone, vendorMarks, SidebarTools, OverviewToolRows } = await server.ssrLoadModule(
    "/src/tool-status.tsx",
  );
  const tools = ["deepseek_harness", "ds_bot", "cursor", "codex", "claude"].map((tool) => ({
    tool,
    installed: true,
    presence: "missing",
  }));
  const details = [
    { name: "DeepSeek Harness", tokens: "暂不提供", calls: "暂不提供", fee: "不提供", source: "—" },
    { name: "DS Bot", tokens: "需要重新授权", calls: "需要重新授权", fee: "不提供", source: "需要重新授权" },
    { name: "Cursor", tokens: "未授权", calls: "未授权", fee: "不提供", source: "未授权" },
    { name: "ChatGPT", tokens: "12", calls: "1", fee: "不提供", source: "本机记录" },
    { name: "Claude", tokens: "未授权", calls: "未授权", fee: "不提供", source: "未授权" },
  ];
  const perTool = toolMarks(tools, details, true);
  // 每一端各自的状态仍然按工具算（12 端）
  assert.equal(perTool.length, 12);
  const marks = vendorMarks(perTool);
  const side = renderToStaticMarkup(createElement(SidebarTools, { marks }));
  const rows = renderToStaticMarkup(createElement(OverviewToolRows, { marks, onOpen() {} }));
  const sideStatus = [...side.matchAll(/aria-label="([^"]+)"/g)].map((match) => match[1]);
  const sideDot = [...side.matchAll(/tool-dot ([a-z_]+)/g)].map((match) => match[1]);
  const rowStatus = [...rows.matchAll(/<i class="dot ([a-z_]+) [a-z]+"><\/i>([^<]+)/g)].map((match) => match[2]);
  const rowDot = [...rows.matchAll(/<i class="dot ([a-z_]+) [a-z]+"><\/i>/g)].map((match) => match[1]);
  const allowed = ["未发现", "未授权", "正常", "需要处理", "数据过时", "需要重新授权"];

  assert.deepEqual(sideStatus, rowStatus);
  // 按厂商合并：DeepSeek（Harness + DS Bot）、Cursor（应用 + 命令行）、ChatGPT、Claude、Google、腾讯、Grok Bot、Muse
  assert.deepEqual(
    marks.map((mark) => mark.name),
    ["DeepSeek", "Cursor", "ChatGPT", "Claude", "Google", "腾讯", "Grok Bot", "Muse"],
  );
  // 厂商的状态取各端里最需要注意的：DS Bot 授权失效，DeepSeek 一行就提示重新授权
  assert.deepEqual(sideStatus, ["需要重新授权", "未授权", "正常", "未授权", "未发现", "未发现", "未发现", "未发现"]);
  assert.deepEqual(rowDot, [
    "needs_reauth",
    "unauthorized",
    "authorized",
    "unauthorized",
    "missing",
    "missing",
    "missing",
    "missing",
  ]);
  // 侧栏点按 22:57：正常 = 雾蓝，需要处理 = 赭色，未连接或过时 = 灰
  assert.deepEqual(sideDot, ["attention", "quiet", "ok", "quiet", "quiet", "quiet", "quiet", "quiet"]);
  const toned = toolMarks(tools, details, true, new Set(["deepseek_harness"]), new Set(["codex"]));
  assert.deepEqual(toned.map((mark) => mark.tone), [
    "attention",
    "attention",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
    "quiet",
  ]);
  assert.equal(toolTone("unauthorized", true, false), "quiet");
  assert.equal(toolTone("authorized", true, true), "attention");
  assert.ok(sideStatus.every((status) => allowed.includes(status)));
  assert.equal(rowStatus.includes("暂不提供"), false);
  // 有多端的厂商，副标题列出发现的端；只有一端时沿用那一端的说明
  assert.match(rows, /DeepSeek<\/strong><small>DeepSeek Harness、DS Bot · 1 端已授权<\/small>/);
  assert.match(rows, /Google<\/strong><small>未发现<\/small>/);

  const live = vendorMarks(
    toolMarks(
      tools.map((tool) => ({ ...tool, presence: tool.tool === "codex" ? "authorized" : "unauthorized" })),
      details,
      false,
    ),
  );
  const liveSide = renderToStaticMarkup(createElement(SidebarTools, { marks: live }));
  const liveRows = renderToStaticMarkup(createElement(OverviewToolRows, { marks: live, onOpen() {} }));
  const liveSideStatus = [...liveSide.matchAll(/aria-label="([^"]+)"/g)].map((match) => match[1]);
  const liveRowStatus = [...liveRows.matchAll(/<i class="dot ([a-z_]+) [a-z]+"><\/i>([^<]+)/g)].map((match) => match[2]);
  assert.deepEqual(liveSideStatus, liveRowStatus);
  assert.deepEqual(liveSideStatus, ["未授权", "未授权", "正常", "未授权", "未发现", "未发现", "未发现", "未发现"]);
  assert.match(liveRows, /DeepSeek<\/strong><small>DeepSeek Harness、DS Bot · 尚未授权<\/small>/);
  assert.equal(liveRowStatus.includes("暂不提供"), false);
} finally {
  await server.close();
}

const shell = readFileSync(new URL("./src/shell.css", import.meta.url), "utf8");
const usage = readFileSync(new URL("./src/usage.css", import.meta.url), "utf8");
assert.match(shell, /\.tool-dot\.ok \{[^}]*background:\s*var\(--color-mist\)/);
assert.match(shell, /\.tool-dot\.attention \{[^}]*background:\s*var\(--color-ochre\)/);
assert.match(shell, /\.tool-dot \{[^}]*background:\s*var\(--color-text-tertiary\)/);
assert.match(usage, /\.tool-status \.dot\.authorized \{[^}]*background:\s*var\(--color-mist\)/);
assert.match(usage, /\.tool-status \.dot\.needs_reauth \{[^}]*background:\s*var\(--color-ochre\)/);
assert.match(usage, /\.attention-item \{[^}]*padding:\s*12px 16px/);
assert.match(usage, /\.attention-ok \{[^}]*min-height:\s*0;[^}]*padding:\s*12px 16px/);
