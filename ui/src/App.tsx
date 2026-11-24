import { useEffect, useRef, useState } from "react";
import { Logo } from "./Logo";
import { isoDay } from "./DateRange";
import { emptyUsage, fallbackSnapshot, inTauri, listConversations, loadSnapshot, loadUsage, refreshUsage, saveSettings, scanSystem, sendNotice, type ConversationRow, type SysScan, type Snapshot, type SourceStatus, type UsageDay, type UsageView } from "./api";
import { loadDevFixture } from "./fixture";
import { fixtureHandoffRows, HandoffFilters, HandoffPage, parseUpdated, handoffSubtitle, sourcesWhenListFails, type ConversationSources, type HandoffFilter } from "./HandoffPage";
import { IconClock, IconPlus, IconRefresh, IconSearch } from "./icons";
import { SettingsPage } from "./SettingsPage";
import { fixtureSpaceSummary, formatBytes, scanAllTools, SpacePage, type SpaceScans } from "./SpacePage";
import { sideTotal, SysEnable, SysList, SystemPanel } from "./SystemSpace";
import type { SpaceSummary } from "./OverviewCards";
import { Console, type PendingItem } from "./Console";
import { ToolsPage } from "./ToolsPage";
import { applyAppearance } from "./theme";
import { spaceTotals, vendorName } from "./vendor-data";
import { SetupPage } from "./SetupPage";
import { toolMarks, vendorMarks } from "./tool-status";
import type { VendorId } from "./catalog";
import { UpdateBadge, UpdateGate, useUpdater } from "./update";
import { RangeControl, UsagePage, type RefreshPending, type UsageRange } from "./UsagePage";

const pages = [
  { id: "overview", label: "总览", nav: true },
  { id: "tools", label: "工具", nav: true },
  { id: "handoff", label: "交接", nav: true },
  { id: "space", label: "清理", nav: true },
  { id: "settings", label: "设置", nav: true },
  // 用量明细从总览的图表进入，不单独占导航
  { id: "usage", label: "用量明细", nav: false },
] as const;

type PageId = (typeof pages)[number]["id"];


