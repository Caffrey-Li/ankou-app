import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
execFileSync("npx", ["vite", "build", "--config", "ui/vite.config.ts"], {
  cwd: root,
  stdio: "inherit",
});

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return files(path);
    return /\.(js|css|html)$/.test(name) ? [path] : [];
  });
}

const built = files(join(root, "ui", "dist"))
  .map((path) => readFileSync(path, "utf8"))
  .join("\n");
assert.equal(built.includes("12.34"), false);
assert.equal(built.includes("fixture-m2"), false);
for (const needle of ["WANXIANG_VERIFY", "verify_enabled", "verify_set"]) {
  assert.equal(built.includes(needle), false, needle);
}

const builtApp = execFileSync(
  "cargo",
  ["build", "--release", "-p", "wanxiang-app", "--offline", "--message-format=json"],
  { cwd: root, encoding: "utf8" },
);
const executable = builtApp
  .split("\n")
  .filter((line) => line.startsWith("{"))
  .map((line) => JSON.parse(line))
  .find((message) => message.reason === "compiler-artifact" && message.target?.name === "wanxiang-app")
  ?.executable;
assert.ok(executable, "release executable");
const binary = readFileSync(executable);
for (const needle of ["WANXIANG_VERIFY", "verify_enabled", "verify_set"]) {
  assert.equal(binary.includes(Buffer.from(needle)), false, needle);
}
