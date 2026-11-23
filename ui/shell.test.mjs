import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const shell = readFileSync(new URL("./src/shell.css", import.meta.url), "utf8");
const app = readFileSync(new URL("./src/App.tsx", import.meta.url), "utf8");
const icons = readFileSync(new URL("./src/icons.tsx", import.meta.url), "utf8");

// 新版：顶部导航，没有侧栏；颜色都走主题变量
assert.match(shell, /\.topbar \{/);
assert.match(shell, /\.topnav button\[aria-current="page"\]/);
assert.match(shell, /height:\s*56px/);
assert.match(shell, /font-size:\s*11px/);
assert.match(app, /className="topnav"/);
for (const label of ["总览", "工具", "交接", "清理", "设置"]) assert.match(app, new RegExp(`label: "${label}"`));
assert.equal(app.includes('className="sidebar"'), false);
assert.match(app, /更新于 —/);
assert.match(app, /aria-label="刷新"/);
assert.match(icons, /M2\.83333 7\.4375L8\.5 2\.83333/);
assert.match(icons, /stroke="currentColor"/);
