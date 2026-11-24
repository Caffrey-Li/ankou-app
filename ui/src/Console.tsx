import { useState } from "react";
import type { ConversationRow, UsageView } from "./api";
import { vendors, type VendorId } from "./catalog";
import { formatBytes } from "./SpacePage";
import { ToolIcon, type VendorMark } from "./tool-status";
import { chartColor, cnCount, stackDays, vendorHeadline, vendorName } from "./vendor-data";
import "./console.css";

export type PendingItem = {
  title: string;
  detail: string;
  action: string;
  quiet: boolean;
  to: "settings" | "handoff" | "space" | "tools";
};

export type SpaceTotals = { vendor: VendorId; bytes: number; cleanable: number }[];

export function Ring({ percent, size = 56 }: { percent: number; size?: number }) {
  const value = Math.max(0, Math.min(100, percent));
  return (
    <div className="c-ring" style={{ width: size, height: size }}>
      <svg viewBox="0 0 56 56" width={size} height={size} aria-hidden="true">
        <circle cx="28" cy="28" r="22" className="c-ring-track" />
        <circle cx="28" cy="28" r="22" className="c-ring-value" pathLength={100} strokeDasharray={`${value} 100`} />
      </svg>
      <strong style={{ fontSize: Math.max(10, Math.round(size * 0.22)) }}>{value}%</strong>
    </div>
  );
}

export function StatusDot({ mark }: { mark: VendorMark }) {
  return <i className={`c-dot ${mark.tone}`} role="img" aria-label={mark.status} />;
}

function VendorTile({
  mark,
  usage,
  onOpen,
}: {
  mark: VendorMark;
  usage: UsageView;
  onOpen: () => void;
}) {
  const vendor = vendors.find((item) => item.id === mark.id)!;
  const head = mark.presence === "authorized" ? vendorHeadline(usage, vendor) : null;
  return (
    <button type="button" className="c-tile" onClick={onOpen}>
      <span className="c-tile-head">
        <ToolIcon id={mark.icon} className="c-icon" />
        <strong>{mark.name}</strong>
        <StatusDot mark={mark} />
      </span>
      {head?.kind === "ring" ? (
        <Ring percent={head.percent} />
      ) : (
        <span className="c-tile-value">{head ? head.value : mark.status}</span>
      )}
      <span className="c-tile-foot">
        <span>{head ? head.label : mark.subtitle}</span>
        {head?.sub ? <small>{head.sub}</small> : null}
      </span>
    </button>
  );
}

/** 坐标轴刻度：取 1 / 2 / 2.5 / 5 × 10ⁿ 的整齐步长，大约 4 格 */
export function niceTicks(max: number, count = 4) {
  if (max <= 0) return [0];
  const raw = max / count;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * power).find((value) => value >= raw) ?? raw;
  const ticks: number[] = [];
  for (let value = 0; value < max + step * 0.999; value += step) ticks.push(value);
  return ticks;
}

/** 轴上的短写法：3.4 亿、2500 万、800 */
export function shortCount(value: number) {
  const trim = (n: number) => String(Number(n.toFixed(1)));
  if (value >= 1e8) return `${trim(value / 1e8)} 亿`;
  if (value >= 1e4) return `${trim(value / 1e4)} 万`;
  return String(Math.round(value));
}

/** 「11 月 23 日」→ 横轴上只写「23 日」，第一格带上月份 */
function dayTick(label: string, first: boolean) {
  if (first) return label;
  const match = /(\d+)\s*日/.exec(label);
  return match ? `${match[1]} 日` : label;
}

// 自下而上的堆叠顺序与配色顺序一致（已用配色校验：相邻两色在色弱下也分得开）
const stackOrder: VendorId[] = ["anthropic", "openai", "google", "deepseek"];

