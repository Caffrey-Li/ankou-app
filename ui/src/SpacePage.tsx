import { useEffect, useState } from "react";
import { cancelScan, prepareCleanup, runCleanup, scanSpace, type CleanupItem, type SpaceEntry, type SpaceScan } from "./api";
import { inTauri } from "./api";
import { Check } from "./Check";
import { IconCheck, IconLock, IconTrash } from "./icons";
import { vendors } from "./catalog";
import { ToolIcon } from "./tool-status";
import "./space.css";

const GB = 1_000_000_000;

export const fixtureSpaceSummary = {
  tool: "DeepSeek Harness",
  size: "8.2 GB",
  threshold: "5 GB",
  protected: "6.2 GB",
  cleanable: "2.0 GB",
};

// 夹具数字与 Figma 20:223 一致，只在开发对照时使用。
const fixtureEntries: SpaceEntry[] = [
  { rel: "cache/job", bytes: 1_100_000_000, files: 20, dev: 1, ino: 1, modified: 30, weak: false },
  { rel: "logs/session.log", bytes: 320_000_000, files: 1, dev: 1, ino: 2, modified: 20, weak: false },
  { rel: "attachments/v1/tmp/clip", bytes: 180_000_000, files: 4, dev: 1, ino: 3, modified: 10, weak: false },
  { rel: "storages/session_projcache/blob", bytes: 410_000_000, files: 2, dev: 1, ino: 4, modified: 5, weak: true },
];

const fixtureScan: SpaceScan = {
  protected_bytes: 6_200_000_000,
  cleanable_bytes: 2_010_000_000,
  unscanned: 1,
  cancelled: false,
  quit: null,
  entries: fixtureEntries,
  trash_supported: true,
};

const fixtureCursorScan: SpaceScan = {
  protected_bytes: 3_100_000_000,
  cleanable_bytes: 0,
  unscanned: 0,
  cancelled: false,
  quit: null,
  entries: [],
  trash_supported: true,
};

const fixtureCodexScan: SpaceScan = { ...fixtureCursorScan, protected_bytes: 2_600_000_000 };

const KB = 1_000;
const MB = 1_000_000;

export function formatBytes(bytes: number, digits = 2) {
  if (bytes >= GB) return `${(bytes / GB).toFixed(digits)} GB`;
  if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
  if (bytes >= KB) return `${Math.round(bytes / KB)} KB`;
  return `${bytes} B`;
}

/** 数字和单位分开，指标与工具行用一位小数（Figma 20:316「14.6 GB」）。 */
export function splitSize(bytes: number) {
  const [value, unit = ""] = formatBytes(bytes, 1).split(" ");
  return { value, unit };
}

const groups = [
  { id: "cache", name: "运行缓存", prefix: "cache/", path: "~/.dsh/cache", note: "下次启动自动重建" },
  { id: "logs", name: "日志", prefix: "logs/", path: "~/.dsh/logs", note: "不影响对话" },
  { id: "tmp", name: "附件临时文件", prefix: "attachments/v1/tmp/", path: "~/.dsh/attachments/v1/tmp", note: "未发送的临时附件会丢失" },
  { id: "proj", name: "会话项目缓存", prefix: "storages/session_projcache/", path: "storages/session_projcache", note: "可能影响恢复速度" },
];

export type SpaceToolId =
  | "deepseek_harness"
  | "cursor"
  | "codex"
  | "ds_bot"
  | "claude"
  | "antigravity"
  | "gemini"
  | "workbuddy"
  | "grok_bot"
  | "muse"
  | "codebuddy"
  | "cursor_cli";

/** 只统计、不清理的工具。 */
type StatTool =
  | "cursor"
  | "codex"
  | "claude"
  | "antigravity"
  | "gemini"
  | "workbuddy"
  | "grok_bot"
  | "muse"
  | "codebuddy"
  | "cursor_cli";
/** 后来接入的工具：扫描结果放在一起。 */
const extraTools = ["antigravity", "gemini", "workbuddy", "grok_bot", "muse", "codebuddy", "cursor_cli"] as const;
type ExtraTool = (typeof extraTools)[number];

