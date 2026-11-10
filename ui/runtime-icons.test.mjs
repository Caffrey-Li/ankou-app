import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";

const images = [];

function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "dist") continue;
    assert.notEqual(name, "tool-icons");
    const file = new URL(`${name}${statSync(new URL(name, dir)).isDirectory() ? "/" : ""}`, dir);
    if (file.pathname.endsWith("/")) walk(file);
    else if (/\.(png|icns|ico|svg|webp|jpe?g|gif)$/i.test(name)) images.push(name);
  }
}

walk(new URL("../ui/", import.meta.url));
walk(new URL("../app/", import.meta.url));

const vendor = /chatgpt|openai|deepseek|cursor|codex|claude|anthropic|grok/i;
for (const name of images) assert.equal(vendor.test(name), false, name);

const catalog = readFileSync(new URL("./src/catalog.ts", import.meta.url), "utf8");
assert.match(catalog, /id: "deepseek_harness"[\s\S]*?letter: "H"/);
assert.match(catalog, /id: "ds_bot"[\s\S]*?letter: "B"/);

const status = readFileSync(new URL("./src/tool-status.tsx", import.meta.url), "utf8");
assert.equal(status.includes("tool-icons"), false);
assert.equal(status.includes("chatgpt"), false);
assert.match(status, /spec\?\.letter/);

const app = readFileSync(new URL("./src/App.tsx", import.meta.url), "utf8");
assert.match(app, /className="traffic" data-tauri-drag-region/);
assert.match(app, /className="toolbar-drag" data-tauri-drag-region/);
for (const line of app.split("\n")) {
  if (!line.includes("data-tauri-drag-region")) continue;
  assert.equal(line.includes("<button"), false, line);
  assert.equal(line.includes("<input"), false, line);
}
