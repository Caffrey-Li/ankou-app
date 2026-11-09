import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const shell = readFileSync(new URL("./src/shell.css", import.meta.url), "utf8");
const app = readFileSync(new URL("./src/App.tsx", import.meta.url), "utf8");
const icons = readFileSync(new URL("./src/icons.tsx", import.meta.url), "utf8");

assert.match(shell, /padding:\s*20px 10px 14px/);
assert.match(shell, /rgba\(255,\s*255,\s*255,\s*0\.45\)/);
assert.match(shell, /rgba\(255,\s*255,\s*255,\s*0\.85\)/);
assert.match(shell, /font-weight:\s*var\(--font-weight-medium\)/);
assert.match(shell, /color:\s*var\(--color-mist\)/);
assert.match(shell, /width:\s*7px/);
assert.match(shell, /font-size:\s*11px/);
assert.match(app, /更新于 —/);
assert.match(app, /aria-label="刷新"/);
assert.match(icons, /M2\.83333 7\.4375L8\.5 2\.83333/);
assert.match(icons, /stroke="currentColor"/);