function rangesLabel(range: UsageRange) {
  if (range === "today") return "今天";
  if (range === "7") return "7 天";
  if (range === "30") return "30 天";
  if (range === "custom") return "自定义";
  return "本月";
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
    forced === "tools" ||
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
  const [selectedVendor, setSelectedVendor] = useState<VendorId>("deepseek");
  const [handoffFocus, setHandoffFocus] = useState<{ tool: string; id: string; at: number } | undefined>(undefined);
  const [spaceScans, setSpaceScans] = useState<SpaceScans | null>(null);
  const [spaceScanning, setSpaceScanning] = useState(false);
  const [spaceScanFailed, setSpaceScanFailed] = useState(false);
  const [listed, setListed] = useState(false);
  const updater = useUpdater();
  useEffect(() => {
    // 换页时内容区回到顶部（工具直达的定位在设置页里另行处理）
    document.querySelector("main")?.scrollTo(0, 0);
  }, [page]);

  function openTool(id: VendorId) {
    setSelectedVendor(id);
    setPage("tools");
  }
  function openHandoff(row: ConversationRow | null) {
    if (row) setHandoffFocus({ tool: row.tool, id: row.id, at: Date.now() });
    setPage("handoff");
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
  const [spaceSide, setSpaceSide] = useState<"ai" | "system">("ai");
  const [sysScan, setSysScan] = useState<SysScan | null>(null);
  const [sysScanning, setSysScanning] = useState(false);
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
  const marks = vendorMarks(
    toolMarks(snapshot.tools, usage.details, fixture, attentionTools, staleTools, snapshot.grants),
  );
  const windows =
    typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
  const threshold = snapshot.settings.context_threshold_percent;
  const scripted = scriptedSources();
  const handoffRows = fixtureM3 ? fixtureHandoffRows() : newestFirst(conversations);
  const handoffSources: ConversationSources | null = fixtureM3
    ? { codex: "ok", ds_bot: "ok" }
    : scripted ?? sources;
  const overThreshold = handoffRows.filter((row) => row.percent != null && row.percent >= threshold).length;
  // 总览的空间：启动后在后台把已授权的各端扫一遍（只统计大小，不读内容）
  const spaceGranted = snapshot.grants.some(
    (grant) => grant.capability === "space_scan" && grant.status === "active",
  );
  useEffect(() => {
    if (!ready || fixture || !inTauri() || !spaceGranted || spaceScans) return;
    let alive = true;
    void scanAllTools()
      .then((scans) => {
        if (alive) setSpaceScans(scans);
      })
      .catch(() => {
        if (alive) setSpaceScanFailed(true);
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, spaceGranted]);
  const totals = spaceScans ? spaceTotals(spaceScans) : null;
  // 系统与本地模型：开启后才统计（只算大小）
  const sysEnabled = snapshot.settings.system_scan;
  function runSysScan() {
    if (!inTauri() || sysScanning) return;
    setSysScanning(true);
    void scanSystem()
      .then(setSysScan)
      .catch(() => undefined)
      .finally(() => setSysScanning(false));
  }
  function enableSysScan() {
    const next = { ...snapshot.settings, system_scan: true };
    setSnapshot((current) => ({ ...current, settings: next }));
    void saveSettings(next)
      .then(() => {
        setSysScan(null);
        runSysScan();
      })
      .catch(() => setSnapshot((current) => ({ ...current, settings: { ...next, system_scan: false } })));
  }
  useEffect(() => {
    if (page === "space" && sysEnabled && !sysScan) runSysScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page, sysEnabled]);
  useEffect(() => {
    if (spaceRequest > 0 && sysEnabled) runSysScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceRequest]);
  const aiLocalBytes = sysScan ? sideTotal(sysScan, "ai") : null;
  const aiBytes = (totals ?? []).reduce((sum, item) => sum + item.bytes, 0) + (aiLocalBytes ?? 0);
  const systemBytes = sysScan ? sideTotal(sysScan, "system") : null;
  useEffect(() => {
    applyAppearance(snapshot.settings.appearance ?? "system");
  }, [snapshot.settings.appearance]);
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
    setPending({ ds_bot: true, balance: true, codex: true, windows: true, claude: true, antigravity: true, grok: true });
    void refreshUsage().then((started) => {
      if (!started) {
        setRefreshing(false);
        setPending({ ds_bot: false, balance: false, codex: false, windows: false, claude: false, antigravity: false, grok: false });
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
          setPending({ ds_bot: true, balance: true, codex: true, windows: true, claude: true, antigravity: true, grok: true });
          return;
        }
        if (part === "done") {
          setRefreshing(false);
          setPending({ ds_bot: false, balance: false, codex: false, windows: false, claude: false, antigravity: false, grok: false });
          // 刷新后重读一次授权与 Key 状态（启动时钥匙串可能还没确认完）
          void loadSnapshot().then(setSnapshot).catch(() => undefined);
          return;
        }
        const custom = customRef.current;
        if (!["ds_bot", "balance", "codex", "windows", "claude", "antigravity", "grok"].includes(part)) return;
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
        setListed(true);
        setConversations(page.rows);
        setSources({
          codex: page.codex,
          ds_bot: page.ds_bot,
          harness: page.harness,
          cursor: page.cursor,
          claude: page.claude,
          antigravity: page.antigravity,
          workbuddy: page.workbuddy,
          grok_bot: page.grok_bot,
          codebuddy: page.codebuddy,
          cursor_cli: page.cursor_cli,
        });
      })
      .catch(() => {
        if (!alive) return;
        setListed(true);
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
      <>
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
      <UpdateGate updater={updater} />
      </>
    );
  }

  const hottest =
    [...handoffRows]
      .filter((row) => row.percent != null && row.percent >= threshold)
      .sort((a, b) => (b.percent ?? 0) - (a.percent ?? 0))[0] ?? null;
  const overVendors = (totals ?? []).filter((item) => item.bytes > snapshot.settings.space_threshold_gb * 1e9);
  const pendingList: PendingItem[] = [
    ...attentionItems(usage, keySaved, hottest ? overThreshold - 1 : overThreshold, null, snapshot.settings.space_threshold_gb).map(
      (item) => ({ ...item, to: item.title.includes("重新授权") ? ("tools" as const) : item.to }),
    ),
    ...(overVendors.length
      ? [
          {
            title: `${overVendors.length} 个工具超过 ${snapshot.settings.space_threshold_gb} GB`,
            detail: overVendors.map((item) => `${vendorName(item.vendor)} ${formatBytes(item.bytes, 1)}`).join(" · "),
            action: "清理",
            quiet: true,
            to: "space" as const,
          },
        ]
      : []),
  ];
  const navCurrent = current.id === "usage" ? "overview" : current.id;
  const pageTitle =
    page === "handoff"
      ? handoffSubtitle(handoffRows, handoffSources, threshold)
      : page === "space"
        ? spaceSide === "system"
          ? "系统缓存、开发工具、备份与系统文件 · 只统计大小，清理只移到废纸篓"
          : `按工具统计本机已知占用 · 阈值 ${snapshot.settings.space_threshold_gb} GB`
        : page === "usage"
          ? `${rangesLabel(range)} · ${usage.range_label}`
          : page === "settings"
            ? "阈值、刷新、外观与常驻"
            : "";

  return (
    <div className="window app-shell">
      <header className="topbar" inert={updater.info.mandatory ? true : undefined}>
        {windows ? null : <div className="traffic" data-tauri-drag-region aria-hidden="true" />}
        <div className="brand" data-tauri-drag-region>
          <Logo />
          <span>万象小助手</span>
        </div>
        <nav className="topnav" aria-label="主导航">
          {pages
            .filter((item) => item.nav)
            .map((item) => (
              <button
                key={item.id}
                type="button"
                aria-current={item.id === navCurrent ? "page" : undefined}
                onClick={() => setPage(item.id)}
              >
                {item.label}
                {item.id === "handoff" && overThreshold > 0 ? <span className="nav-count">{overThreshold}</span> : null}
              </button>
            ))}
        </nav>
        <div className="toolbar-drag" data-tauri-drag-region />
        <UpdateBadge updater={updater} onOpen={() => setPage("settings")} />
        <span className="topbar-stamp">{usage.stamp === "—" ? "更新于 —" : `${usage.stamp} 更新`}</span>
        <button
          type="button"
          className={refreshing ? "topbar-icon spinning" : "topbar-icon"}
          aria-label="刷新"
          aria-busy={refreshing}
          disabled={refreshing}
          onClick={onRefresh}
        >
          <IconRefresh />
        </button>
        <button
          type="button"
          className="topbar-icon"
          aria-label="搜索对话"
          onClick={() => {
            setPage("handoff");
            setSearchRequest((count) => count + 1);
          }}
        >
          <IconSearch />
        </button>
      </header>
      <main className={`page page-${page}`} inert={updater.info.mandatory ? true : undefined}>
        {pageTitle ? (
          <header className="page-head">
            <div className="title-block">
              <h1>{current.label}</h1>
              <p className="subtitle">{pageTitle}</p>
            </div>
            <div className="toolbar-actions">
              {page === "usage" ? (
                <>
                  <button type="button" className="text-button" onClick={() => setPage("overview")}>
                    返回总览
                  </button>
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
                </>
              ) : page === "handoff" ? (
                <>
                  <HandoffFilters filter={handoffFilter} onFilter={setHandoffFilter} />
                  <button type="button" className="primary-button toolbar-create" onClick={() => setPreviewRequest((value) => value + 1)}>
                    <IconPlus />
                    新建交接
                  </button>
                </>
              ) : page === "space" ? (
                <>
                <div className="space-sides" role="tablist" aria-label="清理范围">
                  <button type="button" role="tab" aria-selected={spaceSide === "ai"} onClick={() => setSpaceSide("ai")}>
                    AI 工具
                    <small>{totals ? formatBytes(aiBytes, 1) : ""}</small>
                  </button>
                  <button type="button" role="tab" aria-selected={spaceSide === "system"} onClick={() => setSpaceSide("system")}>
                    系统
                    <small>{systemBytes != null ? formatBytes(systemBytes, 1) : ""}</small>
                  </button>
                </div>
                <div className="glass-pill">
                  <IconClock />
                  <span>
                    {(spaceSide === "system" ? sysScanning : spaceScanning)
                      ? "正在扫描…"
                      : spaceSide === "system" && !sysEnabled
                        ? "系统未开启统计"
                        : spaceStamp
                          ? `扫描于 ${spaceStamp}`
                          : "尚未扫描"}
                  </span>
                  <span className="pill-rule" aria-hidden="true" />
                  <button
                    type="button"
                    className={(spaceSide === "system" ? sysScanning : spaceScanning) ? "icon-hit spinning" : "icon-hit"}
                    aria-label="重新扫描"
                    aria-busy={spaceSide === "system" ? sysScanning : spaceScanning}
                    disabled={spaceSide === "system" ? sysScanning : spaceScanning}
                    onClick={() => {
                      // 在「系统」这边点重新扫描就是要统计系统：没开启的直接开启
                      if (spaceSide === "system" && !sysEnabled) enableSysScan();
                      else setSpaceRequest((value) => value + 1);
                    }}
                  >
                    <IconRefresh />
                  </button>
                </div>
                </>
              ) : null}
            </div>
          </header>
        ) : null}
        {page === "settings" ? (
          <SettingsPage
            snapshot={snapshot}
            onChange={(update) => setSnapshot((current) => update(current))}
            onReload={() => {
              void loadSnapshot().then(setSnapshot);
            }}
            updater={updater}
            onTools={() => setPage("tools")}
          />
        ) : page === "usage" ? (
          <UsagePage usage={usage} pending={pending} range={range} onSettings={() => setPage("tools")} />
        ) : page === "overview" ? (
          <Console
            marks={marks}
            usage={usage}
            items={pendingList}
            hottest={hottest}
            space={totals}
            pendingLoading={inTauri() && !fixtureM3 && !fixtureM4 && fixtureKind !== "handoff" && !listed}
            spaceLoading={inTauri() && !fixture && spaceGranted && !spaceScans && !spaceScanFailed}
            thresholdGb={snapshot.settings.space_threshold_gb}
            onVendor={openTool}
            onGo={(to) => setPage(to === "tools" ? "tools" : to)}
            onHandoff={openHandoff}
            onUsage={() => setPage("usage")}
          />
        ) : page === "tools" ? (
          <ToolsPage
            marks={marks}
            selected={selectedVendor}
            onSelect={setSelectedVendor}
            usage={usage}
            rows={handoffRows}
            threshold={threshold}
            snapshot={snapshot}
            onChange={(update) => setSnapshot((current) => update(current))}
            onReload={() => {
              void loadSnapshot().then(setSnapshot);
            }}
            space={totals}
            onHandoff={openHandoff}
            onClean={() => setPage("space")}
          />
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
            onSettings={() => setPage("tools")}
            focus={handoffFocus}
          />
        ) : (
          <>
          <div className="space-side" hidden={spaceSide !== "ai"}>
          <SpacePage
            localAi={{
              bytes: aiLocalBytes,
              detail: sysEnabled ? (
                <SysList scan={sysScan} side="ai" scanning={sysScanning} windows={windows} onCleaned={runSysScan} />
              ) : (
                <SysEnable compact onEnable={enableSysScan} />
              ),
            }}
            fixture={fixtureM4}
            windows={windows}
            sheet={sheet === "confirm" || sheet === "result" ? sheet : ""}
            thresholdGb={snapshot.settings.space_threshold_gb}
            scanRequest={spaceRequest}
            initialScans={spaceScans}
            onScanning={setSpaceScanning}
            onStamp={setSpaceStamp}
            onSummary={setSpaceSummary}
            onScans={setSpaceScans}
          />
          </div>
          <div className="space-side" hidden={spaceSide !== "system"}>
            <SystemPanel
              scan={sysScan}
              enabled={sysEnabled}
              scanning={sysScanning}
              windows={windows}
              aiBytes={aiBytes}
              onEnable={enableSysScan}
              onCleaned={runSysScan}
            />
          </div>
          </>
        )}
      </main>
      <UpdateGate updater={updater} />
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
  if (part === "grok") {
    return {
      ...current,
      cards: current.cards.map((card) =>
        card.name.startsWith("Grok Bot") ? next.cards.find((item) => item.name === card.name) ?? card : card,
      ),
    };
  }
  if (part === "windows") {
    const fresh = next.cards.filter((card) => card.name.startsWith("ChatGPT"));
    const rest = current.cards.filter((card) => !card.name.startsWith("ChatGPT"));
    return { ...current, cards: [...fresh, ...rest] };
  }
  if (part === "ds_bot" || part === "codex" || part === "claude" || part === "antigravity") {
    const names =
      part === "ds_bot"
        ? ["DS Bot", "DeepSeek Harness"]
        : part === "codex"
          ? ["ChatGPT"]
          : part === "claude"
            ? ["Claude"]
            : ["Antigravity"];
    // 这一部分刷新到的柱子；其余工具保留上一次的值
    const fresh: Record<string, (keyof UsageDay)[]> = {
      ds_bot: ["bot", "harness"],
      codex: ["codex"],
      claude: ["claude"],
      antigravity: ["antigravity"],
    };
    const cards =
      part === "claude"
        ? current.cards.map((card) =>
            card.name.startsWith("Claude") ? next.cards.find((item) => item.name === card.name) ?? card : card,
          )
        : current.cards;
    return {
      ...current,
      cards,
      total: next.total,
      total_reason: next.total_reason,
      banner: next.banner,
      days: next.days.map((day) => {
        const previous = current.days.find((item) => item.label === day.label);
        const merged: UsageDay = { ...day };
        for (const key of ["codex", "bot", "harness", "claude", "antigravity"] as const) {
          if (fresh[part].includes(key)) continue;
          (merged as unknown as Record<string, number | null | undefined>)[key] = previous?.[key] ?? day[key];
        }
        return merged;
      }),
      details: current.details.map((row) =>
        names.includes(row.name) ? next.details.find((item) => item.name === row.name) ?? row : row,
      ),
    };
  }
  return current;
}
