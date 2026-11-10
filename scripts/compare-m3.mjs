import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { PNG } from "pngjs";
import pixelmatch from "pixelmatch";
import { chromium } from "playwright";

const root = new URL("..", import.meta.url);
const file = (path) => new URL(path, root);
const decode = (path) => PNG.sync.read(readFileSync(path));
const encode = (png, path) => writeFileSync(path, PNG.sync.write(png));

function windowFrame(source) {
  let minX = source.width;
  let minY = source.height;
  let maxX = 0;
  let maxY = 0;
  for (let y = 0; y < source.height; y += 1) {
    for (let x = 0; x < source.width; x += 1) {
      const i = (source.width * y + x) << 2;
      const bright = source.data[i] + source.data[i + 1] + source.data[i + 2];
      if (source.data[i + 3] > 200 && bright > 80) {
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
    }
  }
  return crop(source, minX, minY, maxX - minX + 1, maxY - minY + 1);
}

function crop(source, x, y, width, height) {
  const png = new PNG({ width, height });
  PNG.bitblt(source, png, x, y, width, height, 0, 0);
  return png;
}

function scale(source, targetWidth, targetHeight) {
  const png = new PNG({ width: targetWidth, height: targetHeight });
  for (let y = 0; y < targetHeight; y += 1) {
    const sy = Math.min(source.height - 1, Math.round((y * (source.height - 1)) / Math.max(1, targetHeight - 1)));
    for (let x = 0; x < targetWidth; x += 1) {
      const sx = Math.min(source.width - 1, Math.round((x * (source.width - 1)) / Math.max(1, targetWidth - 1)));
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
  const png = new PNG({ width: left.width + gap + right.width, height: Math.max(left.height, right.height) });
  png.data.fill(242);
  PNG.bitblt(left, png, 0, 0, left.width, left.height, 0, 0);
  PNG.bitblt(right, png, 0, 0, right.width, right.height, left.width + gap, 0);
  return png;
}

function diffPair(expected, actual) {
  const right = flatten(actual);
  const left = scale(flatten(expected), right.width, right.height);
  const diff = new PNG({ width: left.width, height: left.height });
  const mismatched = pixelmatch(left.data, right.data, diff.data, left.width, left.height, { threshold: 0.05, includeAA: false });
  return { left, right, diff, mismatched, ratio: Number((mismatched / (left.width * left.height)).toFixed(4)) };
}

function regionRatio(left, right, box) {
  const x = Math.max(0, Math.round(box.x * left.width));
  const y = Math.max(0, Math.round(box.y * left.height));
  const w = Math.max(1, Math.min(left.width - x, Math.round(box.w * left.width)));
  const h = Math.max(1, Math.min(left.height - y, Math.round(box.h * left.height)));
  const a = new PNG({ width: w, height: h });
  const b = new PNG({ width: w, height: h });
  PNG.bitblt(left, a, x, y, w, h, 0, 0);
  PNG.bitblt(right, b, x, y, w, h, 0, 0);
  const diff = new PNG({ width: w, height: h });
  const mismatched = pixelmatch(a.data, b.data, diff.data, w, h, { threshold: 0.05, includeAA: false });
  return { mismatched, ratio: Number((mismatched / (w * h)).toFixed(4)), size: `${w}x${h}` };
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
      if (!settled && (logs.includes("Local:") || logs.includes("already in use"))) {
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

const regions = {
  工具栏: { x: 250 / 1280, y: 16 / 900, w: 990 / 1280, h: 54 / 900 },
  侧栏: { x: 9 / 1280, y: 9 / 900, w: 224 / 1280, h: 880 / 900 },
  对话列表: { x: 272 / 1280, y: 86 / 900, w: 336 / 1280, h: 780 / 900 },
  详情: { x: 622 / 1280, y: 86 / 900, w: 620 / 1280, h: 780 / 900 },
};

mkdirSync(file("design/refs"), { recursive: true });
mkdirSync(file("design/actual"), { recursive: true });
mkdirSync(file("design/diff"), { recursive: true });

const handoffRef = windowFrame(decode(file("design/refs/m3-handoff-18-646.png")));
const previewRef = windowFrame(decode(file("design/refs/m3-preview-20-73.png")));
encode(handoffRef, file("design/refs/m3-handoff.png"));
encode(previewRef, file("design/refs/m3-preview.png"));

const server = await waitForServer();
try {
  const browser = await chromium.launch({ channel: "chrome" });
  for (const [name, url] of [
    ["m3-handoff", "http://127.0.0.1:1420/?compare=1&fixture=m3&page=handoff&preview=1"],
    ["m3-preview", "http://127.0.0.1:1420/?compare=1&fixture=m3&page=handoff&preview=1&sheet=preview"],
    ["m3-overview", "http://127.0.0.1:1420/?compare=1&fixture=m3&page=overview&preview=1"],
  ]) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
    await page.goto(url, { waitUntil: "networkidle" });
    await page.waitForFunction(() => document.body.innerText.includes("夹具对话") || document.body.innerText.includes("超过提醒阈值"));
    await page.screenshot({ path: file(`design/actual/${name}.png`).pathname, omitBackground: true });
    await page.close();
  }
  await browser.close();
} finally {
  server.kill("SIGTERM");
}

function whitePanel(source) {
  let minX = source.width;
  let minY = source.height;
  let maxX = 0;
  let maxY = 0;
  let runs = 0;
  for (let y = 0; y < source.height; y += 1) {
    let run = 0;
    let runStart = 0;
    for (let x = 0; x < source.width; x += 1) {
      const i = (source.width * y + x) << 2;
      const white = source.data[i] > 248 && source.data[i + 1] > 248 && source.data[i + 2] > 248;
      if (white) {
        if (run === 0) runStart = x;
        run += 1;
      } else if (run >= 700) {
        runs += 1;
        if (runStart < minX) minX = runStart;
        if (x - 1 > maxX) maxX = x - 1;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
        run = 0;
      } else {
        run = 0;
      }
    }
    if (run >= 700) {
      runs += 1;
      if (runStart < minX) minX = runStart;
      if (source.width - 1 > maxX) maxX = source.width - 1;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
  if (runs === 0) return null;
  return crop(source, minX, minY, maxX - minX + 1, maxY - minY + 1);
}

function report(name, reference) {
  const actual = decode(file(`design/actual/${name}.png`));
  const pair = diffPair(reference, actual);
  encode(pair.diff, file(`design/diff/${name}.png`));
  encode(beside(pair.left, pair.right), file(`design/diff/${name}-side-by-side.png`));
  return {
    mismatched: pair.mismatched,
    ratio: pair.ratio,
    size: `${pair.left.width}x${pair.left.height}`,
    regions: Object.fromEntries(Object.entries(regions).map(([label, box]) => [label, regionRatio(pair.left, pair.right, box)])),
  };
}

const previewPair = diffPair(previewRef, decode(file("design/actual/m3-preview.png")));
const previewPanel = diffPair(whitePanel(previewRef), whitePanel(previewPair.right));
encode(previewPanel.diff, file("design/diff/m3-preview-panel.png"));
encode(beside(previewPanel.left, previewPanel.right), file("design/diff/m3-preview-panel-side-by-side.png"));

console.log(
  JSON.stringify(
    {
      threshold: 0.05,
      handoff: report("m3-handoff", handoffRef),
      preview: report("m3-preview", previewRef),
      previewPanel: { mismatched: previewPanel.mismatched, ratio: previewPanel.ratio, size: `${previewPanel.left.width}x${previewPanel.left.height}` },
      note: "预览弹窗单独裁出白面板后再比，不把背后的列表算进去。夹具文案和稿面示例不同。",
    },
    null,
    2,
  ),
);
