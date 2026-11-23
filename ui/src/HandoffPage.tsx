import { useEffect, useMemo, useRef, useState } from "react";
import {
  beginSummary,
  exportHandoff,
  inTauri,
  organizeHandoff,
  placeDeeplink,
  prepareDeeplink,
  prepareHandoff,
  sendSummary,
  type SourceStatus,
} from "./api";
import { Check } from "./Check";
import rowsFromFile from "./fixture-m3.json";
import { IconArrow, IconCheck, IconChevron, IconClose, IconCopy, IconSearch, IconSpark } from "./icons";
import { ToolIcon } from "./tool-status";
import "./handoff.css";
import "./usage.css";

export type HandoffFilter = "all" | "attention" | "recent";

export type HandoffRow = {
  id: string;
  tool: string;
  title: string;
  updated: string;
  clock?: string;
  percent: number | null;
  status: string;
  cwd: string | null;
  context: string;
  source: string;
};

const fixtureRows = rowsFromFile as HandoffRow[];

export const handoffCopy = {
  missingServer: "ChatGPT 应用未运行，没有读到本机会话",
  deleted: "这条对话已经不在，无法交接",
  tooLong: "超过 8000 字，不能打开深链。全文仍可复制。",
  summaryFailed: "摘要没有生成。编辑还在，可以改用本地整理或重试。",
  deeplinkFailed: "未能放入：深链没有打开 · 编辑已保留",
  truncated: "长对话只取了最近部分",
  withheld: "暂不提供",
};

const sectionNames = ["目标", "进展", "未完成", "关键决策和约束", "下一步", "不确定事项", "资料位置"] as const;

/** 浏览器预览（设计稿对照）用的示例内容；应用里一律用真实对话生成。 */
const sampleSections = [
  "完成夹具里列出的下一步。",
  "· 列表和比例已能显示",
  "· 真实验证尚未进行",
  "· 正文只留在内存里",
  "1. 看预览再决定是否放入",
  "· 比例未知的对话仍可交接",
  "夹具，不读取本机对话库",
];

type Organized = {
  goal: string;
  progress: string;
  unfinished: string;
  decisions: string;
  next: string;
  unknown: string;
  sources: string;
  note: string;
};

export function sectionsFrom(organized: Organized, source: string) {
  const sources = [source, organized.sources].filter((part) => part.trim()).join("\n");
  return [
    organized.goal,
    organized.progress,
    organized.unfinished,
    organized.decisions,
    organized.next,
    organized.unknown,
    sources,
  ].map((text) => text.trim());
}

export function composeSections(texts: string[]) {
  return sectionNames
    .map((name, index) => [name, (texts[index] ?? "").trim()] as const)
    .filter(([, text]) => text)
    .map(([name, text]) => `${name}\n${text}`)
    .join("\n\n");
}

export type ConversationSources = {
  codex: SourceStatus;
  ds_bot: SourceStatus;
  harness?: SourceStatus;
  cursor?: SourceStatus;
  claude?: SourceStatus;
  antigravity?: SourceStatus;
  workbuddy?: SourceStatus;
  grok_bot?: SourceStatus;
  codebuddy?: SourceStatus;
  cursor_cli?: SourceStatus;
};

const emptyReason: Record<Exclude<SourceStatus, "ok">, string> = {
  unauthorized: "未授权，可在设置里开启",
  needs_reauth: "授权需要重新确认，可在设置里处理",
  read_failed: "读取失败，没有读到本机会话",
  server_unavailable: handoffCopy.missingServer,
  no_records: "没有本机对话",
};

