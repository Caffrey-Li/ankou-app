import type { ToolSnapshot, UsageDetail } from "./api";
import { presenceLabel, toolSpecs, type Presence, type ToolId } from "./catalog";

export type ToolMark = {
  id: ToolId;
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

export function toolMarks(
  tools: ToolSnapshot[],
  details: UsageDetail[],
  fixture: boolean,
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
    const status = presenceLabel(presence);
    const withheld = row?.tokens === "暂不提供";
    const subtitle = withheld
      ? "用量暂不提供"
      : presence === "authorized"
        ? "已授权用量"
        : presence === "needs_reauth"
          ? "授权已失效"
          : presence === "unauthorized"
            ? "尚未授权"
            : "未发现";
    return {
      id: tool.id,
      letter: tool.letter,
      name: tool.name,
      presence,
      status,
      subtitle,
    };
  });
}

export function SidebarTools({ marks }: { marks: ToolMark[] }) {
  return (
    <ul className="tools">
      {marks.map((mark) => (
        <li key={mark.id}>
          <span className="letter" aria-hidden="true">
            {mark.letter}
          </span>
          <span className="tool-name">{mark.name}</span>
          <span className={`tool-dot dot ${mark.presence}`} role="img" aria-label={mark.status} />
        </li>
      ))}
    </ul>
  );
}

export function OverviewToolRows({
  marks,
  onOpen,
}: {
  marks: ToolMark[];
  onOpen: () => void;
}) {
  return marks.map((mark) => (
    <button key={mark.id} type="button" className="tool-row" onClick={onOpen}>
      <span className="tool-mark" aria-hidden="true">
        {mark.letter}
      </span>
      <span className="tool-copy">
        <strong>{mark.name}</strong>
        <small>{mark.subtitle}</small>
      </span>
      <span className="tool-status">
        <i className={`dot ${mark.presence}`} />
        {mark.status}
      </span>
      <span className="tool-time">—</span>
      <span className="tool-chev" aria-hidden="true">
        ›
      </span>
    </button>
  ));
}
