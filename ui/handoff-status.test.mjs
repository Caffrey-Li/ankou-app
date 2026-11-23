import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

const server = await createServer({
  configFile: new URL("./vite.config.ts", import.meta.url).pathname,
  server: { middlewareMode: true },
  appType: "custom",
  logLevel: "error",
});

const statuses = ["unauthorized", "needs_reauth", "read_failed", "server_unavailable", "no_records", "ok"];

const reason = {
  unauthorized: "未授权，可在设置里开启",
  needs_reauth: "授权需要重新确认，可在设置里处理",
  read_failed: "读取失败，没有读到本机会话",
  server_unavailable: "ChatGPT 应用未运行，没有读到本机会话",
  no_records: "没有本机对话",
};

const alertStatuses = new Set(["unauthorized", "needs_reauth", "read_failed", "server_unavailable"]);

try {
  const { HandoffPage, handoffEmptyLines, handoffSubtitle, sourcesWhenListFails } = await server.ssrLoadModule(
    "/src/HandoffPage.tsx",
  );
  const row = {
    id: "demo",
    tool: "codex",
    title: "示例",
    updated: "2026-01-01T00:00:00Z",
    percent: 10,
    status: "",
    cwd: null,
    context: "",
    source: "jsonl",
  };

  for (const codex of statuses) {
    for (const ds_bot of statuses) {
      const sources = { codex, ds_bot };
      const subtitle = handoffSubtitle([], sources, 80);
      const lines = handoffEmptyLines(sources);
      if (codex === "unauthorized" && ds_bot === "unauthorized") {
        assert.equal(subtitle, "未授权", `${codex}/${ds_bot}`);
        assert.equal(/\d/.test(subtitle), false);
      } else {
        assert.equal(subtitle, "0 条对话 · 0 条超过提醒阈值", `${codex}/${ds_bot}`);
      }
      for (const [status, label] of [
        [codex, "ChatGPT"],
        [ds_bot, "DS Bot"],
      ]) {
        if (status === "ok") {
          assert.equal(
            lines.some((line) => line.text.startsWith(`${label}：`)),
            false,
            `${label} ok`,
          );
        } else {
          const line = lines.find((item) => item.text === `${label}：${reason[status]}`);
          assert.ok(line, `${label} ${status} ${lines.map((item) => item.text)}`);
          assert.equal(line.tone, status === "no_records" ? "note" : "alert", `${label} ${status}`);
        }
      }
      const html = renderToStaticMarkup(
        createElement(HandoffPage, {
          rows: [],
          sources,
          threshold: 80,
          filter: "all",
          query: "",
          sheet: "",
          previewRequest: 0,
          onQuery() {},
        }),
      );
      for (const line of lines) {
        const klass = line.tone === "note" ? "handoff-note" : "handoff-alert";
        assert.ok(html.includes(`<p class="${klass}"><span>${line.text}</span>`), `${line.text} ${html}`);
      }
      if (codex !== "no_records" && ds_bot !== "no_records") {
        assert.equal(html.includes("handoff-note"), false, `${codex}/${ds_bot}`);
      }
      if (!alertStatuses.has(codex) && !alertStatuses.has(ds_bot)) {
        assert.equal(html.includes("handoff-alert"), false, `${codex}/${ds_bot}`);
      }
      if (codex !== "server_unavailable" && ds_bot !== "server_unavailable") {
        assert.equal(html.includes(reason.server_unavailable), false, `${codex}/${ds_bot}`);
      }
    }
  }

  const counted = handoffSubtitle([row, { ...row, id: "two", percent: 90 }], {
    codex: "ok",
    ds_bot: "unauthorized",
  }, 80);
  assert.equal(counted, "2 条对话 · 1 条超过提醒阈值");

  assert.equal(handoffSubtitle([], null, 80), "");
  assert.deepEqual(handoffEmptyLines(null), []);
  const loading = renderToStaticMarkup(
    createElement(HandoffPage, {
      rows: [],
      sources: null,
      threshold: 80,
      filter: "all",
      query: "",
      sheet: "",
      previewRequest: 0,
      onQuery() {},
    }),
  );
  for (const phrase of [...Object.values(reason), "未授权", "条对话", "超过提醒阈值"]) {
    assert.equal(loading.includes(phrase), false, phrase);
  }

  const failed = sourcesWhenListFails();
  assert.deepEqual(failed, {
    codex: "read_failed",
    ds_bot: "read_failed",
    harness: "read_failed",
    cursor: "read_failed",
    claude: "read_failed",
    antigravity: "read_failed",
    workbuddy: "read_failed",
    grok_bot: "read_failed",
    codebuddy: "read_failed",
    cursor_cli: "read_failed",
  });
  assert.notEqual(failed.codex, "unauthorized");
  assert.notEqual(failed.ds_bot, "unauthorized");
  assert.equal(handoffSubtitle([], failed, 80), "0 条对话 · 0 条超过提醒阈值");

  const app = readFileSync(new URL("./src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /useState<ConversationSources \| null>\(null\)/);
  assert.match(app, /listConversations\(\)[\s\S]*?\.catch\(/);
  assert.match(app, /sourcesWhenListFails\(\)/);

  const filled = renderToStaticMarkup(
    createElement(HandoffPage, {
      rows: [row],
      sources: { codex: "ok", ds_bot: "no_records" },
      threshold: 80,
      filter: "all",
      query: "",
      sheet: "",
      previewRequest: 0,
      onQuery() {},
    }),
  );
  assert.equal(filled.includes("没有本机对话"), false);
  assert.equal(filled.includes("未授权，可在设置里开启"), false);
  assert.ok(filled.includes("示例"));
} finally {
  await server.close();
}
