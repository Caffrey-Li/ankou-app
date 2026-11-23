import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Logo } from "./Logo";
import {
  emptyUsage,
  fallbackSnapshot,
  inTauri,
  listConversations,
  loadSnapshot,
  loadUsage,
  refreshUsage,
  trayAction,
  type ConversationRow,
  type Snapshot,
  type UsageView,
} from "./api";
import { specOf, vendorOf, vendors } from "./catalog";
import { Ring } from "./Console";
import { IconRefresh, IconSet } from "./icons";
import { applyAppearance } from "./theme";
import { ToolIcon } from "./tool-status";
import { cnCount, vendorHeadline } from "./vendor-data";
import "./settings.css";
import "./tray.css";

type TrayItem = {
  icon: "bubble" | "disk" | "alert";
  title: string;
  detail: string;
  action: string;
  to: "handoff" | "space" | "settings";
};

export function trayItems(rows: ConversationRow[], usage: UsageView, snapshot: Snapshot): TrayItem[] {
  const threshold = snapshot.settings.context_threshold_percent;
  const items: TrayItem[] = [];
  const hot = rows
    .filter((row) => row.percent != null && row.percent >= threshold)
    .sort((a, b) => (b.percent ?? 0) - (a.percent ?? 0));
  for (const row of hot.slice(0, 2)) {
    items.push({
      icon: "bubble",
      title: row.title,
      detail: `${row.tool} · 上下文 ${row.percent}%`,
      action: "交接",
      to: "handoff",
    });
  }
  const reauth = new Set<string>();
  for (const card of usage.cards) {
    if (card.note === "需要重新授权") reauth.add(card.name.split(" · ")[0]);
  }
  for (const row of usage.details) {
    if (row.tokens === "需要重新授权" || row.source === "需要重新授权") reauth.add(row.name);
  }
  for (const name of reauth) {
    items.push({ icon: "alert", title: name, detail: "需要重新授权", action: "处理", to: "settings" });
  }
  return items.slice(0, 3);
}

