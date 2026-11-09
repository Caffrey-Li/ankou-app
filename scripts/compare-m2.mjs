import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { PNG } from "pngjs";
import pixelmatch from "pixelmatch";
import { chromium } from "playwright";

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

function cropRect(source, x, y, width, height) {
  const png = new PNG({ width, height });
  PNG.bitblt(source, png, x, y, width, height, 0, 0);
  return png;
}

function windowFrame(source, width, height) {
  const x = Math.max(0, Math.round((source.width - width) / 2));
  const y = Math.max(0, Math.round((source.height - height) / 2));
  return cropRect(source, x, y, Math.min(width, source.width - x), Math.min(height, source.height - y));
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
  const png = new PNG({
    width: left.width + gap + right.width,
    height: Math.max(left.height, right.height),
  });
  png.data.fill(242);
  PNG.bitblt(left, png, 0, 0, left.width, left.height, 0, 0);
  PNG.bitblt(right, png, 0, 0, right.width, right.height, left.width + gap, 0);
  return png;
}

function diffPair(expected, actual) {
  const right = flatten(actual);
  const left = scale(flatten(expected), right.width, right.height);
  const diff = new PNG({ width: left.width, height: left.height });
  const mismatched = pixelmatch(left.data, right.data, diff.data, left.width, left.height, {
    threshold: 0.05,
    includeAA: false,
  });
  return {
    left,
    right,
    diff,
    mismatched,
    ratio: Number((mismatched / (left.width * left.height)).toFixed(4)),
  };
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

function cropBox(source, box) {
  const png = new PNG({ width: box.w, height: box.h });
  PNG.bitblt(source, png, box.x, box.y, box.w, box.h, 0, 0);
  return png;
}

function componentRatio(reference, actual, boxes) {
  const left = flatten(cropBox(reference, boxes.reference));
  const right = scale(flatten(cropBox(actual, boxes.actual)), left.width, left.height);
  const diff = new PNG({ width: left.width, height: left.height });
  const mismatched = pixelmatch(left.data, right.data, diff.data, left.width, left.height, {
    threshold: 0.05,
    includeAA: false,
  });
  return { mismatched, ratio: Number((mismatched / (left.width * left.height)).toFixed(4)), size: `${left.width}x${left.height}` };
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

const usageRegions = {
  工具栏: { x: 250 / 1280, y: 16 / 980, w: 990 / 1280, h: 52 / 980 },
  提示条: { x: 272 / 1280, y: 76 / 980, w: 970 / 1280, h: 50 / 980 },
  额度卡: { x: 272 / 1280, y: 168 / 980, w: 970 / 1280, h: 136 / 980 },
  图表: { x: 272 / 1280, y: 322 / 980, w: 970 / 1280, h: 313 / 980 },
  明细: { x: 272 / 1280, y: 655 / 980, w: 970 / 1280, h: 324 / 980 },
};

const overviewRegions = {
  工具栏: { x: 250 / 1280, y: 16 / 900, w: 990 / 1280, h: 54 / 900 },
  需要处理: { x: 272 / 1280, y: 79 / 900, w: 970 / 1280, h: 257 / 900 },
  额度卡: { x: 272 / 1280, y: 358 / 900, w: 312 / 1280, h: 211 / 900 },
  对话与空间: { x: 601 / 1280, y: 358 / 900, w: 641 / 1280, h: 211 / 900 },
  工具列表: { x: 272 / 1280, y: 589 / 900, w: 970 / 1280, h: 273 / 900 },
};

const shiftedComponents = {
  额度卡: {
    reference: { x: 272, y: 358, w: 312, h: 211 },
    actual: { x: 272, y: 211, w: 312, h: 206 },
  },
  工具列表: {
    reference: { x: 272, y: 589, w: 970, h: 273 },
    actual: { x: 272, y: 434, w: 970, h: 303 },
  },
};

mkdirSync(file("design/refs"), { recursive: true });
mkdirSync(file("design/actual"), { recursive: true });
mkdirSync(file("design/diff"), { recursive: true });

const usageRef = windowFrame(decode(file("design/refs/m2-usage-18-320.png")), 1280, 980);
const overviewRef = windowFrame(decode(file("design/refs/m2-overview-18-70.png")), 1280, 900);
encode(usageRef, file("design/refs/usage.png"));
encode(overviewRef, file("design/refs/overview.png"));

async function shoot(browser, name, url, width, height) {
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
  await page.goto(url, { waitUntil: "networkidle" });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await document.fonts.load("700 22px 'Noto Sans SC'");
    await document.fonts.load("500 13px 'Noto Sans SC'");
    await document.fonts.load("400 12px 'Noto Sans SC'");
  });
  if (url.includes("fixture=m2")) {
    await page.waitForFunction(
      () =>
        document.documentElement.dataset.fixture === "m2" &&
        (document.body.innerText.includes("6%") || document.body.innerText.includes("已授权")),
    );
  }
  await page.screenshot({ path: file(`design/actual/${name}.png`).pathname, omitBackground: true });
  await page.close();
}

