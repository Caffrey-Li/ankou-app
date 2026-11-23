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
  const { UpdateGate, UpdateSection, UpdateBadge, progressText } = await server.ssrLoadModule("/src/update.tsx");
  const base = {
    info: { current: "0.1.0", latest: null, notes: null, date: null, minimum: null, mandatory: false, error: null },
    checking: false,
    installing: false,
    progress: null,
    installError: null,
    checkedAt: null,
    check: async () => {},
    install: async () => {},
  };
  const render = (component, updater) => renderToStaticMarkup(createElement(component, { updater, onOpen() {} }));

  // 没有新版本：不挡、不显示入口，设置里显示当前版本和「检查更新」
  assert.equal(render(UpdateGate, base), "");
  assert.equal(render(UpdateBadge, base), "");
  assert.match(render(UpdateSection, base), /当前版本 0\.1\.0/);
  assert.match(render(UpdateSection, base), /检查更新/);

  // 有新版本但不强制：不挡，侧栏有入口，设置里可以更新
  const optional = { ...base, info: { ...base.info, latest: "0.2.0", notes: "- 修复 A\n- 新增 B", minimum: "0.1.0" } };
  assert.equal(render(UpdateGate, optional), "");
  assert.match(render(UpdateBadge, optional), /新版本 0\.2\.0/);
  const section = render(UpdateSection, optional);
  assert.match(section, /有新版本 0\.2\.0/);
  assert.match(section, /更新并重启/);
  assert.match(section, /修复 A/);

  // 强制更新：整窗挡住，只有「退出」和「立即更新」
  const forced = { ...base, info: { ...base.info, latest: "0.3.0", minimum: "0.3.0", mandatory: true } };
  const gate = render(UpdateGate, forced);
  assert.match(gate, /aria-modal="true"/);
  assert.match(gate, /需要更新到 0\.3\.0/);
  assert.match(gate, /最低要求 0\.3\.0/);
  assert.equal((gate.match(/<button/g) ?? []).length, 2);
  assert.match(render(UpdateSection, forced), /必须更新/);

  // 进度文字
  assert.equal(progressText({ downloaded: 5 * 1048576, total: 20 * 1048576, stage: "download" }), "正在下载 25%（5.0 / 20.0 MB）");
  assert.equal(progressText({ downloaded: 0, total: null, stage: "install" }), "正在安装，完成后自动重启…");
} finally {
  await server.close();
}
