import { useEffect, useState } from "react";
import mark from "../../brand/mark.svg";
import {
  emptyUsage,
  fallbackSnapshot,
  inTauri,
  loadSnapshot,
  loadUsage,
  refreshUsage,
  type Snapshot,
  type UsageView,
} from "./api";
import { loadDevFixture } from "./fixture";
import {
  IconBubble,
  IconChart,
  IconClock,
  IconDisk,
  IconHome,
  IconRefresh,
  IconSearch,
  IconSet,
} from "./icons";
import { SettingsPage } from "./SettingsPage";
import { SetupPage } from "./SetupPage";
import { OverviewToolRows, SidebarTools, toolMarks } from "./tool-status";
import { QuotaCard, RangeControl, UsagePage, type UsageRange } from "./UsagePage";

const pages = [
  { id: "overview", label: "概览", icon: <IconHome /> },
  { id: "usage", label: "额度与用量", icon: <IconChart /> },
  { id: "handoff", label: "对话与交接", icon: <IconBubble /> },
  { id: "space", label: "空间管理", icon: <IconDisk /> },
  { id: "settings", label: "设置", icon: <IconSet /> },
] as const;

type PageId = (typeof pages)[number]["id"];

const weekdays = ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"];

function rangesLabel(range: UsageRange) {
  if (range === "today") return "今天";
  if (range === "7") return "7 天";
  if (range === "30") return "30 天";
  if (range === "custom") return "自定义";
  return "本月";
}

