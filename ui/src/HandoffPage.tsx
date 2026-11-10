import { useEffect, useMemo, useState } from "react";
import {
  beginSummary,
  exportHandoff,
  inTauri,
  organizeHandoff,
  placeDeeplink,
  prepareDeeplink,
  prepareHandoff,
  sendSummary,
} from "./api";
import rowsFromFile from "./fixture-m3.json";
import { IconArrow, IconCheck, IconChevron, IconClose, IconCopy, IconSearch } from "./icons";
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
  missingServer: "Codex 应用未运行，没有读到本机会话",
  deleted: "这条对话已经不在，无法交接",
  tooLong: "超过 8000 字，不能打开深链。全文仍可复制。",
  summaryFailed: "摘要没有生成。编辑还在，可以改用本地整理或重试。",
  deeplinkFailed: "未能放入：深链没有打开 · 编辑已保留",
  truncated: "长对话只取了最近部分",
  withheld: "暂不提供",
};

const sections = [
  ["目标", "完成夹具里列出的下一步。"],
  ["进展", "· 列表和比例已能显示"],
  ["未完成", "· 真实验证尚未进行"],
  ["关键决策和约束", "· 正文只留在内存里"],
  ["下一步", "1. 看预览再决定是否放入"],
  ["不确定事项", "· 比例未知的对话仍可交接"],
  ["资料位置", "夹具，不读取本机对话库"],
] as const;

export function handoffSubtitle(rows: HandoffRow[], threshold: number) {
  const over = rows.filter((row) => row.percent != null && row.percent >= threshold).length;
  return `${rows.length} 条对话 · ${over} 条超过提醒阈值`;
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
  threshold,
  filter,
  query,
  sheet,
  previewRequest,
  onQuery,
}: {
  rows: HandoffRow[];
  threshold: number;
  filter: HandoffFilter;
  query: string;
  sheet: "" | "preview" | "confirm";
  previewRequest: number;
  onQuery: (query: string) => void;
}) {
  const [selectedId, setSelectedId] = useState(rows[0]?.id ?? "");
  const [mode, setMode] = useState<"local" | "ai">("ai");
  const [open, setOpen] = useState<"" | "preview" | "confirm">(sheet);
  const [notice, setNotice] = useState("");
  const [draft, setDraft] = useState("");
  const [deeplinkToken, setDeeplinkToken] = useState("");
  const visible = useMemo(() => {
    const matched = rows.filter((row) => row.title.includes(query) || row.tool.includes(query));
    if (filter === "attention") {
      return matched.filter((row) => row.percent != null && row.percent >= threshold);
    }
    if (filter === "recent") {
      return [...matched].reverse();
    }
    return matched;
  }, [filter, query, rows, threshold]);
  const selected = rows.find((row) => row.id === selectedId) ?? visible[0];
  const previewText = useMemo(() => sections.map(([name, text]) => `${name}\n${text}`).join("\n"), []);
  useEffect(() => {
    if (previewRequest > 0) setOpen("preview");
  }, [previewRequest]);
  useEffect(() => {
    if (open !== "preview" || !inTauri() || toolKind(selected?.tool ?? "") !== "cursor") return;
    const text = draft || previewText;
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
  }, [draft, open, previewText, selected?.tool]);

  async function generate() {
    if (!selected) return;
    if (!inTauri()) {
      setDraft(previewText);
      setOpen("preview");
      return;
    }
    try {
      const next = await prepareHandoff(toolKind(selected.tool), selected.id);
      if (next.note === handoffCopy.withheld) {
        setNotice(handoffCopy.withheld);
        return;
      }
      const organized = mode === "local" ? await organizeHandoff(next.text) : null;
      setDraft(organized ? organized.progress : next.text);
      setNotice(next.truncated ? handoffCopy.truncated : next.note);
      if (mode === "ai") setOpen("confirm");
      else setOpen("preview");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : handoffCopy.deleted);
    }
  }

  async function confirmSend() {
    if (!inTauri()) {
      setOpen("preview");
      return;
    }
    try {
      const preview = await beginSummary(draft || previewText);
      const text = await sendSummary(preview.token, preview.body);
      setDraft(text);
      setOpen("preview");
    } catch {
      setNotice(handoffCopy.summaryFailed);
      setOpen("");
    }
  }

  async function copyText() {
    await navigator.clipboard?.writeText(draft || previewText);
    setNotice("已复制，请手动粘贴");
  }

  async function place() {
    const text = draft || previewText;
    if (!inTauri() || !deeplinkToken) {
      await navigator.clipboard?.writeText(text);
      setNotice(toolKind(selected?.tool ?? "") === "cursor" ? "深链默认关闭，已改为复制" : "已复制，请手动粘贴");
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

  return (
    <div className="handoff">
      <section className="handoff-list" aria-label="对话列表">
        <label className="handoff-search">
          <IconSearch />
          <input
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
              <span className="handoff-mark">{markOf(row.tool)}</span>
              <span>
                <strong>{row.title}</strong>
                <small>
                  {toolLabel(row.tool)} · {row.updated}
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
            <span className="handoff-mark">{markOf(selected.tool)}</span>
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
                <b>{selected.percent == null ? selected.context || "未知" : `${selected.percent}%`}</b>
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
            <button type="button" className="handoff-choice recommended" onClick={() => setOpen("preview")}>
              <span>
                <strong>在 Cursor 新对话中继续</strong>
                <small>预填到新对话输入框，由你检查后发送</small>
              </span>
              <em>推荐</em>
            </button>
            <button type="button" className="handoff-choice" onClick={() => void copyText()}>
              <span>
                <strong>换到其他工具</strong>
                <small>Codex、DS Bot、Harness · 复制后手动粘贴</small>
              </span>
              <IconChevron />
            </button>
          </div>
          <div className="handoff-note">
            <strong>交接不会改动原对话</strong>
            <p>生成的内容先给你预览和编辑；放入后由你检查并发送。完成或取消后，万象不保留原文与摘要。</p>
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
              生成交接内容
            </button>
          </div>
        </section>
      ) : (
        <section className="handoff-detail">
          <p className="handoff-alert">{handoffCopy.missingServer}</p>
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
            <div className="sheet-wide">
              <span>目标</span>
              <p>{sections[0][1]}</p>
            </div>
            <div className="sheet-grid">
              {sections.slice(1).map(([name, text]) => (
                <label key={name}>
                  {name}
                  <p>{text}</p>
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
                放入 Cursor 新对话
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
              <input type="checkbox" defaultChecked /> 只对本次有效
            </label>
            <button type="button" className="primary-button" onClick={() => void confirmSend()}>
              发送并生成
            </button>
            <button
              type="button"
              className="quiet-button"
              onClick={() => {
                setMode("local");
                setOpen("preview");
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

function formatClock(date: Date) {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function toolKind(tool: string) {
  const name = tool.toLowerCase();
  if (name === "codex") return "codex";
  if (name === "ds bot" || name === "ds_bot") return "ds_bot";
  if (name.includes("harness")) return "deepseek_harness";
  return "cursor";
}

function toolLabel(tool: string) {
  const kind = toolKind(tool);
  if (kind === "codex") return "Codex";
  if (kind === "ds_bot") return "DS Bot";
  if (kind === "deepseek_harness") return "DeepSeek Harness";
  return "Cursor";
}

function markOf(tool: string) {
  const kind = toolKind(tool);
  if (kind === "codex") return "Cx";
  if (kind === "ds_bot") return "B";
  if (kind === "deepseek_harness") return "H";
  return "C";
}

export function fixtureHandoffRows() {
  return fixtureRows;
}
