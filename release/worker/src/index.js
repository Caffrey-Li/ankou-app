// 安扣助手下载入口（Cloudflare Worker）。
// - /download            官网下载最新安装包（可带 ?from=来源）
// - /latest.json         应用检查更新（新版本带 ?v=当前版本&a=芯片）
// - /v<版本>/<文件>       指定版本的安装包或更新包
// - /stats?key=…         统计页（只看汇总，不含任何个人信息）
// 只记：时间、类型、版本、芯片、系统、国家或地区、来源。不存 IP、不存完整 UA。

const FILE = /^(latest\.json|(?:Ankou|Wanxiang)\.dmg|v\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?\/(?:Ankou|Wanxiang)-[0-9A-Za-z.\-]+\.(?:dmg|app\.tar\.gz))$/;
const VERSION = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.]+)?$/;
const ARCH = new Set(["aarch64", "x86_64"]);
const KEEP_DAYS = 400;

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method Not Allowed", { status: 405, headers: { allow: "GET, HEAD" } });
    }
    const path = url.pathname.replace(/^\/+/, "");
    if (path === "" || path === "download") return serve(request, env, ctx, url, "Wanxiang.dmg");
    if (path === "stats") return stats(request, env, url, false);
    if (path === "stats.json") return stats(request, env, url, true);
    if (!FILE.test(path)) return new Response("Not Found", { status: 404 });
    return serve(request, env, ctx, url, path);
  },

  async scheduled(_event, env) {
    const cutoff = Date.now() - KEEP_DAYS * 86_400_000;
    await env.DB.prepare("DELETE FROM events WHERE ts < ?1").bind(cutoff).run();
  },
};

async function serve(request, env, ctx, url, key) {
  const head = request.method === "HEAD";
  const object = head ? await env.BUCKET.head(key) : await env.BUCKET.get(key, { range: request.headers, onlyIf: request.headers });
  if (!object) return new Response("Not Found", { status: 404 });

  const headers = new Headers();
  object.writeHttpMetadata(headers);
  headers.set("etag", object.httpEtag);
  headers.set("accept-ranges", "bytes");
  if (key === "latest.json") headers.set("cache-control", "no-cache, max-age=0");
  if (key.endsWith(".dmg")) {
    const name = key === "Wanxiang.dmg" ? "Wanxiang.dmg" : key.split("/").pop();
    headers.set("content-disposition", `attachment; filename="${name}"`);
  }

  // 只记完整的第一次请求：HEAD、续传的后半段、自测请求都不记
  const range = request.headers.get("range");
  const partial = range && !/^bytes=0-/.test(range);
  if (!head && !partial && request.headers.get("x-wanxiang-selftest") !== "1") {
    ctx.waitUntil(record(request, env, url, key).catch(() => undefined));
  }

  if (head || !("body" in object)) {
    headers.set("content-length", String(object.size));
    return new Response(null, { status: head ? 200 : 304, headers });
  }
  if (object.range && range) {
    const offset = object.range.offset ?? 0;
    const length = object.range.length ?? object.size - offset;
    headers.set("content-range", `bytes ${offset}-${offset + length - 1}/${object.size}`);
    headers.set("content-length", String(length));
    return new Response(object.body, { status: 206, headers });
  }
  headers.set("content-length", String(object.size));
  return new Response(object.body, { status: 200, headers });
}

async function record(request, env, url, key) {
  const now = Date.now();
  const kind = key === "latest.json" ? "check" : key.endsWith(".tar.gz") ? "update" : "download";
  let version = "unknown";
  let arch = "unknown";
  if (kind === "check") {
    const v = url.searchParams.get("v") ?? "";
    version = VERSION.test(v) ? v : "unknown";
    const a = url.searchParams.get("a") ?? "";
    arch = ARCH.has(a) ? a : "unknown";
  } else if (key === "Wanxiang.dmg") {
    version = await latestVersion(env);
  } else {
    const match = /^v([^/]+)\//.exec(key);
    version = match && VERSION.test(match[1]) ? match[1] : "unknown";
  }
  const country = (request.cf && typeof request.cf.country === "string" ? request.cf.country : "XX").slice(0, 2);
  await env.DB.prepare(
    "INSERT INTO events (ts, day, kind, version, arch, os, country, src) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
  )
    .bind(now, new Date(now).toISOString().slice(0, 10), kind, version, arch, osOf(request, kind), country, sourceOf(request, url, kind))
    .run();
}

async function latestVersion(env) {
  try {
    const object = await env.BUCKET.get("latest.json");
    if (!object) return "unknown";
    const manifest = await object.json();
    return VERSION.test(manifest.version ?? "") ? manifest.version : "unknown";
  } catch {
    return "unknown";
  }
}

