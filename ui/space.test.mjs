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
  const { SpacePage, formatBytes, groupEntries, groupMark, splitSize, statOnlyNote } = await server.ssrLoadModule("/src/SpacePage.tsx");
  assert.deepEqual(splitSize(14_600_000_000), { value: "14.6", unit: "GB" });
  assert.equal(formatBytes(1_100_000_000), "1.10 GB");
  assert.equal(formatBytes(9126), "9 KB");
  assert.equal(formatBytes(236074), "236 KB");
  assert.equal(formatBytes(245200), "245 KB");
  assert.equal(formatBytes(6_000_000), "6 MB");
  assert.equal(formatBytes(0), "0 B");

  const html = renderToStaticMarkup(
    createElement(SpacePage, {
      fixture: true,
      windows: false,
      sheet: "",
      thresholdGb: 5,
      scanRequest: 0,
      onStamp() {},
    }),
  );
  assert.equal(html.includes("checked="), false);
  assert.match(html, /证据不足/);
  assert.equal(html.includes("session.log"), false);
  assert.match(html, /日志/);
  // 八个工具都是可以选中的按钮，默认选中 Harness
  assert.equal((html.match(/aria-pressed="(true|false)"/g) ?? []).length, 8);
  assert.equal((html.match(/aria-pressed="true"/g) ?? []).length, 1);
  assert.match(html, /清理所选项/);

  for (const [tool, name] of [
    ["cursor", "Cursor"],
    ["codex", "ChatGPT"],
    ["claude", "Claude"],
    ["antigravity", "Antigravity"],
    ["gemini", "Gemini"],
    ["workbuddy", "WorkBuddy"],
  ]) {
    const detail = renderToStaticMarkup(
      createElement(SpacePage, {
        fixture: true,
        windows: false,
        sheet: "",
        thresholdGb: 5,
        scanRequest: 0,
        onStamp() {},
        initialTool: tool,
      }),
    );
    assert.match(detail, new RegExp(`aria-label="${name}"`));
    assert.equal(detail.includes("清理所选项"), false, `${name} 不能出现清理按钮`);
    assert.ok(detail.includes(statOnlyNote[tool]));
  }
  const bot = renderToStaticMarkup(
    createElement(SpacePage, {
      fixture: true,
      windows: false,
      sheet: "",
      thresholdGb: 5,
      scanRequest: 0,
      onStamp() {},
      initialTool: "ds_bot",
    }),
  );
  assert.match(bot, /查看 DeepSeek Harness/);
  assert.equal(bot.includes("清理所选项"), false);

  const grouped = groupEntries([
    { rel: "logs/new", bytes: 1, files: 1, dev: 1, ino: 1, modified: 20, weak: false },
    { rel: "logs/old", bytes: 1, files: 1, dev: 1, ino: 2, modified: 5, weak: true },
  ]);
  assert.deepEqual(
    grouped[0].entries.map((entry) => entry.rel),
    ["logs/old", "logs/new"],
  );
  const none = groupMark(grouped[0].entries, {});
  assert.equal(none.checked, false);
  assert.equal(none.indeterminate, false);
  const some = groupMark(grouped[0].entries, { "logs/old": true });
  assert.equal(some.checked, false);
  assert.equal(some.indeterminate, true);
} finally {
  await server.close();
}