const toolNames: Record<SpaceToolId, string> = {
  deepseek_harness: "DeepSeek Harness",
  cursor: "Cursor",
  codex: "ChatGPT",
  ds_bot: "DS Bot",
  claude: "Claude",
  antigravity: "Antigravity",
  gemini: "Gemini",
  workbuddy: "WorkBuddy",
  grok_bot: "Grok Bot",
  muse: "Muse",
  codebuddy: "CodeBuddy",
  cursor_cli: "Cursor 命令行",
};

/** 只统计、不清理的工具的说明（决策：Cursor 与 ChatGPT 全部受保护）。 */
export const statOnlyNote: Record<StatTool, string> = {
  cursor: "Cursor 的数据全部受保护，万象只统计占用，不清理",
  codex: "ChatGPT 无可安全重建的证据，只统计不清理",
  claude: "Claude 桌面应用的数据全部受保护，万象只统计占用，不清理",
  antigravity: "Antigravity 的对话与工作数据全部受保护，万象只统计占用，不清理",
  gemini: "Gemini 桌面应用的数据全部受保护，万象只统计占用，不清理",
  workbuddy: "WorkBuddy 的数据全部受保护，万象只统计占用，不清理",
  grok_bot: "Grok Bot 的数据全部受保护，万象只统计占用，不清理",
  muse: "Muse 桌面应用的数据全部受保护，万象只统计占用，不清理",
  codebuddy: "CodeBuddy 命令行的会话与配置全部受保护，万象只统计占用，不清理",
  cursor_cli: "Cursor 命令行的对话与配置全部受保护，万象只统计占用，不清理",
};

export function groupEntries(entries: SpaceEntry[]) {
  return groups
    .map((group) => ({
      ...group,
      entries: entries
        .filter((entry) => entry.rel.startsWith(group.prefix))
        .slice()
        .sort((a, b) => a.modified - b.modified || a.rel.localeCompare(b.rel)),
    }))
    .filter((group) => group.entries.length > 0);
}

export function groupMark(entries: SpaceEntry[], checked: Record<string, boolean>) {
  const marked = entries.filter((entry) => checked[entry.rel]).length;
  return {
    checked: marked === entries.length && marked > 0,
    indeterminate: marked > 0 && marked < entries.length,
  };
}

/** 能统计空间的各端（DS Bot 与 Harness 共用目录，不单独扫）。 */
export const spaceTools = [
  "deepseek_harness",
  "cursor",
  "codex",
  "claude",
  "antigravity",
  "gemini",
  "workbuddy",
  "grok_bot",
  "muse",
  "codebuddy",
  "cursor_cli",
] as const;

export type SpaceScans = Record<(typeof spaceTools)[number], SpaceScan | null>;

/** 一次扫完所有端；没授权或失败的记为 null。只统计大小，不读内容。 */
export async function scanAllTools(): Promise<SpaceScans> {
  const results = await Promise.all(spaceTools.map((tool) => scanSpace(tool).catch(() => null)));
  return Object.fromEntries(spaceTools.map((tool, index) => [tool, results[index]])) as SpaceScans;
}

