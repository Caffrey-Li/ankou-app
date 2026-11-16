import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import mark from "../../brand/mark.svg";
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
import { IconBubble, IconDisk, IconRefresh } from "./icons";
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
    void loadSnapshot().then(setSnapshot);
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
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<string>("usage-item", (event) => {
        if (event.payload === "started") setRefreshing(true);
        if (event.payload === "done") {
          setRefreshing(false);
          reload();
        }
      }).then((unlisten) => {
        if (alive) stop = unlisten;
        else unlisten();
      }),
    );
    return () => {
      alive = false;
      stop?.();
      window.removeEventListener("focus", onFocus);
    };
  }, [reload]);

  const panelRef = useRef<HTMLDivElement>(null);
  const items = trayItems(rows, usage, snapshot);
  const shape = `${items.length}|${refreshing}`;
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
  const window5h = usage.cards.find((card) => card.name.includes("5 小时窗口"));
  const balance = usage.cards.find((card) => card.name === "DeepSeek 账户");
  const balanceText = balance && balance.value !== "—" ? balance.value : balance?.note ?? "未保存密钥";

  return (
    <div className="tray-panel" ref={panelRef} role="dialog" aria-label="万象快速面板">
      <div className="tray-head">
        <img src={mark} alt="" width={22} height={22} />
        <strong>万象</strong>
        <span className="grow" />
        <span className="tray-stamp">{refreshing ? "刷新中" : usage.stamp === "—" ? "尚未刷新" : `${usage.stamp} 更新`}</span>
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
      </div>
      <p className="tray-status">
        <i className={items.length > 0 ? "attention" : "ok"} />
        {items.length > 0 ? `${items.length} 件事需要处理` : "一切正常"}
      </p>
      {items.map((item) => (
        <div className="tray-item" key={`${item.title}-${item.detail}`}>
          <span className={item.icon === "alert" ? "tray-badge alert" : "tray-badge"}>
            {item.icon === "disk" ? <IconDisk /> : <IconBubble />}
          </span>
          <span className="tray-copy">
            <strong>{item.title}</strong>
            <span>{item.detail}</span>
          </span>
          <span className="grow" />
          <button type="button" className="text-button" onClick={() => void trayAction(item.to)}>
            {item.action}
          </button>
        </div>
      ))}
      <hr />
      <div className="tray-quota">
        <TrayRing percent={window5h?.percent ?? null} />
        <span className="tray-copy">
          <strong>
            {window5h?.percent != null ? `ChatGPT 5 小时窗口 ${window5h.percent}%` : "ChatGPT 5 小时窗口"}
          </strong>
          <span>{window5h?.percent != null ? window5h.reset ?? window5h.note : window5h?.note ?? "未授权"}</span>
        </span>
      </div>
      <div className="tray-line">
        <span>DeepSeek 余额</span>
        <strong className={balance && balance.value !== "—" ? "" : "muted"}>{balanceText}</strong>
      </div>
      <div className="tray-line">
        <span>Cursor 额度</span>
        <strong className="muted">暂不提供</strong>
      </div>
      <hr />
      <div className="tray-actions">
        <button type="button" className="pill-button primary" onClick={() => void trayAction("overview")}>
          打开万象
        </button>
        <span className="grow" />
        <button type="button" className="text-button" onClick={() => void trayAction("settings")}>
          设置
        </button>
        <button type="button" className="text-button" onClick={() => void trayAction("quit")}>
          退出
        </button>
      </div>
      <p className="tray-foot">退出后停止所有后台检查</p>
    </div>
  );
}

function TrayRing({ percent }: { percent: number | null }) {
  const turn = percent == null ? 0 : Math.max(0, Math.min(100, percent));
  return (
    <svg className="tray-ring" viewBox="0 0 40 40" width="40" height="40" aria-hidden="true">
      <circle cx="20" cy="20" r="16" />
      {percent == null ? null : <circle cx="20" cy="20" r="16" pathLength={100} strokeDasharray={`${turn} 100`} />}
    </svg>
  );
}