export function handoffSubtitle(rows: HandoffRow[], sources: ConversationSources | null, threshold: number) {
  if (!sources) return "";
  if (
    sources.codex === "unauthorized" &&
    sources.ds_bot === "unauthorized" &&
    (sources.harness ?? "unauthorized") === "unauthorized" &&
    (sources.claude ?? "unauthorized") === "unauthorized" &&
    (sources.antigravity ?? "unauthorized") === "unauthorized" &&
    (sources.workbuddy ?? "unauthorized") === "unauthorized" &&
    (sources.grok_bot ?? "unauthorized") === "unauthorized" &&
    (sources.codebuddy ?? "unauthorized") === "unauthorized" &&
    (sources.cursor_cli ?? "unauthorized") === "unauthorized" &&
    (sources.cursor ?? "unauthorized") === "unauthorized"
  )
    return "未授权";
  const over = rows.filter((row) => row.percent != null && row.percent >= threshold).length;
  return `${rows.length} 条对话 · ${over} 条超过提醒阈值`;
}

export function sourcesWhenListFails(): ConversationSources {
  return {
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
  };
}

export function handoffEmptyLines(sources: ConversationSources | null) {
  if (!sources) return [];
  const tools: [string, string, SourceStatus][] = [
    ...(sources.harness ? [["deepseek_harness", "DeepSeek Harness", sources.harness] as [string, string, SourceStatus]] : []),
    ["codex", "ChatGPT", sources.codex],
    ["ds_bot", "DS Bot", sources.ds_bot],
    ...(sources.cursor ? [["cursor", "Cursor", sources.cursor] as [string, string, SourceStatus]] : []),
    ...(sources.claude ? [["claude", "Claude", sources.claude] as [string, string, SourceStatus]] : []),
    ...(sources.antigravity
      ? [["antigravity", "Antigravity", sources.antigravity] as [string, string, SourceStatus]]
      : []),
    ...(sources.workbuddy ? [["workbuddy", "WorkBuddy", sources.workbuddy] as [string, string, SourceStatus]] : []),
    ...(sources.grok_bot ? [["grok_bot", "Grok Bot", sources.grok_bot] as [string, string, SourceStatus]] : []),
    ...(sources.cursor_cli
      ? [["cursor_cli", "Cursor 命令行", sources.cursor_cli] as [string, string, SourceStatus]]
      : []),
    ...(sources.codebuddy ? [["codebuddy", "CodeBuddy", sources.codebuddy] as [string, string, SourceStatus]] : []),
  ];
  return tools.flatMap(([tool, label, status]) => {
    if (status === "ok") return [];
    const tone = status === "no_records" ? "note" : "alert";
    return [{ tool, text: `${label}：${emptyReason[status]}`, tone }];
  });
}

export function HandoffFilters({
  filter,
  onFilter,
}: {
  filter: HandoffFilter;
  onFilter: (filter: HandoffFilter) => void;
}) {
  const items: { id: HandoffFilter; label: string }[] = [
    { id: "all", label: "全部" },
    { id: "attention", label: "需要处理" },
    { id: "recent", label: "最近" },
  ];
  return (
    <div className="range-pills" role="group" aria-label="对话筛选">
      {items.map((item) => (
        <button key={item.id} type="button" aria-pressed={filter === item.id} onClick={() => onFilter(item.id)}>
          {item.label}
        </button>
      ))}
    </div>
  );
}

