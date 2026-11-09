import { useState } from "react";
import type { UsageDay, UsageView } from "./api";
import { IconChart } from "./icons";
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
  onRange,
  onCustom,
}: {
  range: UsageRange;
  start: string;
  end: string;
  onRange: (range: UsageRange) => void;
  onCustom: (start: string, end: string) => void;
}) {
  return (
    <div className="range-control">
      <div className="range-pills" role="group" aria-label="日期范围">
        {ranges.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={range === item.id}
            onClick={() => onRange(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>
      {range === "custom" ? (
        <div className="range-custom">
          <input
            type="date"
            aria-label="开始日期"
            value={start}
            onChange={(event) => onCustom(event.target.value, end)}
          />
          <input
            type="date"
            aria-label="结束日期"
            value={end}
            onChange={(event) => onCustom(start, event.target.value)}
          />
        </div>
      ) : null}
    </div>
  );
}

function placeholderDays(date = new Date()): UsageDay[] {
  const month = date.getMonth();
  const last = date.getDate();
  const days: UsageDay[] = [];
  for (let day = 1; day <= last; day += 1) {
    days.push({ label: `${month + 1} 月 ${day} 日`, codex: null, bot: null });
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

export function UsagePage({ usage, onSettings }: { usage: UsageView; onSettings: () => void }) {
  const [tab, setTab] = useState<(typeof tabs)[number]>("工具");
  const days = usage.days.length > 0 ? usage.days : placeholderDays();
  const max = Math.max(0, ...days.map((day) => (day.codex ?? 0) + (day.bot ?? 0)));
  const scale = Math.max(max, 1);
  return (
    <div className="usage-page">
      <p className="usage-banner">合计不完整：{usage.banner.join("；")}。</p>
      <div className="usage-head">
        <h2>额度与余额</h2>
        <p>{usage.caption}</p>
      </div>
      <div className="quota-grid">
        {usage.cards.map((card, index) => (
          <article key={`${card.name}-${index}`} className="quota-tile">
            <h3>{cardTitle(card.name)}</h3>
            {card.name === "Cursor" ? (
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
                      <p className="share">本机记录</p>
                      <span className={chipClass(card.note)}>{card.note}</span>
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
            <p>本月可核实用量</p>
            <div className="token-figure">
              <strong>{usage.total}</strong>
              {usage.total === "—" ? null : <span>token</span>}
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
            <span><i className="swatch codex" />Codex</span>
          </div>
        </div>
        <div className="usage-chart" aria-label="按日本机记录">
          {days.map((day) => {
            const codex = day.codex ?? 0;
            const bot = day.bot ?? 0;
            const missing = day.codex == null && day.bot == null;
            const height = Math.max(8, Math.round(((codex + bot) / scale) * 150));
            return (
              <div key={day.label} className="chart-col">
                <div className="chart-slot">
                  {missing ? (
                    <span className="chart-gap" />
                  ) : (
                    <span className="chart-stack" style={{ height }}>
                      {codex > 0 ? <span className="bar codex" style={{ flexGrow: codex }} /> : null}
                      {bot > 0 ? <span className="bar bot" style={{ flexGrow: bot }} /> : null}
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
            {usage.details.map((row) => (
              <div key={row.name} className="detail-row">
                <span className="detail-name">{row.name}</span>
                <span>工具</span>
                <span className="num">{row.tokens}</span>
                <span className="num">{row.calls}</span>
                <span className="num">{row.fee}</span>
                <span className="source">
                  {row.source || "—"}
                  <AuthLink source={row.source} onSettings={onSettings} />
                </span>
              </div>
            ))}
          </div>
        ) : (
          <p className="chart-empty">暂不提供</p>
        )}
        <p className="detail-foot">Harness 与 DS Bot 共用本机调用记录，已按调用编号去重。不同单位与币种分别合计。</p>
      </section>
    </div>
  );
}

export function QuotaCard({ usage, onOpen }: { usage: UsageView; onOpen: () => void }) {
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
        <Ring percent={codex?.percent ?? null} />
        <div>
          <strong>{codex?.name ?? "Codex"}</strong>
          {codex?.reset ? <p className="reset-line">{codex.reset}</p> : null}
          <p className="quota-status">{codex?.note}</p>
        </div>
      </div>
      <div className="quota-line">
        <span>DeepSeek 余额</span>
        <strong>{balance && balance.value !== "—" ? balance.value : balance?.note ?? "未保存密钥"}</strong>
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
