import { useEffect, useState } from "react";

import { loadToolIcons, type ToolSnapshot, type UsageDetail } from "./api";
import { presenceLabel, toolSpecs, type Presence, type ToolId } from "./catalog";

const iconSrc = new Map<string, string | null>();
let iconLoad: Promise<void> | null = null;

function ensureToolIcons() {
  if (!iconLoad) {
    iconLoad = loadToolIcons()
      .then((rows) => {
        for (const row of rows) {
          iconSrc.set(row.tool, row.png_base64 ? `data:image/png;base64,${row.png_base64}` : null);
        }
      })
      .catch(() => {
        iconLoad = null;
      });
  }
  return iconLoad;
}

export function ToolIcon({ id, className }: { id: string; className?: string }) {
  const [, setTick] = useState(0);
  useEffect(() => {
    let alive = true;
    void ensureToolIcons()?.then(() => {
      if (alive) setTick((value) => value + 1);
    });
    return () => {
      alive = false;
    };
  }, []);
  const spec = toolSpecs.find((tool) => tool.id === id);
  const src = iconSrc.get(id);
  if (src) return <img className={className ? `${className} tool-app-icon` : "tool-app-icon"} src={src} alt="" aria-hidden="true" />;
  return (
    <span className={className} aria-hidden="true">
      {spec?.letter ?? ""}
    </span>
  );
}

export type ToolMark = {
  id: ToolId;
  /** 侧栏状态点：正常 = 雾蓝，需要处理 = 赭色，未连接或过时 = 灰（22:57「数据」三色点）。 */
  tone: "ok" | "attention" | "quiet";
  letter: string;
  name: string;
  presence: Presence;
  status: string;
  subtitle: string;
};

function detailOf(details: UsageDetail[], name: string) {
  return details.find((row) => row.name === name);
}

function says(row: UsageDetail | undefined, text: string) {
  return row?.tokens === text || row?.source === text;
}

export function toolTone(presence: Presence, attention: boolean, stale: boolean): ToolMark["tone"] {
  if (presence === "needs_reauth") return "attention";
  if (presence !== "authorized") return "quiet";
  if (attention) return "attention";
  if (stale) return "quiet";
  return "ok";
}

export function toolMarks(
  tools: ToolSnapshot[],
  details: UsageDetail[],
  fixture: boolean,
  attention: ReadonlySet<string> = new Set(),
  stale: ReadonlySet<string> = new Set(),
  grants: { tool: string; capability: string; status: string }[] = [],
): ToolMark[] {
  return toolSpecs.map((tool) => {
    const live = tools.find((item) => item.tool === tool.id)?.presence ?? "missing";
    const row = detailOf(details, tool.name);
    let presence = live;
    if (fixture) {
      if (!row) presence = "missing";
      else if (says(row, "需要重新授权")) presence = "needs_reauth";
      else if (says(row, "未授权")) presence = "unauthorized";
      else presence = "authorized";
    }
    const tone = toolTone(presence, attention.has(tool.id), stale.has(tool.id));
    const status =
      presence !== "authorized"
        ? presenceLabel(presence)
        : tone === "attention"
          ? "需要处理"
          : tone === "quiet"
            ? "数据过时"
            : "正常";
    const withheld = row?.tokens === "暂不提供";
    const caps = grantedLabels(tool.id, grants);
    const granted = caps.length > 0 ? `已授权${caps.join("、")}` : "";
    const subtitle = withheld
      ? [granted, "用量暂不提供"].filter(Boolean).join(" · ")
      : presence === "authorized"
        ? granted || "已授权"
        : presence === "needs_reauth"
          ? "授权已失效"
          : presence === "unauthorized"
            ? "尚未授权"
            : "未发现";
    return {
      id: tool.id,
      tone,
      letter: tool.letter,
      name: tool.name,
      presence,
      status,
      subtitle,
    };
  });
}

const capLabels: [string, string][] = [
  ["usage", "用量"],
  ["conversation", "对话"],
  ["space_scan", "空间"],
  ["account_link", "余额"],
];

function grantedLabels(tool: string, grants: { tool: string; capability: string; status: string }[]) {
  const own = grants.filter((grant) => grant.tool === tool && grant.status === "active");
  return capLabels.filter(([cap]) => own.some((grant) => grant.capability === cap)).map(([, label]) => label);
}

export function SidebarTools({ marks, onOpen }: { marks: ToolMark[]; onOpen?: (id: ToolId) => void }) {
  return (
    <ul className="tools">
      {marks.map((mark) => (
        <li key={mark.id}>
          <button type="button" className="tool-link" onClick={() => onOpen?.(mark.id)}>
            <ToolIcon id={mark.id} className="letter" />
            <span className="tool-name">{mark.name}</span>
            <span className={`tool-dot ${mark.tone}`} role="img" aria-label={mark.status} />
          </button>
        </li>
      ))}
    </ul>
  );
}

export function OverviewToolRows({
  marks,
  onOpen,
  stamp = "—",
}: {
  marks: ToolMark[];
  onOpen: (id: ToolId) => void;
  stamp?: string;
}) {
  return marks.map((mark) => (
    <button key={mark.id} type="button" className="tool-row" onClick={() => onOpen(mark.id)}>
      <ToolIcon id={mark.id} className="tool-mark" />
      <span className="tool-copy">
        <strong>{mark.name}</strong>
        <small>{mark.subtitle}</small>
      </span>
      <span className="tool-status">
        <i className={`dot ${mark.presence} ${mark.tone}`} />
        {mark.status}
      </span>
      <span className="tool-time">{mark.presence === "authorized" ? stamp : "—"}</span>
      <span className="tool-chev" aria-hidden="true">
        ›
      </span>
    </button>
  ));
}
