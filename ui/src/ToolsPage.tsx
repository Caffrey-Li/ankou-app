import type { ConversationRow, Snapshot, UsageView } from "./api";
import { specOf, vendors, type VendorId } from "./catalog";
import { Ring, StatusDot, type SpaceTotals } from "./Console";
import { VendorAccess } from "./SettingsPage";
import { formatBytes } from "./SpacePage";
import { ToolIcon, type VendorMark } from "./tool-status";
import { groupDetails } from "./UsagePage";
import { cnCount, vendorCards, vendorHeadline, vendorRows } from "./vendor-data";
import "./console.css";
import "./tools.css";

/** 命令行、Bot 这类没有自己应用图标的端：用终端符号，不借别的图标。 */
const noAppIcon = new Set(["ds_bot", "cursor_cli"]);

export function EndIcon({ id, size = 14 }: { id: string; size?: number }) {
  if (noAppIcon.has(id)) {
    return (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
        <rect x="1.5" y="2.5" width="13" height="11" rx="2" />
        <path d="m4.5 6 2 2-2 2M8.5 10h3" />
      </svg>
    );
  }
  return <ToolIcon id={id} className="end-icon" />;
}

function Rail({ marks, usage, selected, onSelect }: { marks: VendorMark[]; usage: UsageView; selected: VendorId; onSelect: (id: VendorId) => void }) {
  return (
    <nav className="c-panel t-rail" aria-label="工具">
      {marks.map((mark) => {
        const vendor = vendors.find((item) => item.id === mark.id)!;
        const head = mark.presence === "authorized" ? vendorHeadline(usage, vendor) : null;
        const short = head ? (head.kind === "ring" ? `${head.percent}%` : head.value) : "";
        return (
          <button
            key={mark.id}
            type="button"
            className="t-rail-item"
            aria-current={mark.id === selected ? "page" : undefined}
            onClick={() => onSelect(mark.id)}
          >
            <ToolIcon id={mark.icon} className="c-icon" />
            <span className="t-rail-name">{mark.name}</span>
            <small>{short === "—" ? "" : short}</small>
            <StatusDot mark={mark} />
          </button>
        );
      })}
    </nav>
  );
}