export function SpacePage({
  fixture,
  windows,
  sheet,
  thresholdGb,
  scanRequest,
  onStamp,
  onSummary,
  onScans,
  initialTool = "deepseek_harness",
}: {
  /** 每次扫描完，把各端的结果交给总览和工具页 */
  onScans?: (scans: SpaceScans) => void;
  onSummary?: (summary: { tool: string; known: number; protectedBytes: number; cleanable: number }) => void;
  initialTool?: SpaceToolId;
  fixture: boolean;
  windows: boolean;
  sheet: "" | "confirm" | "result";
  thresholdGb: number;
  scanRequest: number;
  onStamp: (stamp: string) => void;
}) {
  const [live, setLive] = useState<SpaceScan | null>(null);
  const [cursorScan, setCursorScan] = useState<SpaceScan | null>(null);
  const [codexScan, setCodexScan] = useState<SpaceScan | null>(null);
  const [claudeScan, setClaudeScan] = useState<SpaceScan | null>(null);
  const [extraScans, setExtraScans] = useState<Record<ExtraTool, SpaceScan | null>>({
    antigravity: null,
    gemini: null,
    workbuddy: null,
    grok_bot: null,
    muse: null,
    codebuddy: null,
    cursor_cli: null,
  });
  const [tool, setTool] = useState<SpaceToolId>(initialTool);
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});
  const [scanning, setScanning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [confirming, setConfirming] = useState(sheet === "confirm");
  const [result, setResult] = useState<CleanupItem[] | null>(sheet === "result" ? fixtureResult() : null);
  const [releasedBytes, setReleasedBytes] = useState(0);
  const [estimateBytes, setEstimateBytes] = useState(0);
  const scan = fixture ? fixtureScan : live;
  const cursor = fixture ? fixtureCursorScan : cursorScan;
  const codex = fixture ? fixtureCodexScan : codexScan;
  const entries = scan?.entries ?? [];
  const selected = entries.filter((entry) => checked[entry.rel]);
  const selectedBytes = selected.reduce((sum, entry) => sum + entry.bytes, 0);
  const known = (scan?.protected_bytes ?? 0) + (scan?.cleanable_bytes ?? 0);
  const cleanable = scan?.cleanable_bytes ?? 0;
  const protectedBytes = scan?.protected_bytes ?? 0;
  const cursorKnown = cursor ? cursor.protected_bytes + cursor.cleanable_bytes : 0;
  const codexKnown = codex ? codex.protected_bytes + codex.cleanable_bytes : 0;
  const claude = fixture ? null : claudeScan;
  const claudeKnown = claude ? claude.protected_bytes + claude.cleanable_bytes : 0;
  const extras: Record<ExtraTool, SpaceScan | null> = fixture
    ? { antigravity: null, gemini: null, workbuddy: null, grok_bot: null, muse: null, codebuddy: null, cursor_cli: null }
    : extraScans;
  const extraKnown = (id: ExtraTool) => {
    const found = extras[id];
    return found ? found.protected_bytes + found.cleanable_bytes : 0;
  };
  const extraSizes = extraTools.map(extraKnown);
  const anyScan = Boolean(scan || cursor || codex);
  const allKnown = known + cursorKnown + codexKnown + claudeKnown + extraSizes.reduce((sum, bytes) => sum + bytes, 0);
  const overCount = [known, cursorKnown, codexKnown, claudeKnown, ...extraSizes].filter(
    (bytes) => bytes > thresholdGb * GB,
  ).length;
  const unscannedCount = (scan?.unscanned ?? 0) + (cursor?.unscanned ?? 0) + (codex?.unscanned ?? 0);
  /** 每一端的扫描结果；DS Bot 与 Harness 共用目录，已计入 Harness。 */
  const scanOf = (id: SpaceToolId): SpaceScan | null =>
    id === "deepseek_harness"
      ? scan
      : id === "cursor"
        ? cursor
        : id === "codex"
          ? codex
          : id === "claude"
            ? claude
            : id === "ds_bot"
              ? null
              : extras[id];
  const sizeOf = (id: SpaceToolId) => {
    const found = scanOf(id);
    return found ? found.protected_bytes + found.cleanable_bytes : 0;
  };
  const vendorRows = vendors.map((vendor) => {
    const members = vendor.members as SpaceToolId[];
    const found = members.filter((id) => scanOf(id));
    const protectedSum = found.reduce((sum, id) => sum + (scanOf(id)?.protected_bytes ?? 0), 0);
    const cleanSum = found.reduce((sum, id) => sum + (scanOf(id)?.cleanable_bytes ?? 0), 0);
    return { vendor, members, scanned: found.length > 0, protectedSum, cleanSum, total: protectedSum + cleanSum };
  });
  const vendorMax = Math.max(...vendorRows.map((row) => row.total), 1);
  const currentVendor = vendors.find((vendor) => (vendor.members as string[]).includes(tool));
  const quit = scan?.quit ?? null;
  const cleanupDisabled = windows || selected.length === 0 || busy || scanning || Boolean(quit);
  const dialog = confirming || sheet === "confirm" ? "confirm" : result || sheet === "result" ? "result" : "";

  useEffect(() => {
    if (scanRequest === 0) return;
    void onScan();
  }, [scanRequest]);

  // 打开页面时自动做一次只读扫描（只统计大小），不必再手动点刷新
  useEffect(() => {
    if (!fixture && !anyScan && scanRequest === 0 && inTauri()) void onScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function onScan() {
    if (fixture) {
      onStamp("19:10");
      return;
    }
    setScanning(true);
    setNotice("");
    try {
      const all = await scanAllTools();
      onScans?.(all);
      const harness = all.deepseek_harness;
      const cursor = all.cursor;
      const codex = all.codex;
      const claudeResult = all.claude;
      const { antigravity, gemini, workbuddy, muse, codebuddy } = all;
      const grokBot = all.grok_bot;
      const cursorCli = all.cursor_cli;
      setCursorScan(cursor);
      setCodexScan(codex);
      setClaudeScan(claudeResult);
      setExtraScans({ antigravity, gemini, workbuddy, grok_bot: grokBot, muse, codebuddy, cursor_cli: cursorCli });
      if (!harness) {
        setNotice("扫描没有完成");
        return;
      }
      setLive(harness);
      onSummary?.({
        tool: "DeepSeek Harness",
        known: harness.protected_bytes + harness.cleanable_bytes,
        protectedBytes: harness.protected_bytes,
        cleanable: harness.cleanable_bytes,
      });
      setChecked({});
      onStamp(stampNow());
      if (harness.cancelled) setNotice("扫描已取消");
    } finally {
      setScanning(false);
    }
  }

  function toggleGroup(items: SpaceEntry[], on: boolean) {
    setChecked((current) => {
      const next = { ...current };
      for (const item of items) next[item.rel] = on;
      return next;
    });
  }

  async function onCleanup() {
    if (fixture || cleanupDisabled) return;
    setBusy(true);
    setConfirming(false);
    try {
      const token = await prepareCleanup("deepseek_harness", selected);
      const done = await runCleanup("deepseek_harness", token, selected);
      if (done.blocked) {
        setNotice(done.blocked);
        return;
      }
      setEstimateBytes(selected.reduce((sum, entry) => sum + entry.bytes, 0));
      setReleasedBytes(done.total_bytes);
      setResult(done.items);
      setChecked({});
      const after = await scanSpace("deepseek_harness");
      setLive(after);
      onSummary?.({
        tool: "DeepSeek Harness",
        known: after.protected_bytes + after.cleanable_bytes,
        protectedBytes: after.protected_bytes,
        cleanable: after.cleanable_bytes,
      });
    } catch (err) {
      setNotice(err instanceof Error ? err.message : "清理没有完成");
    } finally {
      setBusy(false);
    }
  }

  const resultItems = result ?? (sheet === "result" ? fixtureResult() : []);
  const released = resultItems
    .filter((item) => item.label.startsWith("已清理"))
    .length;

  return (
    <div className="space-page">
      {scanning && (
        <div className="space-progress">
          <span>正在扫描</span>
          <i />
          <button type="button" onClick={() => void cancelScan("deepseek_harness")}>
            取消
          </button>
        </div>
      )}
      <section className="space-metrics">
        <Metric label="已知占用" bytes={anyScan ? allKnown : null} />
        <Metric label="超过阈值" value={anyScan ? String(overCount) : null} unit="个工具" tone={overCount > 0 ? "warn" : ""} />
        <Metric label="可清理" bytes={scan ? cleanable : null} />
        <Metric label="未扫描" value={anyScan ? String(unscannedCount) : null} unit="处" tone="quiet" />
        <span className="space-metrics-note">重叠目录只统计一次；对话、记忆与账户数据始终受保护</span>
      </section>
      <div className="space-body">
        <aside className="space-tools" aria-label="工具">
          {vendorRows.map((row) => (
            <ToolRow
              key={row.vendor.id}
              id={row.vendor.members[0] as SpaceToolId}
              icon={row.vendor.icon}
              name={row.vendor.name}
              selected={(row.vendor.members as string[]).includes(tool)}
              onSelect={setTool}
              size={row.scanned ? formatBytes(row.total, 1) : "未扫描"}
              over={row.total > thresholdGb * GB}
              bar={row.scanned ? { width: row.total / vendorMax, protectedBytes: row.protectedSum, cleanable: row.cleanSum } : null}
            />
          ))}
          <p>{statOnlyNote.codex}</p>
        </aside>
        <div className="space-detail-col">
        {currentVendor && currentVendor.members.length > 1 ? (
          <div className="range-pills space-ends" role="tablist" aria-label={`${currentVendor.name} 的各端`}>
            {(currentVendor.members as SpaceToolId[]).map((id) => (
              <button key={id} type="button" role="tab" aria-selected={tool === id} onClick={() => setTool(id)}>
                {toolNames[id]}
                <small>{id === "ds_bot" ? "计入 Harness" : scanOf(id) ? formatBytes(sizeOf(id), 1) : "未扫描"}</small>
              </button>
            ))}
          </div>
        ) : null}
        {tool === "deepseek_harness" ? (
          <section className="space-detail" aria-label="DeepSeek Harness">
            <DetailHead
              id="deepseek_harness"
              meta={
                scan
                  ? `已知占用 ${formatBytes(known, 1)} · 阈值 ${thresholdGb} GB · 另有 ${scan.unscanned} 处未扫描`
                  : "尚未扫描"
              }
            />
            <StackBar protectedBytes={protectedBytes} cleanable={cleanable} known={Boolean(scan)} />
            <h2>可清理</h2>
            {entries.length === 0 ? (
              <p className="space-empty">{scan ? "这一项还没有可清理的内容。" : "扫描后列出可清理的内容。"}</p>
            ) : (
              <ul className="space-entries">
                {groupEntries(entries).map((group) => {
                  const mark = groupMark(group.entries, checked);
                  const open = Boolean(openGroups[group.id]);
                  const weak = group.entries.some((entry) => entry.weak);
                  const bytes = group.entries.reduce((sum, entry) => sum + entry.bytes, 0);
                  const many = group.entries.length > 1;
                  return (
                    <li key={group.id}>
                      <div className="space-category">
                        <Check
                          aria-label={group.name}
                          checked={mark.checked}
                          indeterminate={mark.indeterminate}
                          onChange={() => toggleGroup(group.entries, !mark.checked)}
                        />
                        <button
                          type="button"
                          className="space-expand"
                          aria-expanded={many ? open : undefined}
                          disabled={!many}
                          onClick={() => setOpenGroups((current) => ({ ...current, [group.id]: !open }))}
                        >
                          <strong>
                            {group.name}
                            {weak && <em className="tag ochre">证据不足</em>}
                          </strong>
                          <small>
                            {group.path}
                            {many ? ` · ${group.entries.length} 项` : ""} · {group.note}
                          </small>
                        </button>
                        <b>{formatBytes(bytes)}</b>
                      </div>
                      {many && open ? (
                        <ul className="space-children">
                          {group.entries.map((entry) => {
                            const name = entry.rel.split("/").pop() || entry.rel;
                            return (
                              <li key={entry.rel}>
                                <label>
                                  <Check
                                    aria-label={name}
                                    checked={Boolean(checked[entry.rel])}
                                    onChange={(event) =>
                                      setChecked((current) => ({ ...current, [entry.rel]: event.target.checked }))
                                    }
                                  />
                                  <span>
                                    <strong>{name}</strong>
                                    <small>{entry.rel}</small>
                                  </span>
                                  <b>{formatBytes(entry.bytes)}</b>
                                </label>
                              </li>
                            );
                          })}
                        </ul>
                      ) : null}
                    </li>
                  );
                })}
              </ul>
            )}
            <h2>受保护</h2>
            <ul className="space-protected">
              <li>
                <IconLock />
                <span>对话、记忆、账户与其余用户数据</span>
                <b>{scan ? formatBytes(protectedBytes, 1) : "—"}</b>
              </li>
            </ul>
            {notice && <p className="space-notice">{notice}</p>}
            <footer>
              <span>
                <strong>
                  已选 {selected.length} 项 · 预计释放 {formatBytes(selectedBytes)}
                </strong>
                {quit ? <small>{quit}</small> : <small>建议先退出 DeepSeek Harness</small>}
              </span>
              <button type="button" disabled={cleanupDisabled} onClick={() => setConfirming(true)}>
                <IconTrash />
                {windows ? "Windows 暂不支持" : "清理所选项…"}
              </button>
            </footer>
          </section>
        ) : tool === "ds_bot" ? (
          <section className="space-detail" aria-label="DS Bot">
            <DetailHead id="ds_bot" meta="与 DeepSeek Harness 共用 ~/.dsh" />
            <p className="space-explain">
              DS Bot 的数据和 DeepSeek Harness 存在同一个目录里，占用已经计入 Harness，这里不重复统计。清理也在 Harness 里进行。
            </p>
            <div className="space-detail-actions">
              <button type="button" className="space-secondary" onClick={() => setTool("deepseek_harness")}>
                查看 DeepSeek Harness
              </button>
            </div>
          </section>
        ) : (
          <StatOnlyDetail
            id={tool}
            scan={scanOf(tool)}
            thresholdGb={thresholdGb}
          />
        )}
        </div>
      </div>
      {dialog === "confirm" && (
        <dialog className="space-dialog" open>
          <div className="space-dialog-icon ochre">
            <IconTrash />
          </div>
          <h2>
            清理 {fixture || sheet === "confirm" ? 2 : selected.length} 项，约{" "}
            {fixture || sheet === "confirm" ? "800 MB" : formatBytes(selectedBytes)}？
          </h2>
          <p>将移到废纸篓，可以在废纸篓里找回。对话、记忆与账户数据会跳过。</p>
          <div>
            <button type="button" className="primary ochre" disabled={windows || busy} onClick={() => void onCleanup()}>
              {windows ? "Windows 暂不支持" : "清理"}
            </button>
            <button type="button" className="secondary" onClick={() => setConfirming(false)}>
              取消
            </button>
          </div>
        </dialog>
      )}
      {dialog === "result" && (
        <dialog className="space-dialog" open>
          <div className="space-dialog-icon mist">
            <IconCheck />
          </div>
          <h2>清理完成</h2>
          <p>
            实际释放 {fixture || sheet === "result" ? "620 MB" : formatBytes(releasedBytes)}（预计{" "}
            {fixture || sheet === "result" ? "800 MB" : formatBytes(estimateBytes)}）
          </p>
          <ul>
            {(resultItems.length > 0 ? resultItems : fixtureResult()).map((item) => (
              <li key={`${item.name}-${item.label}`}>
                <i className={dotClass(item.label)} />
                <span>{item.name}</span>
                <b>{item.label}</b>
              </li>
            ))}
          </ul>
          <div>
            <button type="button" className="primary mist" onClick={() => setResult(null)}>
              完成
            </button>
            <button type="button" className="secondary" disabled={released === resultItems.length || resultItems.length === 0}>
              重试失败项
            </button>
          </div>
        </dialog>
      )}
    </div>
  );
}

function fixtureResult(): CleanupItem[] {
  return [
    { name: "运行缓存", label: "已清理 620 MB" },
    { name: "日志", label: "已跳过 · 正在使用" },
    { name: "附件临时文件", label: "失败 · 权限不足" },
  ];
}

function dotClass(label: string) {
  if (label.startsWith("已清理")) return "dot ok";
  if (label.includes("权限")) return "dot bad";
  return "dot skip";
}

function stampNow() {
  const now = new Date();
  return `${now.getHours().toString().padStart(2, "0")}:${now.getMinutes().toString().padStart(2, "0")}`;
}

function Metric({
  label,
  bytes,
  value,
  unit,
  tone = "",
}: {
  label: string;
  bytes?: number | null;
  value?: string | null;
  unit?: string;
  tone?: "" | "warn" | "quiet";
}) {
  let shown: { value: string; unit: string } | null = null;
  if (bytes !== undefined && bytes !== null) shown = splitSize(bytes);
  else if (value !== undefined && value !== null) shown = { value, unit: unit ?? "" };
  return (
    <div className="space-metric">
      <span>{label}</span>
      {shown ? (
        <strong className={tone}>
          {shown.value}
          {shown.unit && <small>{shown.unit}</small>}
        </strong>
      ) : (
        <strong className="quiet">—</strong>
      )}
    </div>
  );
}

function DetailHead({ id, meta }: { id: SpaceToolId; meta: string }) {
  return (
    <header>
      <ToolIcon id={id} className="space-mark large" />
      <div>
        <strong>{toolNames[id]}</strong>
        <span>{meta}</span>
      </div>
    </header>
  );
}

function StackBar({ protectedBytes, cleanable, known }: { protectedBytes: number; cleanable: number; known: boolean }) {
  const total = protectedBytes + cleanable;
  const share = total > 0 ? (cleanable / total) * 100 : 0;
  return (
    <>
      <div className="space-stack" aria-hidden="true">
        {known && total > 0 ? (
          <>
            <i style={{ flexGrow: 100 - share }} />
            {cleanable > 0 && <b style={{ flexGrow: share }} />}
          </>
        ) : null}
      </div>
      <p className="space-legend">
        <span>
          <i className="protected" />
          受保护 {known ? formatBytes(protectedBytes, 1) : "—"}
        </span>
        {cleanable > 0 || !known ? (
          <span>
            <i className="clean" />
            可清理 {known ? formatBytes(cleanable, 1) : "—"}
          </span>
        ) : null}
      </p>
    </>
  );
}

function StatOnlyDetail({ id, scan, thresholdGb }: { id: StatTool; scan: SpaceScan | null; thresholdGb: number }) {
  const total = scan ? scan.protected_bytes + scan.cleanable_bytes : 0;
  return (
    <section className="space-detail" aria-label={toolNames[id]}>
      <DetailHead
        id={id}
        meta={
          scan
            ? `已知占用 ${formatBytes(total, 1)} · 阈值 ${thresholdGb} GB${scan.unscanned > 0 ? ` · 另有 ${scan.unscanned} 处未扫描` : ""}`
            : "未扫描：在设置里开启空间扫描授权后，点右上角刷新"
        }
      />
      <StackBar protectedBytes={scan ? total : 0} cleanable={0} known={Boolean(scan)} />
      <h2>可清理</h2>
      <p className="space-empty">{statOnlyNote[id]}。</p>
      <h2>受保护</h2>
      <ul className="space-protected">
        <li>
          <IconLock />
          <span>全部内容</span>
          <b>{scan ? formatBytes(total, 1) : "—"}</b>
        </li>
      </ul>
    </section>
  );
}

function ToolRow({
  id,
  icon,
  name,
  selected,
  onSelect,
  size,
  over,
  bar,
  note,
}: {
  id: SpaceToolId;
  icon?: string;
  name?: string;
  selected: boolean;
  onSelect: (id: SpaceToolId) => void;
  size: string;
  over: boolean;
  bar: { width: number; protectedBytes: number; cleanable: number } | null;
  note?: string;
}) {
  const total = bar ? bar.protectedBytes + bar.cleanable : 0;
  return (
    <button
      type="button"
      className={selected ? "active" : ""}
      aria-pressed={selected}
      onClick={() => onSelect(id)}
    >
      <ToolIcon id={icon ?? id} className="space-mark" />
      <span className="space-tool-copy">
        <strong>
          {name ?? toolNames[id]}
          {over && <em className="tag ochre">超过阈值</em>}
        </strong>
        {note ? (
          <small>{note}</small>
        ) : (
          <i className="space-meter" aria-hidden="true">
            {bar && total > 0 ? (
              <span style={{ width: `${Math.min(1, Math.max(bar.width, 0.04)) * 100}%` }}>
                <b style={{ flexGrow: bar.protectedBytes }} />
                {bar.cleanable > 0 && <b className="clean" style={{ flexGrow: bar.cleanable }} />}
              </span>
            ) : null}
          </i>
        )}
      </span>
      <b className={size === "未扫描" || size === "—" ? "quiet" : ""}>{size}</b>
    </button>
  );
}