export function HandoffPage({
  rows,
  sources,
  threshold,
  filter,
  query,
  sheet,
  previewRequest,
  onQuery,
  defaultMode = "ai",
  searchRequest = 0,
  onSettings = () => undefined,
  focus,
}: {
  /** 从总览或工具页点「交接」带过来的对话 */
  focus?: { tool: string; id: string; at: number };
  rows: HandoffRow[];
  sources: ConversationSources | null;
  threshold: number;
  filter: HandoffFilter;
  query: string;
  sheet: "" | "preview" | "confirm";
  previewRequest: number;
  onQuery: (query: string) => void;
  defaultMode?: "local" | "ai";
  searchRequest?: number;
  onSettings?: () => void;
}) {
  const [selectedId, setSelectedId] = useState(rows[0]?.id ?? "");
  const [mode, setMode] = useState<"local" | "ai">(defaultMode);
  const [open, setOpen] = useState<"" | "preview" | "confirm">(sheet);
  const [notice, setNotice] = useState("");
  const [sheetTexts, setSheetTexts] = useState<string[]>(() =>
    inTauri() ? sectionNames.map(() => "") : sampleSections,
  );
  const [transcript, setTranscript] = useState("");
  const [deeplinkToken, setDeeplinkToken] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (focus) setSelectedId(focus.id);
  }, [focus]);
  useEffect(() => {
    if (searchRequest > 0) searchRef.current?.focus();
  }, [searchRequest]);
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const matched = rows.filter(
      (row) =>
        !needle ||
        row.title.toLowerCase().includes(needle) ||
        toolLabel(row.tool).toLowerCase().includes(needle),
    );
    if (filter === "attention") {
      return matched.filter((row) => row.percent != null && row.percent >= threshold);
    }
    if (filter === "recent") {
      // 最近 7 天有更新的对话，新的在前
      const week = Date.now() - 7 * 86_400_000;
      return matched
        .map((row) => ({ row, at: parseUpdated(row.updated)?.getTime() ?? null }))
        .filter((item) => item.at == null || item.at >= week)
        .sort((a, b) => (b.at ?? 0) - (a.at ?? 0))
        .map((item) => item.row);
    }
    return matched;
  }, [filter, query, rows, threshold]);
  const selected = rows.find((row) => row.id === selectedId) ?? visible[0];
  const previewText = useMemo(() => composeSections(sheetTexts), [sheetTexts]);
  const sourceLine = selected ? `${toolLabel(selected.tool)} · ${selected.title}` : "";
  useEffect(() => {
    if (previewRequest > 0) void generate();
  }, [previewRequest]);
  useEffect(() => {
    if (open !== "preview" || !inTauri() || toolKind(selected?.tool ?? "") !== "cursor") return;
    const text = previewText;
    let cancelled = false;
    prepareDeeplink(text).then(
      (token) => {
        if (!cancelled) setDeeplinkToken(token);
      },
      (error: unknown) => {
        if (cancelled) return;
        setDeeplinkToken("");
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes("8000")) setNotice(handoffCopy.tooLong);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [open, previewText, selected?.tool]);

  async function generate() {
    if (!selected) return;
    if (!inTauri()) {
      setSheetTexts(sampleSections);
      setOpen("preview");
      return;
    }
    try {
      const next = await prepareHandoff(toolKind(selected.tool), selected.id);
      if (next.note === handoffCopy.withheld) {
        setNotice(handoffCopy.withheld);
        return;
      }
      setTranscript(next.text);
      setNotice(next.truncated ? handoffCopy.truncated : next.note);
      if (mode === "ai") {
        setOpen("confirm");
        return;
      }
      await fillLocal(next.text);
      setOpen("preview");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : handoffCopy.deleted);
    }
  }

  async function fillLocal(text: string) {
    const organized = await organizeHandoff(text);
    setSheetTexts(sectionsFrom(organized, sourceLine));
  }

  async function confirmSend() {
    if (!inTauri()) {
      setOpen("preview");
      return;
    }
    try {
      const preview = await beginSummary(transcript);
      const text = await sendSummary(preview.token, preview.body);
      const organized = await organizeHandoff(text);
      setSheetTexts(sectionsFrom(organized, sourceLine));
      setOpen("preview");
    } catch {
      setNotice(handoffCopy.summaryFailed);
      setOpen("");
    }
  }

  async function copyText() {
    await navigator.clipboard?.writeText(previewText);
    setNotice("已复制，请手动粘贴");
  }

  async function place() {
    const text = previewText;
    if (!inTauri() || !deeplinkToken) {
      await navigator.clipboard?.writeText(text);
      setNotice("已复制，到 Cursor 新对话里粘贴");
      return;
    }
    try {
      await placeDeeplink(deeplinkToken, text);
      setDeeplinkToken("");
      setNotice("已放入 Cursor 新对话 · 请检查后发送");
    } catch {
      setNotice(handoffCopy.deeplinkFailed);
    }
  }

  function editSection(index: number, text: string) {
    setSheetTexts((current) => current.map((value, at) => (at === index ? text : value)));
  }

  return (
    <div className="handoff">
      <section className="handoff-list" aria-label="对话列表">
        <label className="handoff-search">
          <IconSearch />
          <input
            ref={searchRef}
            placeholder="搜索对话"
            value={query}
            onChange={(event) => onQuery(event.target.value)}
          />
        </label>
        <div className="handoff-rows">
          {visible.map((row) => (
            <button
              key={row.id}
              type="button"
              className="handoff-row"
              aria-pressed={row.id === selected?.id}
              onClick={() => setSelectedId(row.id)}
            >
              <ToolIcon id={toolKind(row.tool)} className="handoff-mark" />
              <span>
                <strong>{row.title}</strong>
                <small>
                  {toolLabel(row.tool)} · {updatedLabel(row.updated)}
                </small>
              </span>
              <span className="handoff-meter">
                {row.percent == null ? (
                  <>
                    <small>{row.context === handoffCopy.withheld ? handoffCopy.withheld : "未知"}</small>
                    <small>仍可交接</small>
                  </>
                ) : (
                  <>
                    <b className={row.percent >= threshold ? "hot" : ""}>{row.percent}%</b>
                    <span className={row.percent >= threshold ? "handoff-track hot" : "handoff-track"}>
                      <span style={{ width: `${Math.min(100, row.percent)}%` }} />
                    </span>
                  </>
                )}
              </span>
            </button>
          ))}
        </div>
        <p className="handoff-foot">比例未知的对话也可以交接 · DS Bot 群聊会保留成员角色</p>
      </section>
      {selected ? (
        <section className="handoff-detail" aria-label="对话详情">
          <div className="handoff-head">
            <ToolIcon id={toolKind(selected.tool)} className="handoff-mark" />
            <div>
              <h2>{selected.title}</h2>
              <p>
                {toolLabel(selected.tool)}
                {selected.status ? <span className="handoff-chip">{selected.status}</span> : null}
              </p>
            </div>
          </div>
          <div className="handoff-stats">
            <div
              className={selected.percent != null && selected.percent >= threshold ? "handoff-ring hot" : "handoff-ring"}
              style={{ ["--pct" as string]: String(selected.percent ?? 0) }}
            >
              <span>
                {selected.percent == null ? "未知" : `${selected.percent}%`}
                <small>上下文</small>
              </span>
            </div>
            <div>
              <p className="handoff-meta">
                <span>已用</span>
                <b>{selected.context || (selected.percent == null ? "未知" : `${selected.percent}%`)}</b>
              </p>
              <p className="handoff-meta">
                <span>提醒阈值</span>
                <b>
                  {threshold}%{selected.percent != null && selected.percent >= threshold ? " · 已超过" : ""}
                </b>
              </p>
              {selected.cwd ? (
                <p className="handoff-meta">
                  <span>工作目录</span>
                  <b>{selected.cwd}</b>
                </p>
              ) : null}
              <p className="handoff-meta">
                <span>数据来源</span>
                <b>{selected.source}</b>
              </p>
            </div>
          </div>
          <div className="handoff-actions">
            <h3>继续这段工作</h3>
            <button type="button" className="handoff-choice recommended" onClick={() => void generate()}>
              <span>
                <strong>在 Cursor 新对话中继续</strong>
                <small>生成交接内容，复制后在 Cursor 新对话里粘贴</small>
              </span>
              <em>推荐</em>
            </button>
            <button type="button" className="handoff-choice" onClick={() => void generate()}>
              <span>
                <strong>换到其他工具</strong>
                <small>ChatGPT、DS Bot、Harness · 复制后手动粘贴</small>
              </span>
              <IconChevron />
            </button>
          </div>
          <div className="handoff-note">
            <strong>交接不会改动原对话</strong>
            <p>生成的内容先给你预览和编辑；粘贴后由你检查并发送。完成或取消后，万象不保留原文与摘要。</p>
          </div>
          {notice ? <p className="handoff-alert">{notice}</p> : null}
          <div className="handoff-bar">
            <div className="range-pills" role="group" aria-label="整理方式">
              <button type="button" aria-pressed={mode === "local"} onClick={() => setMode("local")}>
                本地整理
              </button>
              <button type="button" aria-pressed={mode === "ai"} onClick={() => setMode("ai")}>
                AI 摘要
              </button>
            </div>
            <p>AI 摘要发送前会告诉你发送范围</p>
            <button type="button" className="primary-button" onClick={() => void generate()}>
              <IconSpark />
              生成交接内容
            </button>
          </div>
        </section>
      ) : (
        <section className="handoff-detail">
          {handoffEmptyLines(sources).map((line) => (
            <p key={line.tool} className={line.tone === "note" ? "handoff-note" : "handoff-alert"}>
              <span>{line.text}</span>
              {line.text.includes("未授权") || line.text.includes("重新") ? (
                <button type="button" className="text-link" onClick={onSettings}>
                  去设置
                </button>
              ) : null}
            </p>
          ))}
        </section>
      )}
      {open === "preview" ? (
        <div className="handoff-mask">
          <section className="handoff-sheet" aria-label="交接内容">
            <header>
              <h2>交接内容</h2>
              <div className="sheet-steps">
                <span className="done">
                  <IconCheck />
                  选择
                </span>
                <span className="done">
                  <IconCheck />
                  生成
                </span>
                <span className="now">
                  <i />
                  预览
                </span>
                <span>
                  <i />
                  放入
                </span>
              </div>
              <button type="button" className="sheet-close" aria-label="关闭" onClick={() => setOpen("")}>
                <IconClose />
              </button>
            </header>
            <p className="handoff-source">
              {toolLabel(selected?.tool ?? "")} · {selected?.title}（{snapshotClock(selected)} 快照）→{" "}
              {toolKind(selected?.tool ?? "") === "cursor" ? "Cursor 新对话" : "复制后手动粘贴"}
            </p>
            {notice === handoffCopy.truncated ? <p className="handoff-alert">{handoffCopy.truncated}</p> : null}
            <label className="sheet-wide">
              <span>{sectionNames[0]}</span>
              <textarea
                value={sheetTexts[0]}
                rows={rowsFor(sheetTexts[0], 52)}
                onChange={(event) => editSection(0, event.target.value)}
              />
            </label>
            <div className="sheet-grid">
              {sectionNames.slice(1).map((name, offset) => (
                <label key={name}>
                  {name}
                  <textarea
                    value={sheetTexts[offset + 1]}
                    placeholder="无"
                    rows={rowsFor(sheetTexts[offset + 1])}
                    onChange={(event) => editSection(offset + 1, event.target.value)}
                  />
                </label>
              ))}
            </div>
            <footer>
              <span>
                {previewText.length} 字
                <br />
                <small>关闭后不保留原文与摘要</small>
              </span>
              <button type="button" className="quiet-button" onClick={() => void copyText()}>
                <IconCopy />
                复制
              </button>
              <button
                type="button"
                className="quiet-button"
                onClick={() => {
                  if (!inTauri()) {
                    setNotice("先选择保存位置");
                    return;
                  }
                  void exportHandoff(previewText);
                }}
              >
                导出
              </button>
              <button type="button" className="primary-button" onClick={() => void place()}>
                <IconArrow />
                {deeplinkToken ? "放入 Cursor 新对话" : "复制，去 Cursor 粘贴"}
              </button>
            </footer>
          </section>
        </div>
      ) : null}
      {open === "confirm" ? (
        <div className="handoff-mask">
          <section className="handoff-dialog" aria-label="发送给 AI 生成摘要？">
            <h2>发送给 AI 生成摘要？</h2>
            <p>
              将把「{selected?.title}」的快照发送到 DeepSeek API。不含附件与其他对话，万象不保存。字数在发送前按请求体计算。
            </p>
            <label>
              <Check defaultChecked /> 只对本次有效
            </label>
            <button type="button" className="primary-button" onClick={() => void confirmSend()}>
              发送并生成
            </button>
            <button
              type="button"
              className="quiet-button"
              onClick={() => {
                setMode("local");
                void fillLocal(transcript).then(() => setOpen("preview"));
              }}
            >
              改用本地整理
            </button>
            <button type="button" className="quiet-button" onClick={() => setOpen("")}>
              取消
            </button>
          </section>
        </div>
      ) : null}
    </div>
  );
}

