import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const files = ["UsagePage.tsx", "App.tsx", "api.ts", "catalog.ts", "usage.css", "OverviewCards.tsx"];
const source = files
  .map((name) => readFileSync(new URL(`./src/${name}`, import.meta.url), "utf8"))
  .join("\n");

for (const banned of ["3.49", "128.40", "17:05", "21:40", "去授权"]) {
  assert.equal(source.includes(banned), false, banned);
}
assert.match(source, /暂不提供/);
assert.match(source, /未授权/);
assert.match(source, /需要重新授权/);
assert.match(source, /去设置授权/);
assert.match(source, /未保存密钥/);
// 概览的对话、空间卡显示真实数据；没有数据时说明原因，不显示 0
assert.match(source, /还没有可显示的对话/);
assert.match(source, /尚未扫描，打开空间管理后点刷新/);
assert.equal(source.includes("尚未接入"), false);
assert.match(source, /一切正常/);
assert.match(source, /不提供/);
assert.match(source, /Harness 与 DS Bot 共用/);
assert.match(source, /管理授权/);
assert.match(source, /个人账户来源待验证/);
assert.match(source, /余额密钥未保存/);
assert.match(source, /DeepSeek 账户 · 余额/);
assert.match(source, /更早没有本机记录，不补算/);
assert.match(source, /按各工具自身周期，不受上方日期影响/);
assert.match(source, /刷新中/);
assert.match(source, /"已实测"/);
assert.equal(source.includes("其余工具运行正常"), false);
assert.match(source, /个对话超过提醒阈值/);
assert.match(source, /已授权/);
