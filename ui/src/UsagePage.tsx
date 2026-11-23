import { useState } from "react";
import type { UsageDay, UsageView } from "./api";
import { DateRangeButton, presetSpan } from "./DateRange";
import { IconAlert, IconChart } from "./icons";
import "./usage.css";

const ranges = [
  { id: "today", label: "今天" },
  { id: "7", label: "7 天" },
  { id: "30", label: "30 天" },
  { id: "month", label: "本月" },
  { id: "custom", label: "自定义" },
] as const;

export type UsageRange = (typeof ranges)[number]["id"];

const tabs = ["工具", "Bot", "群聊", "模型", "会话"] as const;

export function RangeControl({
  range,
  start,
  end,
  today,
  onRange,
  onCustom,
}: {
  range: UsageRange;
  start: string;
  end: string;
  today: string;
  onRange: (range: UsageRange) => void;
  onCustom: (start: string, end: string) => void;
}) {
  function choose(next: UsageRange) {
    if (next === "custom" && range !== "custom") {
      // 默认值取当前正在看的范围，避免两端都变成今天、和副标题对不上。
      const span = presetSpan(range, today);
      onCustom(span.start, span.end);
    }
    onRange(next);
  }
  return (
    <div className="range-control">
      {range === "custom" && start && end ? (
        <DateRangeButton start={start} end={end} today={today} onApply={onCustom} />
      ) : null}
      <div className="range-pills" role="group" aria-label="日期范围">
        {ranges.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={range === item.id}
            onClick={() => choose(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function placeholderDays(date = new Date()): UsageDay[] {
  const month = date.getMonth();
  const last = date.getDate();
  const days: UsageDay[] = [];
  for (let day = 1; day <= last; day += 1) {
    days.push({ label: `${month + 1} 月 ${day} 日`, codex: null, bot: null, harness: null, claude: null, antigravity: null });
  }
  return days;
}

function tick(label: string) {
  return label.replace(" 月 ", "/").replace(" 日", "");
}

function AuthLink({ source, onSettings }: { source: string; onSettings: () => void }) {
  if (source !== "未授权" && source !== "需要重新授权") return null;
  return (
    <button type="button" className="text-link" onClick={onSettings}>
      去设置授权
    </button>
  );
}

export type RefreshPending = {
  ds_bot: boolean;
  balance: boolean;
  codex: boolean;
  windows: boolean;
  claude?: boolean;
  antigravity?: boolean;
  grok?: boolean;
};

const idlePending: RefreshPending = {
  ds_bot: false,
  balance: false,
  codex: false,
  windows: false,
  claude: false,
  antigravity: false,
  grok: false,
};

export function totalLabel(range: UsageRange) {
  if (range === "today") return "今天可核实用量";
  if (range === "7") return "7 天可核实用量";
  if (range === "30") return "30 天可核实用量";
  if (range === "custom") return "所选范围可核实用量";
  return "本月可核实用量";
}

export function UsagePage({
  usage,
  onSettings,
  pending = idlePending,
  range = "month",
}: {
  usage: UsageView;
  onSettings: () => void;
  pending?: RefreshPending;
  range?: UsageRange;
}) {
  const [tab, setTab] = useState<(typeof tabs)[number]>("工具");
  const [moreOpen, setMoreOpen] = useState(false);
  const bannerLines = orderBanner(usage.banner);
  const days = usage.days.length > 0 ? usage.days : placeholderDays();
  const max = Math.max(0, ...days.map((day) => (day.codex ?? 0) + (day.bot ?? 0) + (day.harness ?? 0) + (day.claude ?? 0) + (day.antigravity ?? 0)));
  const scale = Math.max(max, 1);
  return (
    <div className="usage-page">
      <p className="usage-banner" role="note">
        <IconAlert />
        <span>
          合计不完整：{(moreOpen ? bannerLines : bannerLines.slice(0, 2)).join("；")}。
          {bannerLines.length > 2 ? (
            <button type="button" className="banner-more" onClick={() => setMoreOpen(!moreOpen)}>
              {moreOpen ? "收起" : `另有 ${bannerLines.length - 2} 条说明`}
            </button>
          ) : null}
        </span>
        {usage.banner.some((line) => line.includes("未授权") || line.includes("需要重新授权")) ? (
          <button type="button" className="text-link" onClick={onSettings}>
            去设置授权
          </button>
        ) : null}
      </p>
      <div className="usage-head">
        <h2>额度与余额</h2>
        <p>{usage.caption}</p>
      </div>
      <div className={usage.cards.length > 4 ? "quota-grid three" : "quota-grid"}>
        {usage.cards.map((card, index) => (
          <article key={`${card.name}-${index}`} className="quota-tile">
            <h3>{cardTitle(card.name)}</h3>
            {(pending.windows && card.name.startsWith("ChatGPT")) || (pending.claude && card.name.startsWith("Claude")) ||
            (pending.grok && card.name.startsWith("Grok Bot")) ? (
              <p className="quota-plain">刷新中</p>
            ) : pending.balance && card.name === "DeepSeek 账户" ? (
              <p className="quota-plain">刷新中</p>
            ) : card.name === "Cursor" ? (
              <div className="quota-copy">
                <p className="quota-plain">暂不提供</p>
                <p className="share">个人账户来源待验证</p>
                <button type="button" className="text-link" onClick={onSettings}>
                  了解原因
                </button>
              </div>
            ) : card.name === "DeepSeek 账户" ? (
              <BalanceCopy card={card} onSettings={onSettings} />
            ) : (
              <div className="quota-row">
                <Ring percent={card.percent} />
                <div className="quota-copy">
                  {card.percent != null ? (
                    <>
                      {card.reset ? <p className="reset-line">{card.reset}</p> : null}
                      <p className="share">{card.note}</p>
                    </>
                  ) : (
                    <>
                      <p className="quota-plain">{card.note}</p>
                      {card.link ? (
                        <button type="button" className="text-link" onClick={onSettings}>
                          {card.link}
                        </button>
                      ) : null}
                    </>
                  )}
                </div>
              </div>
            )}
          </article>
        ))}
      </div>
      <section className="usage-panel chart-panel">
        <div className="chart-head">
          <div className="total-block">
            <p>{totalLabel(range)}</p>
            <div className="token-figure">
              <strong>{pending.codex || pending.ds_bot || pending.claude || pending.antigravity ? "刷新中" : usage.total}</strong>
              {usage.total === "—" || pending.codex || pending.ds_bot || pending.claude || pending.antigravity ? null : (
                <span>token</span>
              )}
            </div>
            {usage.total === "—" ? (
              <p className="total-reason">
                {usage.total_reason}
                <AuthLink source={usage.total_reason} onSettings={onSettings} />
              </p>
            ) : null}
          </div>
          <div className="fee-block">
            <p>估算费用</p>
            <p className="fee-value">不提供</p>
          </div>
          <div className="chart-legend">
            <span><i className="swatch harness" />Harness</span>
            <span><i className="swatch bot" />DS Bot</span>
            <span><i className="swatch codex" />ChatGPT</span>
            <span><i className="swatch claude" />Claude</span>
            <span><i className="swatch antigravity" />Antigravity</span>
          </div>
        </div>
        <div className="usage-chart" aria-label="按日本机记录">
          {days.map((day) => {
            const codex = day.codex ?? 0;
            const bot = day.bot ?? 0;
            const harness = day.harness ?? 0;
            const claude = day.claude ?? 0;
            const antigravity = day.antigravity ?? 0;
            const missing =
              day.codex == null && day.bot == null && day.harness == null && day.claude == null && day.antigravity == null;
            const height = Math.max(8, Math.round(((codex + bot + harness + claude + antigravity) / scale) * 150));
            return (
              <div key={day.label} className="chart-col">
                <div className="chart-slot">
                  {missing ? (
                    <span className="chart-gap" />
                  ) : (
                    <span className="chart-stack" style={{ height }}>
                      {codex > 0 ? <span className="bar codex" style={{ flexGrow: codex }} /> : null}
                      {bot > 0 ? <span className="bar bot" style={{ flexGrow: bot }} /> : null}
                      {harness > 0 ? <span className="bar harness" style={{ flexGrow: harness }} /> : null}
                      {claude > 0 ? <span className="bar claude" style={{ flexGrow: claude }} /> : null}
                      {antigravity > 0 ? <span className="bar antigravity" style={{ flexGrow: antigravity }} /> : null}
                    </span>
                  )}
                </div>
                <span>{tick(day.label)}</span>
              </div>
            );
          })}
        </div>
        <p className="chart-note">{usage.empty_note}</p>
      </section>
      <section className="usage-panel">
        <div className="detail-head">
          <h2>明细</h2>
          <div className="range-pills" role="tablist" aria-label="明细分组">
            {tabs.map((item) => (
              <button
                key={item}
                type="button"
                role="tab"
                aria-selected={tab === item}
                onClick={() => setTab(item)}
              >
                {item}
              </button>
            ))}
          </div>
        </div>
        {tab === "工具" ? (
          <div className="detail-table">
            <div className="detail-row detail-head-row">
              <span>名称</span>
              <span>类型</span>
              <span className="num">token</span>
              <span className="num">调用</span>
              <span className="num">费用</span>
              <span className="source">来源</span>
            </div>
            {usage.details.map((row) => {
              const waiting =
                (row.name === "DS Bot" && pending.ds_bot) ||
                (row.name === "ChatGPT" && pending.codex) ||
                (row.name === "Claude" && Boolean(pending.claude)) ||
                (row.name === "Antigravity" && Boolean(pending.antigravity));
              return (
              <div key={row.name} className="detail-row">
                <span className="detail-name">{row.name}</span>
                <span>工具</span>
                <span className="num">{waiting ? "刷新中" : row.tokens}</span>
                <span className="num">{waiting ? "刷新中" : row.calls}</span>
                <span className="num">{row.fee}</span>
                <span className="source">
                  {row.source || "—"}
                  <AuthLink source={row.source} onSettings={onSettings} />
                </span>
              </div>
              );
            })}
          </div>
        ) : (
          <p className="chart-empty">暂不提供</p>
        )}
        <p className="detail-foot">Harness 只计本机会话，Bot 工作区的会话归入 DS Bot，不重复计入。不同单位与币种分别合计。</p>
      </section>
    </div>
  );
}

export function QuotaCard({
  usage,
  onOpen,
  pending = idlePending,
}: {
  usage: UsageView;
  onOpen: () => void;
  pending?: RefreshPending;
}) {
  const codex = usage.cards.find((card) => card.percent != null) ?? usage.cards[0];
  const balance = usage.cards.find((card) => card.name === "DeepSeek 账户");
  return (
    <section className="overview-quota">
      <button type="button" className="quota-title" onClick={onOpen}>
        <IconChart />
        <span>额度</span>
        <span className="quota-chev" aria-hidden="true">
          ›
        </span>
      </button>
      <div className="quota-body">
        {pending.windows ? (
          <p className="quota-plain">刷新中</p>
        ) : (
          <>
            <Ring percent={codex?.percent ?? null} />
            <div>
              <strong>{codex?.name ?? "ChatGPT"}</strong>
              {codex?.reset ? <p className="reset-line">{codex.reset}</p> : null}
              <p className="quota-status">{codex?.note}</p>
            </div>
          </>
        )}
      </div>
      <div className="quota-line">
        <span>DeepSeek 余额</span>
        <strong>
          {pending.balance
            ? "刷新中"
            : balance && balance.value !== "—"
              ? balance.value
              : balance?.note ?? "未保存密钥"}
        </strong>
      </div>
      <div className="quota-line">
        <span>Cursor 额度</span>
        <span className="muted">暂不提供</span>
      </div>
    </section>
  );
}

function cardTitle(name: string) {
  if (name === "DeepSeek 账户") return "DeepSeek 余额";
  if (name === "Cursor") return "Cursor 额度";
  return name;
}

function chipClass(note: string) {
  return note.includes("过时") ? "source-chip" : "source-chip fresh";
}

function BalanceCopy({
  card,
  onSettings,
}: {
  card: UsageView["cards"][number];
  onSettings: () => void;
}) {
  const amount = card.value.startsWith("¥") ? card.value.slice(1) : card.value;
  return (
    <div className="quota-copy">
      {card.value === "—" ? (
        <p className="quota-plain">{card.note}</p>
      ) : (
        <p className="money">
          <span>¥</span>
          <strong>{amount}</strong>
        </p>
      )}
      <p className="share">Harness 与 DS Bot 共用</p>
      {card.value === "—" ? null : <span className={chipClass(card.note)}>{card.note}</span>}
      {card.link ? (
        <button type="button" className="text-link" onClick={onSettings}>
          {card.link}
        </button>
      ) : null}
    </div>
  );
}

function Ring({ percent }: { percent: number | null }) {
  const turn = percent == null ? 0 : Math.max(0, Math.min(100, percent)) / 100;
  return (
    <div className="ring" aria-label={percent == null ? "—" : `${percent}%`}>
      <svg viewBox="0 0 56 56" width="56" height="56">
        <circle cx="28" cy="28" r="22" />
        {percent == null ? null : (
          <circle cx="28" cy="28" r="22" pathLength={100} strokeDasharray={`${turn * 100} 100`} />
        )}
      </svg>
      <strong>{percent == null ? "—" : `${percent}%`}</strong>
    </div>
  );
}

/** 先说需要用户处理的（未授权、需要重新授权、过时），其余说明折叠。 */
export function orderBanner(lines: string[]) {
  const rank = (line: string) =>
    line.includes("需要重新授权") ? 0 : line.includes("未授权") ? 1 : line.includes("过时") || line.includes("未运行") ? 2 : 3;
  return lines
    .map((line, index) => ({ line, index }))
    .sort((a, b) => rank(a.line) - rank(b.line) || a.index - b.index)
    .map((item) => item.line);
}
