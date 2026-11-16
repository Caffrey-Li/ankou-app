import { useEffect, useRef, useState } from "react";
import mark from "../../brand/mark.svg";
import { isoDay } from "./DateRange";
import { emptyUsage, fallbackSnapshot, inTauri, listConversations, loadSnapshot, loadUsage, refreshUsage, scanSpace, sendNotice, type ConversationRow, type Snapshot, type SourceStatus, type UsageView } from "./api";
import { loadDevFixture } from "./fixture";
import { fixtureHandoffRows, HandoffFilters, HandoffPage, parseUpdated, handoffSubtitle, sourcesWhenListFails, type ConversationSources, type HandoffFilter } from "./HandoffPage";
import { IconBubble, IconChart, IconClock, IconDisk, IconHome, IconPlus, IconRefresh, IconSearch, IconSet } from "./icons";
import { SettingsPage } from "./SettingsPage";
import { fixtureSpaceSummary, formatBytes, SpacePage } from "./SpacePage";
import { OverviewSpaceCard, OverviewTalkCard, type SpaceSummary } from "./OverviewCards";
import { SetupPage } from "./SetupPage";
import { OverviewToolRows, SidebarTools, toolMarks } from "./tool-status";
import type { ToolId } from "./catalog";
import { QuotaCard, RangeControl, UsagePage, type RefreshPending, type UsageRange } from "./UsagePage";

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

const fixtureKind = import.meta.env.DEV ? new URLSearchParams(window.location.search).get("fixture") : null;

function fixtureStatus(value: string | null): SourceStatus {
  if (
    value === "unauthorized" ||
    value === "needs_reauth" ||
    value === "read_failed" ||
    value === "server_unavailable" ||
    value === "no_records" ||
    value === "ok"
  ) {
    return value;
  }
  return "unauthorized";
}

function scriptedSources(): ConversationSources | null {
  if (fixtureKind !== "handoff") return null;
  const params = new URLSearchParams(window.location.search);
  return { codex: fixtureStatus(params.get("codex")), ds_bot: fixtureStatus(params.get("bot")) };
}
const fixtureClock = fixtureKind === "m2" || fixtureKind === "m3" || fixtureKind === "m4";

function shownDate() {
  return fixtureClock ? new Date(2026, 9, 25) : new Date();
}

function newestFirst(rows: ConversationRow[]) {
  return rows
    .map((row, index) => ({ row, index, at: parseUpdated(row.updated)?.getTime() ?? -1 }))
    .sort((a, b) => b.at - a.at || a.index - b.index)
    .map((item) => item.row);
}

/** 「558,117,107」→「558.1M」，与 Figma 的写法一致。 */
export function compactTokens(text: string) {
  const value = Number(text.replace(/,/g, ""));
  if (!Number.isFinite(value)) return text;
  if (value >= 1e9) return `${(value / 1e9).toFixed(2)}B`;
  if (value >= 1e6) return `${(value / 1e6).toFixed(2)}M`;
  if (value >= 1e4) return `${(value / 1e3).toFixed(1)}K`;
  return text;
}

