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
  "auth.json",
  "cursorauth",
  ".credentials",
];

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
    for (const needle of forbidden) {
      if (text.includes(needle)) hits.push(`${path}: ${needle}`);
    }
  }
}

assert.deepEqual(hits, []);
