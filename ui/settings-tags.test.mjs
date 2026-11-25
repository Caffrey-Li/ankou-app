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
  const { authTagClass } = await server.ssrLoadModule("/src/SettingsPage.tsx");
  const states = ["已授权", "未授权", "未验证", "暂不提供"];
  const html = states.map((state) =>
    renderToStaticMarkup(createElement("span", { className: authTagClass(state) }, state)),
  );
  const classes = html.map((item) => item.match(/class="([^"]+)"/)[1]);
  assert.equal(new Set(classes).size, states.length);
  assert.deepEqual(classes, ["tag granted", "tag unauthorized", "tag unverified", "tag withheld"]);
} finally {
  await server.close();
}
