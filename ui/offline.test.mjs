import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const root = fileURLToPath(new URL("..", import.meta.url));
const roots = ["crates", "app/src", "ui/src", "brand"];
const forbidden = [
  "reqwest",
  "fetch(",
  "xmlhttprequest",
  "websocket",
  "application support",
  "state.vscdb",
];

// 守卫必须写出这些名字才能拒绝它们。只放行守卫自己的源码和测试。
const denylistNeedles = ["auth.json", "cursorauth", ".credentials"];
const denylistAllow = new Set([
  "crates/core/src/guard.rs",
  "crates/core/tests/guard.rs",
  "crates/core/tests/path_props.rs",
]);

function files(dir) {
  const found = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const info = statSync(path);
    if (info.isDirectory()) {
      found.push(...files(path));
    } else if (/\.(rs|ts|tsx|css|svg|mjs|json)$/.test(entry)) {
      found.push(path);
    }
  }
  return found;
}

const hits = [];
for (const relative of roots) {
  for (const path of files(join(root, relative))) {
    const text = readFileSync(path, "utf8").toLowerCase();
    const rel = path.slice(root.length).replaceAll("\\", "/").replace(/^\//, "");
    const needles = denylistAllow.has(rel) ? forbidden : forbidden.concat(denylistNeedles);
    for (const needle of needles) {
      if (text.includes(needle)) hits.push(`${rel}: ${needle}`);
    }
  }
}

assert.deepEqual(hits, []);