export function ToolsPage({
  marks,
  selected,
  onSelect,
  usage,
  rows,
  threshold,
  snapshot,
  onChange,
  onReload,
  space,
  onHandoff,
  onClean,
}: {
  marks: VendorMark[];
  selected: VendorId;
  onSelect: (id: VendorId) => void;
  usage: UsageView;
  rows: ConversationRow[];
  threshold: number;
  snapshot: Snapshot;
  onChange: (update: (current: Snapshot) => Snapshot) => void;
  onReload: () => void;
  space: SpaceTotals | null;
  onHandoff: (row: ConversationRow | null) => void;
  onClean: () => void;
}) {
  const vendor = vendors.find((item) => item.id === selected) ?? vendors[0];
  const mark = marks.find((item) => item.id === vendor.id);
  const members = vendor.members.map((id) => specOf(id)).filter((spec) => spec !== undefined);
  const cards = vendorCards(usage, vendor.id);
  const rings = cards.filter((card) => card.percent != null);
  const balance = cards.find((card) => card.name === "DeepSeek 账户");
  const group = groupDetails(usage.details).find((item) => item.id === vendor.id);
  const talks = [...vendorRows(rows, vendor.id)].sort((a, b) => (b.percent ?? -1) - (a.percent ?? -1)).slice(0, 8);
  const vendorSpace = space?.find((item) => item.vendor === vendor.id) ?? null;
  const authorized = mark?.presence === "authorized";
  // 「需要处理」要说清楚是什么：额度快用完、对话快满
  const hotCount = vendorRows(rows, vendor.id).filter((row) => row.percent != null && row.percent >= threshold).length;
  const reasons = [
    ...rings.filter((card) => (card.percent ?? 0) >= 90).map((card) => `${card.name.split(" · ")[1] ?? "额度"}已用 ${card.percent}%`),
    ...(hotCount ? [`${hotCount} 个对话快满`] : []),
  ];
  const statusText = mark?.tone === "attention" && reasons.length ? reasons.join(" · ") : mark?.status;
  return (
    <div className="t-page">
      <Rail marks={marks} usage={usage} selected={vendor.id} onSelect={onSelect} />
      <div className="t-detail">
        <header className="t-head">
          <ToolIcon id={vendor.icon} className="c-icon t-head-icon" />
          <div className="t-head-copy">
            <h1>{vendor.name}</h1>
            <div className="t-ends">
              {members.map((spec) => (
                <span key={spec.id} className="t-end" title={spec.ends}>
                  <EndIcon id={spec.id} />
                  {members.length > 1 ? spec.name : spec.ends ?? spec.name}
                </span>
              ))}
            </div>
          </div>
          {mark ? (
            <span className={`t-status ${mark.tone}`}>
              <StatusDot mark={mark} />
              {statusText}
            </span>
          ) : null}
        </header>

        {/* 概况一条：额度、余额、token、空间并排，有几项排几项，宽度总是铺满 */}
        <section className="c-panel t-summary" aria-label="概况">
          {rings.slice(0, 2).map((card) => (
            <div key={card.name} className="t-cell ring">
              <Ring percent={card.percent ?? 0} size={52} />
              <div>
                <h2 className="c-h">{card.name.split(" · ")[1] ?? card.name}</h2>
                <p>{card.reset ?? card.note}</p>
                {card.reset ? <small>{card.note}</small> : null}
              </div>
            </div>
          ))}
          {balance ? (
            <div className="t-cell">
              <h2 className="c-h">余额</h2>
              <strong className="t-value">{balance.value === "—" ? balance.note : balance.value}</strong>
              <small>Harness 与 DS Bot 共用</small>
            </div>
          ) : null}
          {group ? (
            <div className="t-cell">
              <h2 className="c-h">本月 token</h2>
              <strong className={/^[\d,]+$/.test(group.row.tokens) ? "t-value" : "t-value muted"}>
                {/^[\d,]+$/.test(group.row.tokens) ? cnCount(group.row.tokens) : group.row.tokens}
              </strong>
              <small>
                {/^[\d,]+$/.test(group.row.calls) ? `${group.row.calls} 次调用` : group.row.source && group.row.source !== "—" ? group.row.source : "本机没有可读的 token 记录"}
                {group.ends.length > 1 ? ` · ${group.ends.length} 端合计` : ""}
              </small>
            </div>
          ) : null}
          {rings.length === 0 && !balance && !group ? (
            <div className="t-cell">
              <h2 className="c-h">额度与用量</h2>
              <strong className="t-value muted">暂不提供</strong>
              <small>本机没有可读的额度或 token 记录</small>
            </div>
          ) : null}
          <div className="t-cell">
            <div className="t-cell-head">
              <h2 className="c-h">本机空间</h2>
              <button type="button" className="text-button" onClick={onClean}>
                去清理
              </button>
            </div>
            <strong className={vendorSpace ? "t-value" : "t-value muted"}>{vendorSpace ? formatBytes(vendorSpace.bytes, 1) : "尚未统计"}</strong>
            <small>
              {vendorSpace
                ? vendorSpace.cleanable > 0
                  ? `可安全清理 ${formatBytes(vendorSpace.cleanable, 1)}`
                  : "全部是对话与设置，受保护"
                : "授权空间扫描后，在「清理」里统计"}
            </small>
          </div>
        </section>

        <div className="t-cols">
          <section className="c-panel t-talks" aria-label="对话">
            <div className="c-panel-head">
              <h2 className="c-h">对话{talks.length ? ` · ${vendorRows(rows, vendor.id).length}` : ""}</h2>
              <button type="button" className="text-button" onClick={() => onHandoff(null)}>
                在交接里查看全部
              </button>
            </div>
            {talks.length === 0 ? (
              <p className="c-empty">{authorized ? "本机没有这家的对话记录，或还没授权读取对话" : "还没有授权读取对话"}</p>
            ) : (
              talks.map((row) => {
                const hot = row.percent != null && row.percent >= threshold;
                return (
                  <div key={`${row.tool}:${row.id}`} className="c-item t-talk">
                    <span className="c-item-copy">
                      <strong>{row.title}</strong>
                      <span>
                        <EndIcon id={row.tool} size={12} />
                        {specOf(row.tool)?.name ?? row.tool}
                      </span>
                    </span>
                    {row.percent != null ? <b className={hot ? "t-pct hot" : "t-pct"}>{row.percent}%</b> : <small className="t-unknown">未知</small>}
                    <button type="button" className={hot ? "c-btn" : "c-btn quiet"} onClick={() => onHandoff(row)}>
                      交接
                    </button>
                  </div>
                );
              })
            )}
          </section>
          <section className="c-panel t-access" aria-label="授权">
            <div className="c-panel-head">
              <h2 className="c-h">授权 · 只读</h2>
            </div>
            <VendorAccess vendorId={vendor.id} snapshot={snapshot} onChange={onChange} onReload={onReload} />
          </section>
        </div>
      </div>
    </div>
  );
}
