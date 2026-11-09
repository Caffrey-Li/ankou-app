import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brand = join(root, "brand");
const icons = join(root, "app", "icons");
mkdirSync(icons, { recursive: true });

function png(name, size) {
  const svg = readFileSync(join(brand, name), "utf8");
  const image = new Resvg(svg, {
    fitTo: { mode: "width", value: size },
    background: "rgba(0,0,0,0)",
  }).render();
  return image.asPng();
}

function writePng(file, name, size) {
  writeFileSync(join(icons, file), png(name, size));
}

writePng("32x32.png", "icon-macos.svg", 32);
writePng("128x128.png", "icon-macos.svg", 128);
writePng("128x128@2x.png", "icon-macos.svg", 256);
writePng("icon-windows-256.png", "icon-windows.svg", 256);
writePng("tray-macos-template.png", "tray-macos-template.svg", 18);
writePng("tray-windows-light.png", "tray-windows-light.svg", 16);
writePng("tray-windows-dark.png", "tray-windows-dark.svg", 16);

const icoImages = [
  [16, "icon-windows-small.svg"],
  [20, "icon-windows-small.svg"],
  [24, "icon-windows-small.svg"],
  [32, "icon-windows.svg"],
  [40, "icon-windows.svg"],
  [48, "icon-windows.svg"],
  [64, "icon-windows.svg"],
  [256, "icon-windows.svg"],
].map(([size, name]) => ({ size, png: png(name, size) }));

writeFileSync(join(icons, "icon.ico"), toIco(icoImages));

if (process.platform === "darwin") {
  const iconset = join(icons, "icon.iconset");
  rmSync(iconset, { recursive: true, force: true });
  mkdirSync(iconset);
  const pairs = [
    ["icon_16x16.png", 16],
    ["icon_16x16@2x.png", 32],
    ["icon_32x32.png", 32],
    ["icon_32x32@2x.png", 64],
    ["icon_128x128.png", 128],
    ["icon_128x128@2x.png", 256],
    ["icon_256x256.png", 256],
    ["icon_256x256@2x.png", 512],
    ["icon_512x512.png", 512],
    ["icon_512x512@2x.png", 1024],
  ];
  for (const [file, size] of pairs) {
    writeFileSync(join(iconset, file), png("icon-macos.svg", size));
  }
  execFileSync("iconutil", ["-c", "icns", iconset, "-o", join(icons, "icon.icns")]);
  rmSync(iconset, { recursive: true, force: true });
}

function toIco(images) {
  const headerSize = 6 + images.length * 16;
  let offset = headerSize;
  const entries = [];
  for (const image of images) {
    const entry = Buffer.alloc(16);
    entry.writeUInt8(image.size >= 256 ? 0 : image.size, 0);
    entry.writeUInt8(image.size >= 256 ? 0 : image.size, 1);
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(image.png.length, 8);
    entry.writeUInt32LE(offset, 12);
    entries.push(entry);
    offset += image.png.length;
  }
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  return Buffer.concat([header, ...entries, ...images.map((image) => image.png)]);
}
