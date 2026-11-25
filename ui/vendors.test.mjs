import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

const server = await createServer({
  configFile: fileURLToPath(new URL("./vite.config.ts", import.meta.url)),
  server: { middlewareMode: true },
  appType: "custom",
  logLevel: "error",
});

try {
  const { vendors, toolSpecs, vendorOf } = await server.ssrLoadModule("/src/catalog.ts");
  // 每一端都只属于一个厂商，没有遗漏
  const members = vendors.flatMap((vendor) => vendor.members);
  assert.equal(new Set(members).size, members.length);
  assert.deepEqual([...members].sort(), toolSpecs.map((spec) => spec.id).sort());
  assert.equal(vendorOf("cursor_cli").name, "Cursor");
  assert.equal(vendorOf("codebuddy").name, "腾讯");
  assert.equal(vendorOf("antigravity").name, "Google");
  // 多端的厂商，每一端都写清楚覆盖的形态
  for (const spec of toolSpecs) assert.ok(spec.ends, spec.id);

  const { groupDetails } = await server.ssrLoadModule("/src/UsagePage.tsx");
  const groups = groupDetails([
    { name: "DeepSeek Harness", tokens: "1,200", calls: "3", fee: "不提供", source: "本机记录" },
    { name: "DS Bot", tokens: "800", calls: "2", fee: "不提供", source: "本机记录" },
    { name: "ChatGPT", tokens: "12", calls: "1", fee: "不提供", source: "本机记录" },
    { name: "Antigravity", tokens: "未授权", calls: "未授权", fee: "不提供", source: "未授权" },
    { name: "Gemini", tokens: "暂不提供", calls: "暂不提供", fee: "不提供", source: "—" },
  ]);
  assert.deepEqual(
    groups.map((group) => [group.name, group.ends.length, group.row.tokens, group.row.calls, group.row.source]),
    [
      ["DeepSeek", 2, "2,000", "5", "本机记录"],
      ["ChatGPT", 1, "12", "1", "本机记录"],
      ["Google", 2, "见各端", "见各端", "见各端"],
    ],
  );

  const { SettingsPage, VendorAccess, vendorPresence } = await server.ssrLoadModule("/src/SettingsPage.tsx");
  assert.equal(vendorPresence(["authorized", "missing"]), "已授权 · 发现 1/2 端");
  assert.equal(vendorPresence(["missing", "missing"]), "未发现");
  assert.equal(vendorPresence(["authorized", "needs_reauth"]), "需要重新授权 · 发现 2/2 端");
  const { fallbackSnapshot } = await server.ssrLoadModule("/src/api.ts");
  // 授权移到「工具」页：一个厂商一块，里面是各端的授权
  const access = renderToStaticMarkup(
    createElement(VendorAccess, { vendorId: "cursor", snapshot: fallbackSnapshot(), onChange() {}, onReload() {} }),
  );
  assert.match(access, /id="tool-cursor"/);
  assert.match(access, /aria-label="Cursor 命令行 对话"/);
  assert.match(access, /cursor-agent 命令行/);
  // 设置页只剩通用设置：有「外观」，没有授权列表
  const html = renderToStaticMarkup(
    createElement(SettingsPage, { snapshot: fallbackSnapshot(), onChange() {}, onReload() {}, onTools() {} }),
  );
  assert.equal((html.match(/class="tool-block/g) ?? []).length, 0);
  assert.match(html, /aria-label="主题"/);
  assert.match(html, /跟随系统/);
  assert.match(html, /去工具/);
} finally {
  await server.close();
}
