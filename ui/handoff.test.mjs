import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const page = readFileSync(new URL("./src/HandoffPage.tsx", import.meta.url), "utf8");
const settings = readFileSync(new URL("./src/SettingsPage.tsx", import.meta.url), "utf8");
const app = readFileSync(new URL("./src/App.tsx", import.meta.url), "utf8");

for (const line of [
  "ChatGPT 应用未运行，没有读到本机会话",
  "这条对话已经不在，无法交接",
  "超过 8000 字，不能打开深链。全文仍可复制。",
  "摘要没有生成。编辑还在，可以改用本地整理或重试。",
  "未能放入：深链没有打开 · 编辑已保留",
  "长对话只取了最近部分",
  "暂不提供",
]) {
  assert.equal(page.includes(line), true, line);
}
assert.equal(app.includes("其余工具运行正常"), false);
assert.equal(settings.includes("codex_conversation"), false);
assert.equal(settings.includes("ds_bot_conversation"), false);
assert.match(settings, /disabled/);
