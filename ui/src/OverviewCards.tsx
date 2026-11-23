import type { ConversationRow } from "./api";
import { IconBubble, IconDisk } from "./icons";
import { formatBytes, splitSize } from "./SpacePage";

/** 空间页最近一次扫描 Harness 的结果，供概览卡片与待办使用。 */
export type SpaceSummary = {
  tool: string;
  known: number;
  protectedBytes: number;
  cleanable: number;
};

const toolLabels: Record<string, string> = {
  codex: "ChatGPT",
  ds_bot: "DS Bot",
  cursor: "Cursor",
  deepseek_harness: "Harness",
  claude: "Claude",
  antigravity: "Antigravity",
  workbuddy: "WorkBuddy",
  grok_bot: "Grok Bot",
  codebuddy: "CodeBuddy",
  cursor_cli: "Cursor 命令行",
};

/** 概览 · 对话卡 18:214：上下文占用最高的一条对话。 */
export function OverviewTalkCard({
  rows,
  threshold,
  onOpen,
}: {
  rows: ConversationRow[];
  threshold: number;
  onOpen: () => void;
}) {
  const known = rows.filter((row) => row.percent != null);
  const top = known.reduce<ConversationRow | null>(
    (best, row) => (best === null || (row.percent ?? 0) > (best.percent ?? 0) ? row : best),
    null,
  );
  const others = known.filter((row) => row !== top && (row.percent ?? 0) >= 60).length;
  return (
    <button type="button" className="overview-card overview-metric" onClick={onOpen}>
      <span className="quota-title">
        <IconBubble />
        <span>对话</span>
        <span className="quota-chev" aria-hidden="true">›</span>
      </span>
      {top && top.percent != null ? (
        <>
          <span className="overview-figure">
            <strong className={top.percent >= threshold ? "warn" : ""}>{top.percent}</strong>
            <small className={top.percent >= threshold ? "warn" : ""}>%</small>
          </span>
          <span className="overview-meta">
            <b>
              {top.title} · {toolLabels[top.tool] ?? top.tool}
            </b>
            <i className="overview-bar" aria-hidden="true">
              <span
                className={top.percent >= threshold ? "fill warn" : "fill"}
                style={{ width: `${Math.min(100, top.percent)}%` }}
              />
            </i>
          </span>
          <span className="overview-foot">
            <span>提醒阈值 {threshold}%</span>
            <span>{others > 0 ? `另有 ${others} 条超过 60%` : "其余对话低于 60%"}</span>
          </span>
        </>
      ) : (
        <span className="overview-pending">{rows.length > 0 ? "对话的上下文比例未知" : "还没有可显示的对话"}</span>
      )}
    </button>
  );
}

/** 概览 · 空间卡 18:240：数字 40 半粗、单位 18、阈值刻度、受保护与可清理图例。 */
export function OverviewSpaceCard({
  summary,
  thresholdGb,
  onOpen,
}: {
  summary: SpaceSummary | null;
  thresholdGb: number;
  onOpen: () => void;
}) {
  const threshold = thresholdGb * 1_000_000_000;
  const scale = summary ? Math.max(summary.known, threshold) * 1.25 : 1;
  const size = summary ? splitSize(summary.known) : null;
  const over = summary ? summary.known > threshold : false;
  return (
    <button type="button" className="overview-card overview-metric" onClick={onOpen}>
      <span className="quota-title">
        <IconDisk />
        <span>空间</span>
        <span className="quota-chev" aria-hidden="true">›</span>
      </span>
      {summary && size ? (
        <>
          <span className="overview-figure">
            <strong>{size.value}</strong>
            <small>{size.unit}</small>
          </span>
          <span className="overview-meta">
            <b>
              {summary.tool} · 阈值 {thresholdGb} GB
            </b>
            <i className="overview-bar" aria-hidden="true">
              <span className="fill protected" style={{ width: `${(summary.protectedBytes / scale) * 100}%` }} />
              {summary.cleanable > 0 ? (
                <span
                  className="fill clean"
                  style={{
                    left: `${(summary.protectedBytes / scale) * 100}%`,
                    width: `${(summary.cleanable / scale) * 100}%`,
                  }}
                />
              ) : null}
              <span className={over ? "mark over" : "mark"} style={{ left: `${(threshold / scale) * 100}%` }} />
            </i>
          </span>
          <span className="overview-foot legend">
            <span>
              <i className="protected" />
              受保护 {formatBytes(summary.protectedBytes, 1)}
            </span>
            <span>
              <i className="clean" />
              可清理 {formatBytes(summary.cleanable, 1)}
            </span>
          </span>
        </>
      ) : (
        <span className="overview-pending">尚未扫描，打开空间管理后点刷新</span>
      )}
    </button>
  );
}