export function TrayPanel() {
  const [snapshot, setSnapshot] = useState<Snapshot>(fallbackSnapshot);
  const [usage, setUsage] = useState<UsageView>(emptyUsage);
  const [rows, setRows] = useState<ConversationRow[]>([]);
  const [refreshing, setRefreshing] = useState(false);

  const reload = useCallback(() => {
    void loadSnapshot().then((next) => {
      setSnapshot(next);
      applyAppearance(next.settings.appearance ?? "system");
    });
    void loadUsage("month").then(setUsage);
    void listConversations()
      .then((page) => setRows(page.rows))
      .catch(() => setRows([]));
  }, []);

  useEffect(() => {
    reload();
    const onFocus = () => reload();
    window.addEventListener("focus", onFocus);
    if (!inTauri()) return () => window.removeEventListener("focus", onFocus);
    let stop: (() => void) | undefined;
    let alive = true;
    void import("@tauri-apps/api/event").then(({ listen }) => {
      void listen<Snapshot["settings"]["appearance"]>("appearance", (event) => applyAppearance(event.payload));
      return listen<string>("usage-item", (event) => {
        if (event.payload === "started") setRefreshing(true);
        if (event.payload === "done") {
          setRefreshing(false);
          reload();
        }
      }).then((unlisten) => {
        if (alive) stop = unlisten;
        else unlisten();
      });
    });
    return () => {
      alive = false;
      stop?.();
      window.removeEventListener("focus", onFocus);
    };
  }, [reload]);

  const panelRef = useRef<HTMLDivElement>(null);
  const items = trayItems(rows, usage, snapshot);
  const shape = `${items.length}|${refreshing}|${rows.length}|${usage.cards.length}`;
  useLayoutEffect(() => {
    const panel = panelRef.current;
    if (!panel || !inTauri()) return;
    // 面板四周留出阴影空间：左右 16，上 8，下 32
    const width = Math.ceil(panel.offsetWidth + 32);
    const height = Math.ceil(panel.offsetHeight + 40);
    void import("@tauri-apps/api/window").then(({ getCurrentWindow, LogicalSize }) =>
      getCurrentWindow().setSize(new LogicalSize(width, height)),
    );
  }, [shape]);
  const threshold = snapshot.settings.context_threshold_percent;
  const hot = rows
    .filter((row) => row.percent != null && row.percent >= threshold)
    .sort((a, b) => (b.percent ?? 0) - (a.percent ?? 0));
  const top = hot[0] ?? null;
  const quotas = vendors
    .filter((vendor) => ["openai", "anthropic", "grok", "deepseek"].includes(vendor.id))
    .map((vendor) => ({ vendor, head: vendorHeadline(usage, vendor) }))
    .filter(({ head }) => head.kind === "ring" || head.value !== "—")
    .slice(0, 4);
  const reauth = items.filter((item) => item.icon === "alert");

  return (
    <div className="tray-panel" ref={panelRef} role="dialog" aria-label="万象快速面板">
      <div className="tray-head">
        <Logo />
        <strong>万象</strong>
        <span className="tray-stamp">{refreshing ? "刷新中" : usage.stamp === "—" ? "尚未刷新" : `${usage.stamp} 更新`}</span>
        <span className="grow" />
        <button
          type="button"
          className="tray-icon-button"
          aria-label="刷新"
          disabled={refreshing}
          onClick={() => {
            setRefreshing(true);
            void refreshUsage().then((started) => {
              if (!started) setRefreshing(false);
            });
          }}
        >
          <IconRefresh />
        </button>
        <button type="button" className="tray-icon-button" aria-label="设置" onClick={() => void trayAction("settings")}>
          <IconSet />
        </button>
      </div>

      {top ? (
        <div className="tray-alert">
          <span className="tray-alert-kicker">对话快满了 · {top.percent}%</span>
          <strong>{top.title}</strong>
          <span className="tray-alert-row">
            <ToolIcon id={vendorOf(top.tool)?.icon ?? top.tool} className="tray-alert-icon" />
            <span>{specOf(top.tool)?.name ?? top.tool}</span>
            <button type="button" onClick={() => void trayAction("handoff")}>
              一键交接
            </button>
          </span>
        </div>
      ) : (
        <p className="tray-status">
          <i className={reauth.length > 0 ? "attention" : "ok"} />
          {reauth.length > 0 ? `${reauth.length} 件事需要处理` : "一切正常"}
        </p>
      )}

      {reauth.map((item) => (
        <div className="tray-item" key={`${item.title}-${item.detail}`}>
          <span className="tray-copy">
            <strong>{item.title}</strong>
            <span>{item.detail}</span>
          </span>
          <span className="grow" />
          <button type="button" className="text-button" onClick={() => void trayAction("overview")}>
            {item.action}
          </button>
        </div>
      ))}

      {quotas.length > 0 ? (
        <section className="tray-section">
          <h2>额度</h2>
          <div className="tray-quotas">
            {quotas.map(({ vendor, head }) => (
              <div key={vendor.id} className="tray-quota-tile">
                {head.kind === "ring" ? (
                  <Ring percent={head.percent} size={44} />
                ) : (
                  <span className="tray-quota-value">{head.value}</span>
                )}
                <span className="tray-copy">
                  <strong>
                    <ToolIcon id={vendor.icon} className="tray-mini-icon" />
                    {vendor.name}
                  </strong>
                  <span>{head.kind === "ring" ? `${head.label} · ${head.sub}` : head.label}</span>
                </span>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {hot.length > 1 ? (
        <section className="tray-section">
          <h2>也快满了</h2>
          <div className="tray-list">
            {hot.slice(1, 4).map((row) => (
              <div key={`${row.tool}:${row.id}`} className="tray-row">
                <span className="tray-copy">
                  <strong>{row.title}</strong>
                  <span>
                    <ToolIcon id={vendorOf(row.tool)?.icon ?? row.tool} className="tray-mini-icon" />
                    {specOf(row.tool)?.name ?? row.tool}
                  </span>
                </span>
                <b>{row.percent}%</b>
                <button type="button" className="tray-chip" onClick={() => void trayAction("handoff")}>
                  交接
                </button>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <div className="tray-foot-row">
        <span>{usage.total && usage.total !== "—" ? `本月 ${cnCount(usage.total)} token` : "本机运行"}</span>
        <span className="grow" />
        <button type="button" className="text-button" onClick={() => void trayAction("quit")}>
          退出
        </button>
        <button type="button" className="tray-open" onClick={() => void trayAction("overview")}>
          打开主窗口
        </button>
      </div>
    </div>
  );
}
