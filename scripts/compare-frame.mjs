import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { PNG } from "pngjs";
import pixelmatch from "pixelmatch";
import { chromium } from "playwright";

const width = 1280;
const height = 900;
const root = new URL("..", import.meta.url);

function file(path) {
  return new URL(path, root);
}

function decode(path) {
  return PNG.sync.read(readFileSync(path));
}

function encode(png, path) {
  writeFileSync(path, PNG.sync.write(png));
}

function cropWindow(source) {
  let minX = source.width;
  let minY = source.height;
  let maxX = 0;
  let maxY = 0;
  for (let y = 0; y < source.height; y += 1) {
    for (let x = 0; x < source.width; x += 1) {
      const i = (source.width * y + x) << 2;
      const bright = source.data[i] + source.data[i + 1] + source.data[i + 2];
      if (source.data[i + 3] > 180 && bright > 540) {
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
    }
  }
  const cropWidth = maxX - minX + 1;
  const cropHeight = maxY - minY + 1;
  const cropped = new PNG({ width: cropWidth, height: cropHeight });
  PNG.bitblt(source, cropped, minX, minY, cropWidth, cropHeight, 0, 0);
  return cropped;
}

function scale(source, targetWidth, targetHeight) {
  const png = new PNG({ width: targetWidth, height: targetHeight });
  for (let y = 0; y < targetHeight; y += 1) {
    const sy = Math.min(source.height - 1, Math.round((y * (source.height - 1)) / (targetHeight - 1)));
    for (let x = 0; x < targetWidth; x += 1) {
      const sx = Math.min(source.width - 1, Math.round((x * (source.width - 1)) / (targetWidth - 1)));
      const si = (source.width * sy + sx) << 2;
      const di = (targetWidth * y + x) << 2;
      png.data[di] = source.data[si];
      png.data[di + 1] = source.data[si + 1];
      png.data[di + 2] = source.data[si + 2];
      png.data[di + 3] = 255;
    }
  }
  return png;
}

function flatten(source) {
  const png = new PNG({ width: source.width, height: source.height });
  for (let i = 0; i < source.data.length; i += 4) {
    const alpha = source.data[i + 3] / 255;
    png.data[i] = Math.round(source.data[i] * alpha + 242 * (1 - alpha));
    png.data[i + 1] = Math.round(source.data[i + 1] * alpha + 242 * (1 - alpha));
    png.data[i + 2] = Math.round(source.data[i + 2] * alpha + 242 * (1 - alpha));
    png.data[i + 3] = 255;
  }
  return png;
}

function beside(left, right) {
  const gap = 16;
  const png = new PNG({ width: left.width + gap + right.width, height: left.height });
  png.data.fill(242);
  PNG.bitblt(left, png, 0, 0, left.width, left.height, 0, 0);
  PNG.bitblt(right, png, 0, 0, right.width, right.height, left.width + gap, 0);
  return png;
}

function regionDiff(left, right, box) {
  const regionWidth = box.w;
  const regionHeight = box.h;
  const expected = new PNG({ width: regionWidth, height: regionHeight });
  const actual = new PNG({ width: regionWidth, height: regionHeight });
  const diff = new PNG({ width: regionWidth, height: regionHeight });
  PNG.bitblt(left, expected, box.x, box.y, regionWidth, regionHeight, 0, 0);
  PNG.bitblt(right, actual, box.x, box.y, regionWidth, regionHeight, 0, 0);
  const mismatched = pixelmatch(expected.data, actual.data, diff.data, regionWidth, regionHeight, {
    threshold: 0.05,
    includeAA: false,
  });
  return { mismatched, ratio: mismatched / (regionWidth * regionHeight) };
}

function waitForServer() {
  return new Promise((resolve, reject) => {
    const child = spawn("npm", ["run", "dev"], { stdio: ["ignore", "pipe", "pipe"] });
    let logs = "";
    let settled = false;
    const timer = setTimeout(() => {
      settled = true;
      child.kill("SIGTERM");
      reject(new Error(`开发服务器没有在 1420 端口就绪\n${logs}`));
    }, 20000);
    const onData = (chunk) => {
      logs += chunk.toString();
      if (!settled && logs.includes("Local:")) {
        settled = true;
        clearTimeout(timer);
        resolve(child);
      }
    };
    child.stdout.on("data", onData);
    child.stderr.on("data", onData);
    child.on("exit", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(new Error(`开发服务器退出 ${code}\n${logs}`));
    });
  });
}

mkdirSync(file("design/refs"), { recursive: true });
mkdirSync(file("design/actual"), { recursive: true });
mkdirSync(file("design/diff"), { recursive: true });

const exported = decode(file("design/refs/18-74-export.png"));
const reference = scale(cropWindow(exported), width, height);
encode(reference, file("design/refs/window.png"));

const server = await waitForServer();
try {
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width, height },
    deviceScaleFactor: 1,
  });
  await page.goto("http://127.0.0.1:1420/?compare=1", { waitUntil: "networkidle" });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await document.fonts.load("700 14px 'Noto Sans SC'");
    await document.fonts.load("500 13px 'Noto Sans SC'");
    await document.fonts.load("400 12px 'Noto Sans SC'");
    await document.fonts.load("400 12px Inter");
    await document.fonts.load("600 8px Inter");
  });
  await page.screenshot({
    path: file("design/actual/window.png").pathname,
    omitBackground: true,
  });
  await browser.close();
} finally {
  server.kill("SIGTERM");
}

const actual = flatten(decode(file("design/actual/window.png")));
const expected = flatten(reference);
const diff = new PNG({ width, height });
const mismatched = pixelmatch(expected.data, actual.data, diff.data, width, height, {
  threshold: 0.05,
  includeAA: false,
});
encode(diff, file("design/diff/window.png"));
encode(beside(expected, actual), file("design/diff/side-by-side.png"));

const regions = {
  sidebar: regionDiff(expected, actual, { x: 9, y: 9, w: 224, h: 880 }),
  toolbar: regionDiff(expected, actual, { x: 243, y: 8, w: 1026, h: 72 }),
  content: regionDiff(expected, actual, { x: 243, y: 80, w: 1026, h: 800 }),
};
console.log(
  JSON.stringify(
    {
      threshold: 0.05,
      mismatched,
      full: mismatched / (width * height),
      regions,
      reference: `${reference.width}x${reference.height}`,
    },
    null,
    2,
  ),
);