/** 只取系统大类，不留完整 UA */
function osOf(request, kind) {
  if (kind !== "download") return "mac";
  const ua = (request.headers.get("user-agent") ?? "").toLowerCase();
  if (ua.includes("iphone") || ua.includes("ipad")) return "ios";
  if (ua.includes("android")) return "android";
  if (ua.includes("mac os x") || ua.includes("macintosh")) return "mac";
  if (ua.includes("windows")) return "windows";
  if (ua.includes("linux")) return "linux";
  return "other";
}

/** 来源：链接上的 from 参数优先，否则取来源网站的域名；都没有就是 direct */
function sourceOf(request, url, kind) {
  if (kind !== "download") return "app";
  const from = (url.searchParams.get("from") ?? "").trim();
  if (from) return from.replace(/[^0-9A-Za-z_\-一-鿿.]/g, "").slice(0, 32) || "direct";
  const referer = request.headers.get("referer");
  if (referer) {
    try {
      return new URL(referer).hostname.slice(0, 64) || "direct";
    } catch {
      return "direct";
    }
  }
  return "direct";
}

// —— 统计页 ——

function sameKey(given, expected) {
  if (!expected || typeof given !== "string" || given.length !== expected.length) return false;
  let diff = 0;
  for (let i = 0; i < given.length; i++) diff |= given.charCodeAt(i) ^ expected.charCodeAt(i);
  return diff === 0;
}

async function stats(request, env, url, json) {
  if (!sameKey(url.searchParams.get("key"), env.STATS_KEY)) return new Response("Not Found", { status: 404 });
  const days = Math.min(Math.max(Number(url.searchParams.get("days")) || 30, 1), 365);
  const since = new Date(Date.now() - (days - 1) * 86_400_000).toISOString().slice(0, 10);
  const week = new Date(Date.now() - 6 * 86_400_000).toISOString().slice(0, 10);
  const q = (sql, ...args) => env.DB.prepare(sql).bind(...args).all().then((r) => r.results);
  const [totals, daily, countries, sources, checkVersions, checkArch, dlVersions, dlOs] = await Promise.all([
    q("SELECT kind, COUNT(*) AS n FROM events GROUP BY kind"),
    q("SELECT day, kind, COUNT(*) AS n FROM events WHERE day >= ?1 GROUP BY day, kind ORDER BY day", since),
    q("SELECT country, COUNT(*) AS n FROM events WHERE kind = 'download' AND day >= ?1 GROUP BY country ORDER BY n DESC LIMIT 20", since),
    q("SELECT src, COUNT(*) AS n FROM events WHERE kind = 'download' AND day >= ?1 GROUP BY src ORDER BY n DESC LIMIT 20", since),
    q("SELECT version, COUNT(*) AS n FROM events WHERE kind = 'check' AND day >= ?1 GROUP BY version ORDER BY n DESC", week),
    q("SELECT arch, COUNT(*) AS n FROM events WHERE kind = 'check' AND day >= ?1 GROUP BY arch ORDER BY n DESC", week),
    q("SELECT version, COUNT(*) AS n FROM events WHERE kind IN ('download','update') AND day >= ?1 GROUP BY version ORDER BY n DESC", since),
    q("SELECT os, COUNT(*) AS n FROM events WHERE kind = 'download' AND day >= ?1 GROUP BY os ORDER BY n DESC", since),
  ]);
  const data = { days, since, totals, daily, countries, sources, checkVersions, checkArch, dlVersions, dlOs };
  if (json) return Response.json(data, { headers: { "cache-control": "no-store" } });
  return new Response(page(data), {
    headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store", "x-robots-tag": "noindex", "referrer-policy": "no-referrer" },
  });
}

const esc = (value) => String(value).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function table(title, rows, label, labelOf = (row) => row[label]) {
  const total = rows.reduce((sum, row) => sum + row.n, 0) || 1;
  const body = rows.length
    ? rows
        .map(
          (row) =>
            `<tr><td>${esc(labelOf(row))}</td><td class="bar"><i style="width:${((row.n / total) * 100).toFixed(1)}%"></i></td><td class="n">${row.n}</td></tr>`,
        )
        .join("")
    : `<tr><td colspan="3" class="empty">暂无</td></tr>`;
  return `<section><h2>${esc(title)}</h2><table>${body}</table></section>`;
}