function formatDate(date: Date) {
  return `${weekdays[date.getDay()]} · ${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

const fixtureClock = import.meta.env.DEV && new URLSearchParams(window.location.search).get("fixture") === "m2";

function shownDate() {
  return fixtureClock ? new Date(2026, 9, 25) : new Date();
}

function attentionItems(usage: UsageView, keySaved: boolean) {
  const items: { title: string; detail: string; action: string; quiet: boolean }[] = [];
  if (!keySaved) {
    items.push({
      title: "余额密钥未保存",
      detail: "保存后才能查询 DeepSeek 余额",
      action: "去设置",
      quiet: true,
    });
  }
  const names = new Set<string>();
  for (const card of usage.cards) {
    if (card.note === "需要重新授权") names.add(card.name);
  }
  for (const row of usage.details) {
    if (row.tokens === "需要重新授权" || row.source === "需要重新授权") names.add(row.name);
  }
  for (const name of names) {
    items.push({
      title: `${name} 需要重新授权`,
      detail: "用量不再计入，直到重新授权",
      action: "重新授权",
      quiet: false,
    });
  }
  return items;
}

function initialPage(): PageId {
  const forced = new URLSearchParams(window.location.search).get("page");
  if (
    forced === "overview" ||
    forced === "usage" ||
    forced === "handoff" ||
    forced === "space" ||
    forced === "settings"
  ) {
    return forced;
  }
  return "overview";
}

export function App() {
  const forced = new URLSearchParams(window.location.search).get("page");
  const [page, setPage] = useState<PageId>(initialPage);
  const [snapshot, setSnapshot] = useState<Snapshot>(fallbackSnapshot);
  const [usage, setUsage] = useState<UsageView>(emptyUsage);
  const [fixture, setFixture] = useState(false);
  const [range, setRange] = useState<UsageRange>("month");
  const [customStart, setCustomStart] = useState("");
  const [customEnd, setCustomEnd] = useState("");
  const [ready, setReady] = useState(!inTauri());
  const current = pages.find((item) => item.id === page) ?? pages[0];
  const preview = new URLSearchParams(window.location.search).get("preview") === "1";
  const keySaved = snapshot.keySaved || fixture;
  const marks = toolMarks(snapshot.tools, usage.details, fixture);
  const windows =
    typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    loadSnapshot().then((next) => {
      if (!alive) return;
      setSnapshot(next);
      setReady(true);
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!import.meta.env.DEV) return;
    let alive = true;
    void loadDevFixture().then((next) => {
      if (!alive || !next) return;
      setUsage(next);
      setFixture(true);
      document.documentElement.dataset.fixture = "m2";
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!inTauri() || fixture) return;
    let alive = true;
    loadUsage(range, customStart, customEnd).then((next) => {
      if (alive) setUsage(next);
    });
    return () => {
      alive = false;
    };
  }, [range, customStart, customEnd, fixture]);

  if (!ready) return <div className="window" />;
  if (
    forced === "setup" ||
    (inTauri() && !snapshot.settings.setup_complete && !preview && forced !== "settings")
  ) {
    return (
      <SetupPage
        snapshot={snapshot}
        onDone={() =>
          setSnapshot({
            ...snapshot,
            settings: { ...snapshot.settings, setup_complete: true },
          })
        }
      />
    );
  }

  return (
    <div className="window">
      <aside className="sidebar">
        {windows ? null : <div className="traffic" aria-hidden="true" />}
        <div className="brand">
          <img src={mark} alt="" width={22} height={22} />
          <span>万象小助手</span>
        </div>
        <nav aria-label="主导航">
          {pages.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-current={item.id === current.id ? "page" : undefined}
              onClick={() => setPage(item.id)}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </nav>
        <div className="nav-gap" aria-hidden="true" />
        <p className="tools-label">工具</p>
        <SidebarTools marks={marks} />
        <p className="local">
          <span>本机运行</span>
          <span>数据只保存在这台电脑</span>
        </p>
      </aside>
      <main>
        <header className="toolbar">
          <div className="title-block">
            <h1>{current.label}</h1>
            <p className="subtitle">
              {page === "settings"
                ? "阈值、刷新、授权与常驻"
                : page === "usage"
                  ? `${rangesLabel(range)} · ${usage.range_label}`
                  : formatDate(shownDate())}
            </p>
          </div>
          {page === "settings" ? null : (
          <div className="toolbar-actions">
            {page === "usage" ? (
              <RangeControl
                range={range}
                start={customStart}
                end={customEnd}
                onRange={setRange}
                onCustom={(start, end) => {
                  setCustomStart(start);
                  setCustomEnd(end);
                }}
              />
            ) : (
              <div className="glass-pill">
                <IconClock />
                <span>{usage.stamp === "—" ? "更新于 —" : `更新于 ${usage.stamp}`}</span>
                <span className="pill-rule" aria-hidden="true" />
                <button
                  type="button"
                  className="icon-hit"
                  aria-label="刷新"
                  onClick={() => {
                    if (!inTauri()) return;
                    void refreshUsage(range, customStart, customEnd).then(setUsage);
                  }}
                >
                  <IconRefresh />
                </button>
              </div>
            )}
            {page === "usage" ? (
              <button
                type="button"
                className="glass-icon"
                aria-label="刷新"
                onClick={() => {
                  if (!inTauri()) return;
                  void refreshUsage(range, customStart, customEnd).then(setUsage);
                }}
              >
                <IconRefresh />
              </button>
            ) : (
              <button type="button" className="glass-icon" aria-label="搜索">
                <IconSearch />
              </button>
            )}
          </div>
          )}
        </header>
        {page === "settings" ? (
          <SettingsPage
            snapshot={snapshot}
            onChange={(update) => setSnapshot((current) => update(current))}
          />
        ) : page === "usage" ? (
          <UsagePage usage={usage} onSettings={() => setPage("settings")} />
        ) : page === "overview" ? (
          <div className="overview-page">
            <Attention usage={usage} keySaved={keySaved} onSettings={() => setPage("settings")} />
            <div className="overview-row">
              <QuotaCard usage={usage} onOpen={() => setPage("usage")} />
              <section className="overview-card placeholder-card">
                <div className="quota-title">
                  <IconBubble />
                  <span>对话</span>
                  <span className="quota-chev" aria-hidden="true">›</span>
                </div>
                <p className="overview-pending">尚未接入</p>
              </section>
              <section className="overview-card placeholder-card">
                <div className="quota-title">
                  <IconDisk />
                  <span>空间</span>
                  <span className="quota-chev" aria-hidden="true">›</span>
                </div>
                <p className="overview-pending">尚未接入</p>
              </section>
            </div>
            <section className="overview-card tool-board">
              <div className="tool-board-head">
                <h2>工具</h2>
                <button type="button" className="text-link" onClick={() => setPage("settings")}>
                  管理授权
                </button>
              </div>
              <OverviewToolRows marks={marks} onOpen={() => setPage("settings")} />
            </section>
          </div>
        ) : (
        <section className="empty" aria-label="页面内容">
          <p className="lead">主窗口已就绪，这一页还没有数据。</p>
          <p className="detail">
            对话和空间会在后续里程碑接入。当前没有读取这些页面的本机数据。
          </p>
          <span className="badge">未验证</span>
        </section>
        )}
      </main>
    </div>
  );
}

function Attention({
  usage,
  keySaved,
  onSettings,
}: {
  usage: UsageView;
  keySaved: boolean;
  onSettings: () => void;
}) {
  const items = attentionItems(usage, keySaved);
  if (items.length === 0) {
    return (
      <section className="overview-card attention attention-ok">
        <strong>一切正常</strong>
        <span>没有需要你处理的事</span>
      </section>
    );
  }
  return (
    <section className="overview-card attention">
      <p className="attention-kicker">
        <i />
        需要处理
      </p>
      <h2>{items.length} 件事需要你处理</h2>
      <p className="attention-lead">其余工具运行正常</p>
      {items.map((item) => (
        <div key={item.title} className="attention-item">
          <span className="attention-icon" aria-hidden="true">
            <IconSet />
          </span>
          <span className="attention-copy">
            <strong>{item.title}</strong>
            <span>{item.detail}</span>
          </span>
          <button
            type="button"
            className={item.quiet ? "attention-action quiet" : "attention-action"}
            onClick={onSettings}
          >
            {item.action}
          </button>
        </div>
      ))}
    </section>
  );
}
