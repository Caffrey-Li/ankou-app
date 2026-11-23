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
      <strong>{value}%</strong>
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

function UsageChart({ usage, onOpen }: { usage: UsageView; onOpen: () => void }) {
  const days = stackDays(usage);
  const max = Math.max(1, ...days.map((day) => day.total));
  const present = (["anthropic", "openai", "deepseek", "google"] as VendorId[]).filter((vendor) =>
    days.some((day) => day.parts.some((part) => part.vendor === vendor && part.value > 0)),
  );
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
      {days.length === 0 || max <= 1 ? (
        <p className="c-empty">{usage.total_reason || "还没有本机用量记录"}</p>
      ) : (
        <div className="c-bars" aria-hidden="true">
          {days.map((day) => (
            <div key={day.label} className="c-bar" title={`${day.label} · ${cnCount(day.total)}`}>
              {day.parts
                .filter((part) => part.value > 0)
                .reverse()
                .map((part) => (
                  <i
                    key={part.vendor}
                    style={{ height: `${(part.value / max) * 100}%`, background: chartColor[part.vendor] }}
                  />
                ))}
            </div>
          ))}
        </div>
      )}
      {days.length > 0 ? (
        <div className="c-axis">
          <span>{days[0].label}</span>
          <span>{days[days.length - 1].label}</span>
        </div>
      ) : null}
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
