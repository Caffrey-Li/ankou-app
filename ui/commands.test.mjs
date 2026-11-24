import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const capability = JSON.parse(
  readFileSync(new URL("../app/capabilities/default.json", import.meta.url), "utf8"),
);
const allowed = capability.permissions.filter((item) => typeof item === "string" && item.startsWith("allow-"));
const ui = ["api.ts", "App.tsx", "SettingsPage.tsx", "SetupPage.tsx", "catalog.ts", "UsagePage.tsx"]
  .map((name) => readFileSync(new URL(`./src/${name}`, import.meta.url), "utf8"))
  .join("\n");

const api = readFileSync(new URL("./src/api.ts", import.meta.url), "utf8");
const invoked = [...api.matchAll(/call(?:<[^>]+>)?\("([a-z_]+)"/g)].map((match) =>
  match[1].replaceAll("_", "-"),
);
assert.deepEqual(
  [...new Set(invoked)].sort(),
  [
    "begin-summary",
    "cancel-scan",
    "cancel-system-scan",
    "check-update",
    "clear-history",
    "complete-setup",
    "deepseek-key-saved",
    "delete-deepseek-key",
    "discover-tools",
    "export-handoff",
    "get-settings",
    "get-usage",
    "install-update",
    "list-conversations",
    "list-grants",
    "open-privacy-settings",
    "organize-handoff",
    "place-deeplink",
    "prepare-cleanup",
    "prepare-deeplink",
    "prepare-handoff",
    "prepare-system-cleanup",
    "refresh-usage",
    "revoke",
    "run-cleanup",
    "run-system-cleanup",
    "save-deepseek-key",
    "scan-space",
    "scan-system",
    "send-notice",
    "send-summary",
    "set-capability",
    "set-settings",
    "tool-icons",
    "tray-action",
  ],
);
const csp = JSON.parse(readFileSync(new URL("../app/tauri.conf.json", import.meta.url), "utf8")).app.security
  .csp;
const directives = Object.fromEntries(
  csp.split(";").map((part) => {
    const [name, ...rest] = part.trim().split(/\s+/);
    return [name, rest.join(" ")];
  }),
);
assert.match(directives["img-src"], /(^| )data:( |$)/);
assert.equal(directives["connect-src"], "ipc: http://ipc.localhost");
assert.equal(allowed.includes("allow-grant"), false);
assert.equal(allowed.length, invoked.length);
for (const name of invoked) {
  assert.equal(allowed.filter((item) => item === `allow-${name}`).length, 1, name);
}
assert.equal(ui.includes("已连接"), false);
assert.match(ui, /未发现/);
assert.match(ui, /未授权/);
assert.match(ui, /未验证/);
assert.match(ui, /disabled/);
assert.match(ui, /launch_at_login: false/);
assert.match(ui, /notifications: false/);
assert.equal(ui.includes("type=\"text\""), false);