function UsageChart({ usage, onOpen }: { usage: UsageView; onOpen: () => void }) {
  const days = stackDays(usage);
  const [active, setActive] = useState<number | null>(null);
  const max = Math.max(0, ...days.map((day) => day.total));
  const ticks = niceTicks(max);
  const top = ticks[ticks.length - 1] || 1;
  const present = stackOrder.filter((vendor) =>
    days.some((day) => day.parts.some((part) => part.vendor === vendor && part.value > 0)),
  );
  const every = days.length > 16 ? 5 : days.length > 8 ? 2 : 1;
  const shown = active == null ? null : days[active];
  return (
    <section className="c-panel c-chart" aria-label="本月用量">
      <div className="c-chart-head">
        <h2 className="c-h">本月 token</h2>
        <strong className="c-big">{usage.total && usage.total !== "—" ? cnCount(usage.total) : "—"}</strong>
        <span className="c-legend">
          {present.map((vendor) => (
            <span key={vendor}>
              <i style={{ background: chartColor[vendor] }} />
              {vendorName(vendor)}
            </span>
          ))}
        </span>
        <button type="button" className="text-button" onClick={onOpen}>
          明细
        </button>
      </div>
      {days.length === 0 || max <= 0 ? (
        <p className="c-empty">{usage.total_reason || "还没有本机用量记录"}</p>
      ) : (
        <div className="c-plot" onMouseLeave={() => setActive(null)}>
          <div className="c-yaxis" aria-hidden="true">
            {ticks.map((tick) => (
              <span key={tick} style={{ bottom: `${(tick / top) * 100}%` }}>
                {shortCount(tick)}
              </span>
            ))}
          </div>
          <div className="c-area">
            {ticks.map((tick) => (
              <i key={tick} className="c-grid" style={{ bottom: `${(tick / top) * 100}%` }} aria-hidden="true" />
            ))}
            <div className="c-cols">
              {days.map((day, index) => {
                const parts = stackOrder
                  .map((vendor) => day.parts.find((part) => part.vendor === vendor))
                  .filter((part): part is NonNullable<typeof part> => Boolean(part && part.value > 0));
                const summary = `${day.label}：共 ${shortCount(day.total)}${parts.map((part) => `，${vendorName(part.vendor)} ${shortCount(part.value)}`).join("")}`;
                return (
                  <div
                    key={day.label}
                    className={active === index ? "c-col active" : "c-col"}
                    tabIndex={0}
                    aria-label={day.total > 0 ? summary : `${day.label}：无记录`}
                    onMouseEnter={() => setActive(index)}
                    onFocus={() => setActive(index)}
                    onBlur={() => setActive(null)}
                  >
                    <div className="c-stack-bar" style={{ height: `${(day.total / top) * 100}%` }}>
                      {parts
                        .slice()
                        .reverse()
                        .map((part) => (
                          <i key={part.vendor} style={{ flexGrow: part.value, background: chartColor[part.vendor] }} />
                        ))}
                    </div>
                  </div>
                );
              })}
            </div>
            {shown ? (
              <div
                className={active != null && active > days.length * 0.6 ? "c-tip left" : "c-tip"}
                style={{ left: `${(((active ?? 0) + 0.5) / days.length) * 100}%` }}
                role="status"
              >
                <strong>{shown.label}</strong>
                <span className="c-tip-total">{shown.total > 0 ? `共 ${shortCount(shown.total)}` : "无记录"}</span>
                {stackOrder
                  .slice()
                  .reverse()
                  .map((vendor) => shown.parts.find((part) => part.vendor === vendor))
                  .filter((part): part is NonNullable<typeof part> => Boolean(part && part.value > 0))
                  .map((part) => (
                    <span key={part.vendor} className="c-tip-row">
                      <i style={{ background: chartColor[part.vendor] }} />
                      {vendorName(part.vendor)}
                      <b>{shortCount(part.value)}</b>
                    </span>
                  ))}
              </div>
            ) : null}
          </div>
          <div className="c-xaxis" aria-hidden="true">
            {days.map((day, index) => (
              <span key={day.label}>
                {index === 0 || index === days.length - 1 || index % every === 0 ? dayTick(day.label, index === 0) : ""}
              </span>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

export function Console({
  marks,
  usage,
  items,
  hottest,
  space,
  pendingLoading = false,
  spaceLoading = false,
  thresholdGb,
  onVendor,
  onGo,
  onHandoff,
  onUsage,
}: {
  marks: VendorMark[];
  usage: UsageView;
  items: PendingItem[];
  /** 上下文占用最高、超过阈值的对话 */
  hottest: ConversationRow | null;
  space: SpaceTotals | null;
  /** 对话列表还在读取：先不说"一切正常" */
  pendingLoading?: boolean;
  /** 启动后的空间统计还在进行 */
  spaceLoading?: boolean;
  thresholdGb: number;
  onVendor: (id: VendorId) => void;
  onGo: (to: PendingItem["to"]) => void;
  onHandoff: (row: ConversationRow | null) => void;
  onUsage: () => void;
}) {
  const totalSpace = space?.reduce((sum, item) => sum + item.bytes, 0) ?? 0;
  const cleanable = space?.reduce((sum, item) => sum + item.cleanable, 0) ?? 0;
  const ranked = [...(space ?? [])].filter((item) => item.bytes > 0).sort((a, b) => b.bytes - a.bytes);
  const spaceColors = ["var(--chart-1)", "var(--chart-2)", "var(--chart-3)", "var(--chart-4)", "var(--fill-3)"];
  return (
    <div className="c-overview">
      <div className="c-main">
        <div className="c-tiles">
          {marks.map((mark) => (
            <VendorTile key={mark.id} mark={mark} usage={usage} onOpen={() => onVendor(mark.id)} />
          ))}
        </div>
        <UsageChart usage={usage} onOpen={onUsage} />
      </div>
      <aside className="c-side">
        <section className="c-panel" aria-label="待处理">
          <div className="c-panel-head">
            <h2 className="c-h">待处理</h2>
            {items.length + (hottest ? 1 : 0) > 0 ? (
              <span className="c-count">{items.length + (hottest ? 1 : 0)}</span>
            ) : null}
          </div>
          {hottest ? (
            <div className="c-item">
              <i className="c-dot attention" aria-hidden="true" />
              <span className="c-item-copy">
                <strong>对话上下文 {hottest.percent}%</strong>
                <span>{hottest.title}</span>
              </span>
              <button type="button" className="c-btn" onClick={() => onHandoff(hottest)}>
                交接
              </button>
            </div>
          ) : null}
          {items.map((item) => (
            <div key={item.title} className="c-item">
              <i className={item.quiet ? "c-dot quiet" : "c-dot attention"} aria-hidden="true" />
              <span className="c-item-copy">
                <strong>{item.title}</strong>
                <span>{item.detail}</span>
              </span>
              <button type="button" className="c-btn quiet" onClick={() => onGo(item.to)}>
                {item.action}
              </button>
            </div>
          ))}
          {items.length === 0 && !hottest ? (
            <p className="c-empty">{pendingLoading ? "正在读取对话…" : "一切正常，没有需要你处理的事"}</p>
          ) : null}
        </section>
        <section className="c-panel c-space" aria-label="本机空间">
          <div className="c-panel-head">
            <h2 className="c-h">本机空间{space ? ` · ${formatBytes(totalSpace, 1)}` : ""}</h2>
            <button type="button" className="text-button" onClick={() => onGo("space")}>
              清理
            </button>
          </div>
          {space && ranked.length > 0 ? (
            <>
              <div className="c-stack" aria-hidden="true">
                {ranked.map((item, index) => (
                  <i key={item.vendor} style={{ flex: Math.max(item.bytes, totalSpace * 0.01), background: spaceColors[Math.min(index, 4)] }} />
                ))}
              </div>
              <div className="c-space-list">
                {ranked.slice(0, 4).map((item) => (
                  <span key={item.vendor} className={item.bytes > thresholdGb * 1e9 ? "over" : undefined}>
                    {vendorName(item.vendor)} <small>{formatBytes(item.bytes, 1)}</small>
                  </span>
                ))}
              </div>
              <p className="c-note">可安全清理 {formatBytes(cleanable, 1)}</p>
            </>
          ) : (
            <p className="c-empty">{spaceLoading ? "正在统计各工具占用…" : "尚未扫描，打开「清理」后会统计各工具占用"}</p>
          )}
        </section>
      </aside>
    </div>
  );
}
