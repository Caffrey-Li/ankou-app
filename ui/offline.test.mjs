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

// 守卫和空间扫描要写出这些名字，才能在 stat 之前跳过。只放行这两处和它们的测试。
const denylistNeedles = ["auth.json", "cursorauth", ".credentials"];
const denylistAllow = new Set([
  "crates/core/src/guard.rs",
  "crates/core/src/space.rs",
  "crates/core/tests/guard.rs",
  "crates/core/tests/path_props.rs",
  "crates/core/tests/space.rs",
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

// 用户同意放宽：Cursor 对话只在这一个文件里按白名单读数据库，别处仍然不许出现数据库名。
const stateAllow = new Set(["crates/adapters/cursor/src/chats.rs"]);

const hits = [];
for (const relative of roots) {
  for (const path of files(join(root, relative))) {
    const text = readFileSync(path, "utf8").toLowerCase();
    const rel = path.slice(root.length).replaceAll("\\", "/").replace(/^\//, "");
    const needles = denylistAllow.has(rel) ? forbidden : forbidden.concat(denylistNeedles);
    for (const needle of needles) {
      if (needle === "state.vscdb" && stateAllow.has(rel)) continue;
      if (text.includes(needle)) hits.push(`${rel}: ${needle}`);
    }
  }
}

assert.deepEqual(hits, []);