export function attentionItems(
  usage: UsageView,
  keySaved: boolean,
  overThreshold: number,
  space: SpaceSummary | null,
  thresholdGb: number,
) {
  const items: { title: string; detail: string; action: string; quiet: boolean; to: "settings" | "handoff" | "space" }[] = [];
  if (space && space.known > thresholdGb * 1_000_000_000) {
    items.push({
      title: `${space.tool} 本机占用 ${formatBytes(space.known, 1)}`,
      detail:
        space.cleanable > 0
          ? `超过提醒阈值 ${thresholdGb} GB · 有证据可安全清理约 ${formatBytes(space.cleanable, 1)}`
          : `超过提醒阈值 ${thresholdGb} GB`,
      action: "查看清理",
      quiet: false,
      to: "space",
    });
  }
  if (!keySaved) {
    items.push({
      title: "余额密钥未保存",
      detail: "保存后才能查询 DeepSeek 余额",
      action: "去设置",
      quiet: true,
      to: "settings",
    });
  }
  if (overThreshold > 0) {
    items.push({
      title: `${overThreshold} 个对话超过提醒阈值`,
      detail: "可以交接后再继续",
      action: "去交接",
      quiet: false,
      to: "handoff",
    });
  }
  const names = new Set<string>();
  for (const card of usage.cards) {
    if (card.note === "需要重新授权") names.add(card.name.split(" · ")[0]);
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
      to: "settings",
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
  const [focusTool, setFocusTool] = useState<{ id: ToolId; at: number } | undefined>(undefined);
  function openTool(id: ToolId) {
    setFocusTool({ id, at: Date.now() });
    setPage("settings");
  }
  const [snapshot, setSnapshot] = useState<Snapshot>(fallbackSnapshot);
  const [usage, setUsage] = useState<UsageView>(emptyUsage);
  const [fixture, setFixture] = useState(false);
  const [range, setRange] = useState<UsageRange>("month");
  const [customStart, setCustomStart] = useState("");
  const [customEnd, setCustomEnd] = useState("");
  const [ready, setReady] = useState(!inTauri());
  const [refreshing, setRefreshing] = useState(false);
  const [handoffFilter, setHandoffFilter] = useState<HandoffFilter>("all");
  const [handoffQuery, setHandoffQuery] = useState("");
  const [previewRequest, setPreviewRequest] = useState(0);
  const [spaceRequest, setSpaceRequest] = useState(0);
  const [searchRequest, setSearchRequest] = useState(0);
  const [spaceStamp, setSpaceStamp] = useState(fixtureKind === "m4" ? "19:10" : "");
  const [spaceSummary, setSpaceSummary] = useState<SpaceSummary | null>(
    fixtureKind === "m4"
      ? { tool: fixtureSpaceSummary.tool, known: 8_210_000_000, protectedBytes: 6_200_000_000, cleanable: 2_010_000_000 }
      : null,
  );
  const [conversations, setConversations] = useState<ConversationRow[]>([]);
  const [sources, setSources] = useState<ConversationSources | null>(null);
  const fixtureM3 = fixtureKind === "m3";
  const fixtureM4 = fixtureKind === "m4";
  const sheet = new URLSearchParams(window.location.search).get("sheet");
  const handoffSheet = sheet === "preview" || sheet === "confirm" ? sheet : "";
  const [pending, setPending] = useState<RefreshPending>({
    ds_bot: false,
    balance: false,
    codex: false,
    windows: false,
  });
  const rangeRef = useRef(range);
  const customRef = useRef({ start: customStart, end: customEnd });
  rangeRef.current = range;
  customRef.current = { start: customStart, end: customEnd };
  const current = pages.find((item) => item.id === page) ?? pages[0];
  const preview = new URLSearchParams(window.location.search).get("preview") === "1";
  const keySaved = snapshot.keySaved || fixture || fixtureM4;
  const attentionTools = new Set<string>(
    (fixtureM3 ? fixtureHandoffRows() : conversations)
      .filter((row) => row.percent != null && row.percent >= snapshot.settings.context_threshold_percent)
      .map((row) => row.tool),
  );
  const staleTools = new Set<string>(
    usage.cards.some((card) => card.name.startsWith("ChatGPT") && card.note.includes("过时")) ? ["codex"] : [],
  );
  const marks = toolMarks(snapshot.tools, usage.details, fixture, attentionTools, staleTools, snapshot.grants);
  const windows =
    typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
  const threshold = snapshot.settings.context_threshold_percent;
  const scripted = scriptedSources();
  const handoffRows = fixtureM3 ? fixtureHandoffRows() : newestFirst(conversations);
  const handoffSources: ConversationSources | null = fixtureM3
    ? { codex: "ok", ds_bot: "ok" }
    : scripted ?? sources;
  const overThreshold = handoffRows.filter((row) => row.percent != null && row.percent >= threshold).length;
  // 概览的空间卡：启动后在后台做一次 Harness 只读扫描（只统计大小）
  const harnessSpaceGranted = snapshot.grants.some(
    (grant) => grant.tool === "deepseek_harness" && grant.capability === "space_scan" && grant.status === "active",
  );
  useEffect(() => {
    if (!ready || fixture || !inTauri() || !harnessSpaceGranted || spaceSummary) return;
    let alive = true;
    void scanSpace("deepseek_harness")
      .then((scan) => {
        if (!alive || scan.cancelled) return;
        setSpaceSummary({
          tool: "DeepSeek Harness",
          known: scan.protected_bytes + scan.cleanable_bytes,
          protectedBytes: scan.protected_bytes,
          cleanable: scan.cleanable_bytes,
        });
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, harnessSpaceGranted]);
  const notified = useRef(new Set<string>());
  const noticeItems = attentionItems(
    usage,
    keySaved,
    overThreshold,
    spaceSummary,
    snapshot.settings.space_threshold_gb,
  ).filter((item) => !item.quiet);
  const noticeKey = noticeItems.map((item) => item.title).join("\n");
  useEffect(() => {
    if (!snapshot.settings.notifications || !inTauri()) return;
    for (const item of noticeItems) {
      if (notified.current.has(item.title)) continue;
      notified.current.add(item.title);
      void sendNotice(item.title, item.detail).catch(() => undefined);
    }
    // 同一件事只提醒一次；列表变化时才重新检查
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noticeKey, snapshot.settings.notifications]);
  function onRefresh() {
    if (!inTauri() || refreshing) return;
    setRefreshing(true);
    setPending({ ds_bot: true, balance: true, codex: true, windows: true });
    void refreshUsage().then((started) => {
      if (!started) {
        setRefreshing(false);
        setPending({ ds_bot: false, balance: false, codex: false, windows: false });
      }
    });
  }

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<string>("navigate", (event) => {
        const next = pages.find((item) => item.id === event.payload);
        if (next) setPage(next.id);
      }).then((stop) => {
        if (alive) unlisten = stop;
        else stop();
      }),
    );
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) => {
      if (!alive) return;
      void listen<string>("usage-item", (event) => {
        const part = event.payload;
        if (part === "started") {
          setRefreshing(true);
          setPending({ ds_bot: true, balance: true, codex: true, windows: true });
          return;
        }
        if (part === "done") {
          setRefreshing(false);
          setPending({ ds_bot: false, balance: false, codex: false, windows: false });
          // 刷新后重读一次授权与 Key 状态（启动时钥匙串可能还没确认完）
          void loadSnapshot().then(setSnapshot).catch(() => undefined);
          return;
        }
        const custom = customRef.current;
        if (part !== "ds_bot" && part !== "balance" && part !== "codex" && part !== "windows") return;
        void loadUsage(rangeRef.current, custom.start, custom.end).then((next) => {
          setUsage((current) => mergeUsage(current, part, next));
          setPending((current) => ({ ...current, [part]: false }));
        });
      }).then((stop) => {
        if (alive) unlisten = stop;
        else stop();
      });
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    loadSnapshot()
      .then((next) => {
        if (alive) setSnapshot(next);
      })
      .catch(() => undefined)
      .finally(() => {
        // 读取失败也要把界面画出来，各页按「未授权 / 读取失败」显示
        if (alive) setReady(true);
      });
    return () => {
      alive = false;
    };
  }, []);

  const conversationGrants = snapshot.grants
    .filter((grant) => grant.capability === "conversation" && grant.status === "active")
    .map((grant) => grant.tool)
    .sort()
    .join(",");
  useEffect(() => {
    if (!inTauri() || fixtureM3 || fixtureM4 || fixtureKind === "handoff") return;
    let alive = true;
    void listConversations()
      .then((page) => {
        if (!alive) return;
        setConversations(page.rows);
        setSources({ codex: page.codex, ds_bot: page.ds_bot, harness: page.harness, cursor: page.cursor });
      })
      .catch(() => {
        if (!alive) return;
        setSources(sourcesWhenListFails());
      });
    return () => {
      alive = false;
    };
    // 授权变化或打开对话页时重新列出
  }, [fixtureM3, conversationGrants, page === "handoff"]);

  useEffect(() => {
    if (!import.meta.env.DEV || fixtureKind === "m4") return;
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
    if (!inTauri() || fixture || fixtureM4) return;
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
        onReload={() => {
          void loadSnapshot().then(setSnapshot);
        }}
      />
    );
  }

  return (
    <div className="window">
      <aside className="sidebar">
        {windows ? null : <div className="traffic" data-tauri-drag-region aria-hidden="true" />}
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
        <SidebarTools marks={marks} onOpen={openTool} />
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
                    : page === "overview" || page === "handoff"
                      ? page === "handoff"
                        ? handoffSubtitle(handoffRows, handoffSources, threshold)
                        : formatDate(shownDate())
                      : page === "space"
                        ? `按工具统计本机已知占用 · 阈值 ${snapshot.settings.space_threshold_gb} GB`
                        : formatDate(shownDate())}
            </p>
          </div>
          <div className="toolbar-drag" data-tauri-drag-region />
          {page === "settings" ? null : (
          <div className="toolbar-actions">
            {page === "usage" ? (
              <RangeControl
                range={range}
                start={customStart}
                end={customEnd}
                today={isoDay(shownDate())}
                onRange={setRange}
                onCustom={(start, end) => {
                  setCustomStart(start);
                  setCustomEnd(end);
                }}
              />
            ) : page === "handoff" ? (
              <>
                <HandoffFilters filter={handoffFilter} onFilter={setHandoffFilter} />
                <button type="button" className="primary-button toolbar-create" onClick={() => setPreviewRequest((value) => value + 1)}>
                  <IconPlus />
                  新建交接
                </button>
              </>
            ) : page === "space" ? (
              <div className="glass-pill">
                <IconClock />
                <span>{spaceStamp ? `扫描于 ${spaceStamp}` : "尚未扫描"}</span>
                <span className="pill-rule" aria-hidden="true" />
                <button
                  type="button"
                  className="icon-hit"
                  aria-label="刷新"
                  onClick={() => setSpaceRequest((value) => value + 1)}
                >
                  <IconRefresh />
                </button>
              </div>
            ) : (
              <div className="glass-pill">
                <IconClock />
                <span>{usage.stamp === "—" ? "更新于 —" : `更新于 ${usage.stamp}`}</span>
                <span className="pill-rule" aria-hidden="true" />
                <button
                  type="button"
                  className={refreshing ? "icon-hit spinning" : "icon-hit"}
                  aria-label="刷新"
                  aria-busy={refreshing}
                  disabled={refreshing}
                  onClick={onRefresh}
                >
                  <IconRefresh />
                </button>
              </div>
            )}
            {page === "usage" ? (
              <button
                type="button"
                className={refreshing ? "glass-icon spinning" : "glass-icon"}
                aria-label="刷新"
                aria-busy={refreshing}
                disabled={refreshing}
                onClick={onRefresh}
              >
                <IconRefresh />
              </button>
            ) : page === "handoff" || page === "space" ? null : (
              <button
                type="button"
                className="glass-icon"
                aria-label="搜索对话"
                onClick={() => {
                  setPage("handoff");
                  setSearchRequest((count) => count + 1);
                }}
              >
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
            onReload={() => {
              void loadSnapshot().then(setSnapshot);
            }}
            focusTool={focusTool}
          />
        ) : page === "usage" ? (
          <UsagePage usage={usage} pending={pending} range={range} onSettings={() => setPage("settings")} />
        ) : page === "overview" ? (
          <div className="overview-page">
            <Attention
              usage={usage}
              keySaved={keySaved}
              overThreshold={overThreshold}
              space={spaceSummary}
              thresholdGb={snapshot.settings.space_threshold_gb}
              onSettings={() => setPage("settings")}
              onHandoff={() => setPage("handoff")}
              onSpace={() => setPage("space")}
            />
            <div className="overview-row">
              <QuotaCard usage={usage} pending={pending} onOpen={() => setPage("usage")} />
              <OverviewTalkCard
                rows={handoffRows}
                threshold={threshold}
                onOpen={() => setPage("handoff")}
              />
              <OverviewSpaceCard
                summary={spaceSummary}
                thresholdGb={snapshot.settings.space_threshold_gb}
                onOpen={() => setPage("space")}
              />
            </div>
            <section className="overview-card tool-board">
              <div className="tool-board-head">
                <h2>工具</h2>
                <button type="button" className="text-link" onClick={() => setPage("settings")}>
                  管理授权
                </button>
              </div>
              <OverviewToolRows marks={marks} stamp={usage.stamp} onOpen={openTool} />
            </section>
          </div>
        ) : page === "handoff" ? (
          <HandoffPage
            rows={handoffRows}
            sources={handoffSources}
            threshold={threshold}
            filter={handoffFilter}
            query={handoffQuery}
            sheet={handoffSheet}
            previewRequest={previewRequest}
            onQuery={setHandoffQuery}
            defaultMode={snapshot.settings.summary_mode === "local" ? "local" : "ai"}
            searchRequest={searchRequest}
            onSettings={() => setPage("settings")}
          />
        ) : page === "space" ? (
          <SpacePage
            fixture={fixtureM4}
            windows={windows}
            sheet={sheet === "confirm" || sheet === "result" ? sheet : ""}
            thresholdGb={snapshot.settings.space_threshold_gb}
            scanRequest={spaceRequest}
            onStamp={setSpaceStamp}
            onSummary={setSpaceSummary}
          />
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

function mergeUsage(current: UsageView, part: string, next: UsageView): UsageView {
  if (part === "balance") {
    return {
      ...current,
      cards: current.cards.map((card) =>
        card.name === "DeepSeek 账户" ? next.cards.find((item) => item.name === card.name) ?? card : card,
      ),
    };
  }
  if (part === "windows") {
    const fresh = next.cards.filter((card) => card.name.startsWith("ChatGPT"));
    const rest = current.cards.filter((card) => !card.name.startsWith("ChatGPT"));
    return { ...current, cards: [...fresh, ...rest] };
  }
  if (part === "ds_bot" || part === "codex") {
    const names = part === "ds_bot" ? ["DS Bot", "DeepSeek Harness"] : ["ChatGPT"];
    return {
      ...current,
      total: next.total,
      total_reason: next.total_reason,
      banner: next.banner,
      days: next.days.map((day) => {
        const previous = current.days.find((item) => item.label === day.label);
        return part === "ds_bot"
          ? { ...day, codex: previous?.codex ?? day.codex }
          : { ...day, bot: previous?.bot ?? day.bot, harness: previous?.harness ?? day.harness };
      }),
      details: current.details.map((row) =>
        names.includes(row.name) ? next.details.find((item) => item.name === row.name) ?? row : row,
      ),
    };
  }
  return current;
}

function Attention({
  usage,
  keySaved,
  overThreshold,
  space,
  thresholdGb,
  onSettings,
  onHandoff,
  onSpace,
}: {
  usage: UsageView;
  keySaved: boolean;
  overThreshold: number;
  space: SpaceSummary | null;
  thresholdGb: number;
  onSettings: () => void;
  onHandoff: () => void;
  onSpace: () => void;
}) {
  const items = attentionItems(usage, keySaved, overThreshold, space, thresholdGb);
  const total = usage.total && usage.total !== "—" ? `本月可核实用量 ${compactTokens(usage.total)} token` : "";
  if (items.length === 0) {
    return (
      <section className="overview-card attention attention-calm">
        <p className="attention-kicker calm">
          <i />
          运行正常
        </p>
        <h2>一切正常</h2>
        <p className="attention-lead">{["没有需要你处理的事", total].filter(Boolean).join(" · ")}</p>
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
      {total ? <p className="attention-lead">{total}</p> : null}
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
            onClick={() => (item.to === "handoff" ? onHandoff() : item.to === "space" ? onSpace() : onSettings())}
          >
            {item.action}
          </button>
        </div>
      ))}
    </section>
  );
}