function snapshotClock(row: HandoffRow | undefined) {
  if (!row) return "";
  if (row.clock) return row.clock;
  const clock = row.updated.match(/\d{1,2}:\d{2}/);
  if (clock) return clock[0].padStart(5, "0");
  if (/^\d{11,}$/.test(row.updated)) return formatClock(new Date(Number(row.updated)));
  if (/^\d{10}$/.test(row.updated)) return formatClock(new Date(Number(row.updated) * 1000));
  const parsed = Date.parse(row.updated);
  if (!Number.isNaN(parsed) && /[T-]/.test(row.updated)) return formatClock(new Date(parsed));
  return row.updated;
}

export function parseUpdated(updated: string): Date | null {
  if (/^\d{11,}$/.test(updated)) return new Date(Number(updated));
  if (/^\d{9,10}$/.test(updated)) return new Date(Number(updated) * 1000);
  if (/^\d{4}-\d{2}-\d{2}/.test(updated)) {
    const parsed = Date.parse(updated);
    return Number.isNaN(parsed) ? null : new Date(parsed);
  }
  return null;
}

/** 列表里的时间：今天显示时刻，昨天显示「昨天」，更早显示月日。 */
export function updatedLabel(updated: string, now = new Date()) {
  const date = parseUpdated(updated);
  if (!date) return updated;
  const day = (value: Date) => new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  const diff = Math.round((day(now) - day(date)) / 86_400_000);
  if (diff <= 0) return formatClock(date);
  if (diff === 1) return `昨天 ${formatClock(date)}`;
  if (date.getFullYear() === now.getFullYear()) return `${date.getMonth() + 1} 月 ${date.getDate()} 日`;
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
}