function page(data) {
  const total = (kind) => data.totals.find((row) => row.kind === kind)?.n ?? 0;
  const sum = (kind) => data.daily.filter((row) => row.kind === kind).reduce((s, row) => s + row.n, 0);
  const days = [];
  for (let i = data.days - 1; i >= 0; i--) days.push(new Date(Date.now() - i * 86_400_000).toISOString().slice(0, 10));
  const at = (day, kind) => data.daily.find((row) => row.day === day && row.kind === kind)?.n ?? 0;
  const peak = Math.max(1, ...days.map((day) => at(day, "download") + at(day, "update")));
  const bars = days
    .map((day) => {
      const d = at(day, "download");
      const u = at(day, "update");
      return `<div class="col" title="${day}：官网下载 ${d}，应用内更新 ${u}，检查更新 ${at(day, "check")}"><i class="u" style="height:${(u / peak) * 100}%"></i><i class="d" style="height:${(d / peak) * 100}%"></i></div>`;
    })
    .join("");
  const checks7 = data.checkVersions.reduce((s, row) => s + row.n, 0);
  const archName = { aarch64: "Apple 芯片", x86_64: "Intel", unknown: "未知" };
  const osName = { mac: "macOS", windows: "Windows", linux: "Linux", ios: "iPhone / iPad", android: "Android", other: "其他" };
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="robots" content="noindex"><title>安扣助手 · 下载统计</title><style>
:root{--bg:#f2f3f5;--card:#fff;--ink:#1d1d1f;--sub:#5f6370;--line:rgba(0,0,0,.08);--a:#2f5d9a;--b:#b5652a;--fill:rgba(0,0,0,.05)}
@media (prefers-color-scheme:dark){:root{--bg:#111317;--card:#1a1d23;--ink:#ececf1;--sub:#a1a4ad;--line:rgba(255,255,255,.07);--a:#7fa6e0;--b:#e39a5e;--fill:rgba(255,255,255,.06)}}
*{box-sizing:border-box}body{margin:0;padding:28px;background:var(--bg);color:var(--ink);font:14px -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif}
h1{margin:0 0 4px;font-size:20px}p.sub{margin:0 0 20px;color:var(--sub);font-size:12px}
.cards{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;margin-bottom:16px}
.card,section{background:var(--card);border-radius:14px;padding:16px 18px}
.card span{color:var(--sub);font-size:12px}.card strong{display:block;margin-top:6px;font-size:26px}.card small{color:var(--sub);font-size:11px}
.chart{display:flex;align-items:flex-end;gap:3px;height:140px;margin-top:10px}.col{flex:1;display:flex;flex-direction:column-reverse;height:100%;background:var(--fill);border-radius:3px;overflow:hidden}
.col i{display:block}.col .d{background:var(--a)}.col .u{background:var(--b)}
.legend{display:flex;gap:14px;color:var(--sub);font-size:11px;margin-top:8px}.legend b{display:inline-block;width:8px;height:8px;border-radius:2px;margin-right:5px}
.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px;margin-top:12px}
h2{margin:0 0 10px;font-size:13px;color:var(--sub);font-weight:600}table{width:100%;border-collapse:collapse}td{padding:5px 0;border-top:1px solid var(--line);font-size:13px}
tr:first-child td{border-top:0}td.bar{width:45%;padding:0 10px}td.bar i{display:block;height:6px;border-radius:3px;background:var(--a)}td.n{text-align:right;width:3em;font-variant-numeric:tabular-nums}td.empty{color:var(--sub)}
@media (max-width:860px){.cards,.grid{grid-template-columns:1fr 1fr}}
</style></head><body>
<h1>安扣助手 · 下载统计</h1>
<p class="sub">最近 ${data.days} 天（UTC 日期，自 ${esc(data.since)}）· 只记时间、版本、芯片、系统、国家或地区、来源，不存 IP</p>
<div class="cards">
<div class="card"><span>官网下载</span><strong>${sum("download")}</strong><small>累计 ${total("download")}</small></div>
<div class="card"><span>应用内更新</span><strong>${sum("update")}</strong><small>累计 ${total("update")}</small></div>
<div class="card"><span>检查更新（近 7 天）</span><strong>${checks7}</strong><small>每台电脑启动时和每 6 小时检查一次</small></div>
<div class="card"><span>检查更新（近 ${data.days} 天）</span><strong>${sum("check")}</strong><small>只统计 0.3.0 及以后的版本</small></div>
</div>
<section><h2>每天下载</h2><div class="chart">${bars}</div><div class="legend"><span><b style="background:var(--a)"></b>官网下载</span><span><b style="background:var(--b)"></b>应用内更新</span></div></section>
<div class="grid">
${table("国家或地区（官网下载）", data.countries, "country")}
${table("来源（官网下载）", data.sources, "src")}
${table("系统（官网下载）", data.dlOs, "os", (row) => osName[row.os] ?? row.os)}
${table("下载的版本", data.dlVersions, "version")}
${table("在用版本（近 7 天检查更新）", data.checkVersions, "version")}
${table("芯片（近 7 天检查更新）", data.checkArch, "arch", (row) => archName[row.arch] ?? row.arch)}
</div>
</body></html>`;
}
