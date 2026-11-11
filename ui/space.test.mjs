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
  const { SpacePage, formatBytes, groupEntries, groupMark } = await server.ssrLoadModule("/src/SpacePage.tsx");
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