/** 按字数估算换行后的行数（中文约 13px，半栏一行约 24 字，整栏约 52 字）。 */
export function rowsFor(text: string, perLine = 24) {
  const lines = text
    .split("\n")
    .reduce((sum, line) => sum + Math.max(1, Math.ceil(visualWidth(line) / perLine)), 0);
  return Math.min(8, Math.max(1, lines));
}

function visualWidth(line: string) {
  let width = 0;
  for (const char of line) width += char.charCodeAt(0) > 0x2e80 ? 1 : 0.55;
  return width;
}

function formatClock(date: Date) {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function toolKind(tool: string) {
  const name = tool.toLowerCase();
  if (name === "codex") return "codex";
  if (name === "ds bot" || name === "ds_bot") return "ds_bot";
  if (name.includes("harness")) return "deepseek_harness";
  if (name === "claude") return "claude";
  if (name === "antigravity") return "antigravity";
  if (name === "workbuddy") return "workbuddy";
  if (name === "grok_bot") return "grok_bot";
  if (name === "codebuddy") return "codebuddy";
  if (name === "cursor_cli") return "cursor_cli";
  return "cursor";
}

function toolLabel(tool: string) {
  const kind = toolKind(tool);
  if (kind === "codex") return "ChatGPT";
  if (kind === "ds_bot") return "DS Bot";
  if (kind === "deepseek_harness") return "DeepSeek Harness";
  if (kind === "claude") return "Claude";
  if (kind === "antigravity") return "Antigravity";
  if (kind === "workbuddy") return "WorkBuddy";
  if (kind === "grok_bot") return "Grok Bot";
  if (kind === "codebuddy") return "CodeBuddy";
  if (kind === "cursor_cli") return "Cursor 命令行";
  return "Cursor";
}

export function fixtureHandoffRows() {
  return fixtureRows;
}
