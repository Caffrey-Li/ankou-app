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
  return match(left, right);
}

function containPair(expected, actual) {
  const right = flatten(actual);
  const fitted = contain(flatten(expected), right.width, right.height);
  return match(fitted, right);
}

function contain(source, targetWidth, targetHeight) {
  const factor = Math.min(targetWidth / source.width, targetHeight / source.height);
  const width = Math.max(1, Math.round(source.width * factor));
  const height = Math.max(1, Math.round(source.height * factor));
  const fitted = scale(source, width, height);
  const png = new PNG({ width: targetWidth, height: targetHeight });
  png.data.fill(255);
  PNG.bitblt(
    fitted,
    png,
    0,
    0,
    width,
    height,
    Math.round((targetWidth - width) / 2),
    Math.round((targetHeight - height) / 2),
  );
  return png;
}

function match(left, right) {
  const diff = new PNG({ width: left.width, height: left.height });
  const mismatched = pixelmatch(left.data, right.data, diff.data, left.width, left.height, { threshold: 0.05, includeAA: false });
  return { left, right, diff, mismatched, ratio: Number((mismatched / (left.width * left.height)).toFixed(4)) };
}

function isPanel(source, x, y) {
  const i = (source.width * y + x) << 2;
  return source.data[i] > 248 && source.data[i + 1] > 248 && source.data[i + 2] > 248 && source.data[i + 3] > 240;
}

function whitePanels(source) {
  const runs = [];
  for (let y = 0; y < source.height; y += 1) {
    let start = -1;
    for (let x = 0; x <= source.width; x += 1) {
      const panel = x < source.width && isPanel(source, x, y);
      if (panel && start < 0) start = x;
      if (!panel && start >= 0) {
        const width = x - start;
        if (width >= 250 && width <= 320) runs.push({ y, x0: start, x1: x - 1 });
        start = -1;
      }
    }
  }
  const boxes = [];
  for (const run of runs) {
    const box = boxes.find(
      (item) => run.y <= item.y1 + 40 && Math.abs(run.x0 - item.x0) < 24 && Math.abs(run.x1 - item.x1) < 24,
    );
    if (box) {
      box.x0 = Math.min(box.x0, run.x0);
      box.x1 = Math.max(box.x1, run.x1);
      box.y0 = Math.min(box.y0, run.y);
      box.y1 = Math.max(box.y1, run.y);
      box.rows += 1;
    } else {
      boxes.push({ x0: run.x0, x1: run.x1, y0: run.y, y1: run.y, rows: 1 });
    }
  }
  return boxes
    .filter((box) => box.y1 - box.y0 > 200 && box.rows > 20)
    .sort((a, b) => a.x0 - b.x0)
    .map((box) => {
      const probe = box.x0 + 16;
      let y0 = box.y0;
      let y1 = box.y1;
      while (y0 > 0 && isPanel(source, probe, y0 - 1)) y0 -= 1;
      while (y1 + 1 < source.height && isPanel(source, probe, y1 + 1)) y1 += 1;
      return crop(source, box.x0, y0, box.x1 - box.x0 + 1, y1 - y0 + 1);
    });
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

const spaceRegions = {
  指标: { x: 0.2, y: 0.1, w: 0.76, h: 0.16 },
  工具列表: { x: 0.2, y: 0.28, w: 0.3, h: 0.66 },
  详情: { x: 0.52, y: 0.28, w: 0.44, h: 0.66 },
};

const overviewRegions = {
  需要处理: { x: 0.2, y: 0.1, w: 0.76, h: 0.28 },
  空间卡: { x: 0.66, y: 0.4, w: 0.3, h: 0.24 },
};

mkdirSync(file("design/refs"), { recursive: true });
mkdirSync(file("design/actual"), { recursive: true });
mkdirSync(file("design/diff"), { recursive: true });

const spaceRef = windowFrame(decode(file("design/refs/m4-space-20-227.png")));
const spacePageRef = windowFrame(decode(file("design/refs/m4-space-20-223.png")));
const overviewRef = windowFrame(decode(file("design/refs/m4-overview-18-70.png")));
const dialogPanels = whitePanels(decode(file("design/refs/m4-dialog-21-198.png")));
const confirmRef = dialogPanels[2];
const resultRef = dialogPanels[3];
if (!confirmRef || !resultRef) throw new Error(`对话框白面板没有裁全：${dialogPanels.length}`);
encode(spaceRef, file("design/refs/m4-space.png"));
encode(overviewRef, file("design/refs/m4-overview.png"));
encode(confirmRef, file("design/refs/m4-confirm.png"));
encode(resultRef, file("design/refs/m4-result.png"));

const server = await waitForServer();
try {
  const browser = await chromium.launch({ channel: "chrome" });
  for (const [name, url, ready] of [
    ["m4-space", "http://127.0.0.1:1420/?compare=1&fixture=m4&page=space&preview=1", "已选"],
    ["m4-confirm", "http://127.0.0.1:1420/?compare=1&fixture=m4&page=space&preview=1&sheet=confirm", "将移到废纸篓"],
    ["m4-result", "http://127.0.0.1:1420/?compare=1&fixture=m4&page=space&preview=1&sheet=result", "清理完成"],
    ["m4-overview", "http://127.0.0.1:1420/?compare=1&fixture=m4&page=overview&preview=1", "查看清理"],
  ]) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
    await page.goto(url, { waitUntil: "networkidle" });
    await page.waitForFunction((text) => document.body.innerText.includes(text), ready);
    await page.screenshot({ path: file(`design/actual/${name}.png`).pathname, omitBackground: true });
    if (name === "m4-confirm" || name === "m4-result") {
      await page.locator(".space-dialog").screenshot({ path: file(`design/actual/${name}-card.png`).pathname });
    }
    await page.close();
  }
  await browser.close();
} finally {
  server.kill("SIGTERM");
}

function report(name, reference, regions, actualName = name) {
  const actual = windowFrame(decode(file(`design/actual/${actualName}.png`)));
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

const confirmPair = containPair(confirmRef, decode(file("design/actual/m4-confirm-card.png")));
const resultPair = containPair(resultRef, decode(file("design/actual/m4-result-card.png")));
encode(confirmPair.diff, file("design/diff/m4-confirm.png"));
encode(beside(confirmPair.left, confirmPair.right), file("design/diff/m4-confirm-side-by-side.png"));
encode(resultPair.diff, file("design/diff/m4-result.png"));
encode(beside(resultPair.left, resultPair.right), file("design/diff/m4-result-side-by-side.png"));

console.log(
  JSON.stringify(
    {
      threshold: 0.05,
      space: report("m4-space", spaceRef, spaceRegions),
      spacePage: report("m4-space-page", spacePageRef, spaceRegions, "m4-space"),
      overview: report("m4-overview", overviewRef, overviewRegions),
      confirm: { ratio: confirmPair.ratio, size: `${confirmPair.left.width}x${confirmPair.left.height}` },
      result: { ratio: resultPair.ratio, size: `${resultPair.left.width}x${resultPair.left.height}` },
    },
    null,
    2,
  ),
);
