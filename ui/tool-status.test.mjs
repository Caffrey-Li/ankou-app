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
  const { toolMarks, toolTone, SidebarTools, OverviewToolRows } = await server.ssrLoadModule(
    "/src/tool-status.tsx",
  );
  const tools = ["deepseek_harness", "ds_bot", "cursor", "codex"].map((tool) => ({
    tool,
    installed: true,
    presence: "missing",
  }));
  const details = [
    { name: "DeepSeek Harness", tokens: "暂不提供", calls: "暂不提供", fee: "不提供", source: "—" },
    { name: "DS Bot", tokens: "需要重新授权", calls: "需要重新授权", fee: "不提供", source: "需要重新授权" },
    { name: "Cursor", tokens: "未授权", calls: "未授权", fee: "不提供", source: "未授权" },
    { name: "ChatGPT", tokens: "12", calls: "1", fee: "不提供", source: "本机记录" },
  ];
  const marks = toolMarks(tools, details, true);
  const side = renderToStaticMarkup(createElement(SidebarTools, { marks }));
  const rows = renderToStaticMarkup(createElement(OverviewToolRows, { marks, onOpen() {} }));
  const sideStatus = [...side.matchAll(/aria-label="([^"]+)"/g)].map((match) => match[1]);
  const sideDot = [...side.matchAll(/tool-dot ([a-z_]+)/g)].map((match) => match[1]);
  const rowStatus = [...rows.matchAll(/<i class="dot ([a-z_]+) [a-z]+"><\/i>([^<]+)/g)].map((match) => match[2]);
  const rowDot = [...rows.matchAll(/<i class="dot ([a-z_]+) [a-z]+"><\/i>/g)].map((match) => match[1]);
  const allowed = ["未发现", "未授权", "正常", "需要处理", "数据过时", "需要重新授权"];

  assert.deepEqual(sideStatus, rowStatus);
  assert.deepEqual(sideStatus, ["正常", "需要重新授权", "未授权", "正常"]);
  assert.deepEqual(rowDot, ["authorized", "needs_reauth", "unauthorized", "authorized"]);
  // 侧栏点按 22:57：正常 = 雾蓝，需要处理 = 赭色，未连接或过时 = 灰
  assert.deepEqual(sideDot, ["ok", "attention", "quiet", "ok"]);
  const toned = toolMarks(tools, details, true, new Set(["deepseek_harness"]), new Set(["codex"]));
  assert.deepEqual(toned.map((mark) => mark.tone), ["attention", "attention", "quiet", "quiet"]);
  assert.equal(toolTone("unauthorized", true, false), "quiet");
  assert.equal(toolTone("authorized", true, true), "attention");
  assert.ok(sideStatus.every((status) => allowed.includes(status)));
  assert.equal(rowStatus.includes("暂不提供"), false);
  assert.match(rows, /DeepSeek Harness<\/strong><small>用量暂不提供<\/small>/);

  const live = toolMarks(
    tools.map((tool) => ({ ...tool, presence: tool.tool === "codex" ? "authorized" : "unauthorized" })),
    details,
    false,
  );
  const liveSide = renderToStaticMarkup(createElement(SidebarTools, { marks: live }));
  const liveRows = renderToStaticMarkup(createElement(OverviewToolRows, { marks: live, onOpen() {} }));
  const liveSideStatus = [...liveSide.matchAll(/aria-label="([^"]+)"/g)].map((match) => match[1]);
  const liveRowStatus = [...liveRows.matchAll(/<i class="dot ([a-z_]+) [a-z]+"><\/i>([^<]+)/g)].map((match) => match[2]);
  assert.deepEqual(liveSideStatus, liveRowStatus);
  assert.deepEqual(liveSideStatus, ["未授权", "未授权", "未授权", "正常"]);
  assert.match(liveRows, /DeepSeek Harness<\/strong><small>用量暂不提供<\/small>/);
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
