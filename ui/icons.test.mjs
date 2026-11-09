import assert from "node:assert/strict";
import { statSync } from "node:fs";

const files = [
  "../app/icons/32x32.png",
  "../app/icons/128x128.png",
  "../app/icons/128x128@2x.png",
  "../app/icons/icon.icns",
  "../app/icons/icon.ico",
  "../app/icons/icon-windows-256.png",
  "../app/icons/tray-macos-template.png",
  "../app/icons/tray-windows-light.png",
  "../app/icons/tray-windows-dark.png",
];

for (const file of files) {
  const path = new URL(file, import.meta.url);
  assert.ok(statSync(path).size > 80, file);
}