const server = await waitForServer();
try {
  const browser = await chromium.launch();
  await shoot(browser, "usage", "http://127.0.0.1:1420/?compare=1&fixture=m2&page=usage", 1280, 980);
  await shoot(browser, "overview", "http://127.0.0.1:1420/?compare=1&fixture=m2&page=overview", 1280, 900);
  await shoot(browser, "usage-empty", "http://127.0.0.1:1420/?compare=1&page=usage", 1280, 980);
  await shoot(browser, "overview-empty", "http://127.0.0.1:1420/?compare=1&page=overview", 1280, 900);
  await browser.close();
} finally {
  server.kill("SIGTERM");
}

const usage = diffPair(usageRef, decode(file("design/actual/usage.png")));
encode(usage.diff, file("design/diff/usage.png"));
encode(beside(usage.left, usage.right), file("design/diff/usage-side-by-side.png"));
const overview = diffPair(overviewRef, decode(file("design/actual/overview.png")));
encode(overview.diff, file("design/diff/overview.png"));
encode(beside(overview.left, overview.right), file("design/diff/overview-side-by-side.png"));
const usageAreas = Object.fromEntries(
  Object.entries(usageRegions).map(([name, box]) => [name, regionRatio(usage.left, usage.right, box)]),
);
const overviewAreas = Object.fromEntries(
  Object.entries(overviewRegions).map(([name, box]) => [name, regionRatio(overview.left, overview.right, box)]),
);
const overviewRefImage = decode(file("design/refs/overview.png"));
const overviewActualImage = decode(file("design/actual/overview.png"));
const componentAreas = Object.fromEntries(
  Object.entries(shiftedComponents).map(([name, boxes]) => [name, componentRatio(overviewRefImage, overviewActualImage, boxes)]),
);

console.log(
  JSON.stringify(
    {
      threshold: 0.05,
      usage: { mismatched: usage.mismatched, ratio: usage.ratio, size: `${usageRef.width}x${usageRef.height}`, regions: usageAreas },
      overview: {
        mismatched: overview.mismatched,
        ratio: overview.ratio,
        size: `${overviewRef.width}x${overviewRef.height}`,
        regions: overviewAreas,
        componentAligned: componentAreas,
      },
      emptyState: ["design/actual/usage-empty.png", "design/actual/overview-empty.png"],
      numberDifferences: [
        "稿面 Codex 62% / 34%，夹具是 rollout 解析出的 6% / 1%",
        "稿面余额 ¥128.40，夹具 balance.json 是 ¥12.34",
        "稿面合计 3.49M 和 11 月 1–9 日，夹具是 10 月 1–25 日的解析合计",
        "稿面时间 17:05 / 21:40，夹具时间来自样本时间戳",
      ],
    },
    null,
    2,
  ),
);
