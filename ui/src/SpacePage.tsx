import { useEffect, useState, type ReactNode } from "react";
import { cancelScan, prepareCleanup, runCleanup, scanSpace, type CleanupItem, type SpaceEntry, type SpaceScan } from "./api";
import { inTauri } from "./api";
import { Check } from "./Check";
import { IconCheck, IconLock, IconSpark, IconTrash } from "./icons";
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

type CleanGroup = { id: string; name: string; prefixes: string[]; path: string; note: string };

/** 能清理的端：与 crates/core/src/space.rs 的 cleanup_patterns 一致 */
export const cleanTools = ["deepseek_harness", "claude", "cursor", "codex"] as const;
export type CleanTool = (typeof cleanTools)[number];

const cleanGroups: Record<CleanTool, CleanGroup[]> = {
  deepseek_harness: [
    { id: "cache", name: "运行缓存", prefixes: ["cache/"], path: "~/.dsh/cache", note: "下次启动自动重建" },
    { id: "logs", name: "日志", prefixes: ["logs/"], path: "~/.dsh/logs", note: "不影响对话" },
    { id: "tmp", name: "附件临时文件", prefixes: ["attachments/v1/tmp/"], path: "~/.dsh/attachments/v1/tmp", note: "未发送的临时附件会丢失" },
    { id: "proj", name: "会话项目缓存", prefixes: ["storages/session_projcache/"], path: "storages/session_projcache", note: "可能影响恢复速度" },
  ],
  claude: [
    { id: "cache", name: "网页缓存", prefixes: ["Cache/"], path: "Claude/Cache", note: "下次打开自动重建" },
    { id: "code", name: "脚本编译缓存", prefixes: ["Code Cache/"], path: "Claude/Code Cache", note: "下次打开自动重建，首次启动略慢" },
    {
      id: "gpu",
      name: "图形缓存",
      prefixes: ["GPUCache/", "GraphiteDawnCache/", "DawnGraphiteCache/", "DawnWebGPUCache/"],
      path: "Claude/GPUCache 等",
      note: "自动重建",
    },
  ],
  cursor: [
    { id: "data", name: "编译缓存", prefixes: ["CachedData/", "CachedProfilesData/"], path: "Cursor/CachedData", note: "下次启动自动重建" },
    { id: "vsix", name: "扩展安装包缓存", prefixes: ["CachedExtensionVSIXs/"], path: "Cursor/CachedExtensionVSIXs", note: "已装的扩展不受影响" },
    { id: "web", name: "网页缓存", prefixes: ["Cache/", "Code Cache/", "GPUCache/"], path: "Cursor/Cache 等", note: "自动重建" },
    { id: "logs", name: "日志", prefixes: ["logs/"], path: "Cursor/logs", note: "不影响对话" },
  ],
  codex: [{ id: "cache", name: "缓存", prefixes: ["cache/"], path: "~/.codex/cache", note: "需要时自动重建" }],
};

/** 受保护部分的第一层目录：常见的给一个看得懂的名字；其余显示原名 */
const partNames: Partial<Record<SpaceToolId, Record<string, string>>> = {
  claude: {
    vm_bundles: "电脑任务的运行环境（删除后要重新下载）",
    "claude-code-vm": "Claude Code 虚拟机",
    "claude-code": "Claude Code 程序",
    "claude-code-sessions": "Claude Code 会话",
    "local-agent-mode-sessions": "智能体会话",
    Partitions: "网页存储",
    IndexedDB: "应用数据",
    "Local Storage": "应用数据",
  },
  cursor: {
    snapshots: "智能体改动的检查点",
    User: "对话与设置",
    AgentStores: "智能体数据",
    Partitions: "网页存储",
  },
  codex: {
    archived_sessions: "已归档的会话",
    sessions: "会话",
    "thread_history_1.sqlite": "对话历史",
    generated_images: "生成的图片",
    plugins: "插件",
    skills: "技能",
    ".tmp": "临时文件",
    "logs_2.sqlite": "日志数据库",
    "logs_2.sqlite.old": "旧日志数据库",
  },
  deepseek_harness: {
    storages: "会话与项目数据",
    attachments: "附件",
    memory: "记忆",
  },
};

export function partName(tool: SpaceToolId, name: string) {
  if (!name) return "其他文件";
  return partNames[tool]?.[name] ?? name;
}

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

/** 只统计、不清理的工具的说明：这些端还没有确认可以安全重建的目录。 */
export const statOnlyNote: Record<StatTool, string> = {
  antigravity: "Antigravity 的对话与工作数据全部受保护，安扣只统计占用，不清理",
  gemini: "Gemini 桌面应用的数据全部受保护，安扣只统计占用，不清理",
  workbuddy: "WorkBuddy 的数据全部受保护，安扣只统计占用，不清理",
  grok_bot: "Grok Bot 的数据全部受保护，安扣只统计占用，不清理",
  muse: "Muse 桌面应用的数据全部受保护，安扣只统计占用，不清理",
  codebuddy: "CodeBuddy 命令行的会话与配置全部受保护，安扣只统计占用，不清理",
  cursor_cli: "Cursor 命令行的对话与配置全部受保护，安扣只统计占用，不清理",
};

export function groupEntries(entries: SpaceEntry[], tool: CleanTool = "deepseek_harness") {
  return cleanGroups[tool]
    .map((group) => ({
      ...group,
      entries: entries
        .filter((entry) => group.prefixes.some((prefix) => entry.rel.startsWith(prefix)))
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
/** 正在进行的一轮扫描：总览、清理页、重新扫描按钮共用，不会同时扫两遍 */
let inflight: Promise<SpaceScans> | null = null;

export function scanAllTools(): Promise<SpaceScans> {
  if (inflight) return inflight;
  inflight = Promise.all(spaceTools.map((tool) => scanSpace(tool).catch(() => null)))
    .then((results) => Object.fromEntries(spaceTools.map((tool, index) => [tool, results[index]])) as SpaceScans)
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/** 有没有一轮扫描正在进行 */
export function scanInFlight() {
  return inflight !== null;
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
  localAi,
  initialScans,
  onScanning,
}: {
  /** 总览启动时已经扫过的结果：进清理页直接用，不再从头扫 */
  initialScans?: SpaceScans | null;
  /** 扫描开始、结束时告诉顶栏，按钮显示进行中 */
  onScanning?: (scanning: boolean) => void;
  /** 各厂商以外的本地模型与 AI 缓存：列表里多一行，右侧显示它的清单 */
  localAi?: { bytes: number | null; detail: ReactNode };
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
  const [tool, setTool] = useState<SpaceToolId | "local_ai">(initialTool);
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});
  const [scanning, setScanning] = useState(false);
  useEffect(() => {
    onScanning?.(scanning);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanning]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [confirming, setConfirming] = useState(sheet === "confirm");
  const [result, setResult] = useState<CleanupItem[] | null>(sheet === "result" ? fixtureResult() : null);
  const [releasedBytes, setReleasedBytes] = useState(0);
  const [estimateBytes, setEstimateBytes] = useState(0);
  const scan = fixture ? fixtureScan : live;
  const cursor = fixture ? fixtureCursorScan : cursorScan;
  const codex = fixture ? fixtureCodexScan : codexScan;
  const known = (scan?.protected_bytes ?? 0) + (scan?.cleanable_bytes ?? 0);
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
  const allKnown = (localAi?.bytes ?? 0) + known + cursorKnown + codexKnown + claudeKnown + extraSizes.reduce((sum, bytes) => sum + bytes, 0);
  const overCount = [known, cursorKnown, codexKnown, claudeKnown, ...extraSizes].filter(
    (bytes) => bytes > thresholdGb * GB,
  ).length;
  useEffect(() => {
    // 换了端，之前的勾选不带过去
    setChecked({});
    setOpenGroups({});
  }, [tool]);
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
  const currentVendor = tool === "local_ai" ? undefined : vendors.find((vendor) => (vendor.members as string[]).includes(tool));
  // 当前选中的端能不能清理：能清理的端共用下面这一套勾选、确认、清理
  const cleanTool: CleanTool | null = (cleanTools as readonly string[]).includes(tool) ? (tool as CleanTool) : null;
  const active = cleanTool ? scanOf(cleanTool) : null;
  const entries = active?.entries ?? [];
  const selected = entries.filter((entry) => checked[entry.rel]);
  const selectedBytes = selected.reduce((sum, entry) => sum + entry.bytes, 0);
  const allCleanable = spaceTools.reduce((sum, id) => sum + (scanOf(id)?.cleanable_bytes ?? 0), 0);
  const quit = active?.quit ?? null;
  const cleanupDisabled = windows || selected.length === 0 || busy || scanning || Boolean(quit);
  const dialog = confirming || sheet === "confirm" ? "confirm" : result || sheet === "result" ? "result" : "";

  useEffect(() => {
    if (scanRequest === 0) return;
    void onScan();
  }, [scanRequest]);

  // 打开页面：总览已经扫过就直接用；还没有结果才扫一次（只统计大小）
  useEffect(() => {
    if (fixture || anyScan || !inTauri()) return;
    if (initialScans && scanRequest === 0) {
      applyScans(initialScans);
      onStamp(stampNow());
      return;
    }
    if (scanRequest === 0) void onScan();
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
      if (!applyScans(all)) {
        setNotice("扫描没有完成");
        return;
      }
      setChecked({});
      onStamp(stampNow());
      if (all.deepseek_harness?.cancelled) setNotice("扫描已取消");
    } finally {
      setScanning(false);
    }
  }

  function applyScans(all: SpaceScans) {
    {
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
      if (!harness) return Object.values(all).some(Boolean);
      setLive(harness);
      onSummary?.({
        tool: "DeepSeek Harness",
        known: harness.protected_bytes + harness.cleanable_bytes,
        protectedBytes: harness.protected_bytes,
        cleanable: harness.cleanable_bytes,
      });
      return true;
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
    if (fixture || cleanupDisabled || !cleanTool) return;
    setBusy(true);
    setConfirming(false);
    try {
      const token = await prepareCleanup(cleanTool, selected);
      const done = await runCleanup(cleanTool, token, selected);
      if (done.blocked) {
        setNotice(done.blocked);
        return;
      }
      setEstimateBytes(selected.reduce((sum, entry) => sum + entry.bytes, 0));
      setReleasedBytes(done.total_bytes);
      setResult(done.items);
      setChecked({});
      const after = await scanSpace(cleanTool);
      if (cleanTool === "deepseek_harness") {
        setLive(after);
        onSummary?.({
          tool: "DeepSeek Harness",
          known: after.protected_bytes + after.cleanable_bytes,
          protectedBytes: after.protected_bytes,
          cleanable: after.cleanable_bytes,
        });
      } else if (cleanTool === "claude") setClaudeScan(after);
      else if (cleanTool === "cursor") setCursorScan(after);
      else setCodexScan(after);
    } catch (err) {
      const text = err instanceof Error ? err.message : String(err ?? "");
      setNotice(
        /授权|grant/i.test(text)
          ? `还没有打开「清理」授权：到「工具」页的 ${cleanTool ? toolNames[cleanTool] : ""} 里打开后再清理`
          : text || "清理没有完成",
      );
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
          <button type="button" onClick={() => spaceTools.forEach((id) => void cancelScan(id))}>
            取消
          </button>
        </div>
      )}
      <section className="space-metrics">
        <Metric label="已知占用" bytes={anyScan ? allKnown : null} />
        <Metric label="超过阈值" value={anyScan ? String(overCount) : null} unit="个工具" tone={overCount > 0 ? "warn" : ""} />
        <Metric label="可清理" bytes={anyScan ? allCleanable : null} />
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
              size={row.scanned ? formatBytes(row.total, 1) : scanning ? "扫描中" : "未扫描"}
              over={row.total > thresholdGb * GB}
              bar={row.scanned ? { width: row.total / vendorMax, protectedBytes: row.protectedSum, cleanable: row.cleanSum } : null}
            />
          ))}
          {localAi ? (
            <button
              type="button"
              className={tool === "local_ai" ? "active" : ""}
              aria-pressed={tool === "local_ai"}
              onClick={() => setTool("local_ai")}
            >
              <span className="space-mark sys-mark" aria-hidden="true">
                <IconSpark />
              </span>
              <span className="space-tool-copy">
                <strong>本地模型与 AI 缓存</strong>
                <small>Ollama、Hugging Face 等</small>
              </span>
              <b className={localAi.bytes == null ? "quiet" : ""}>{localAi.bytes == null ? "未统计" : formatBytes(localAi.bytes, 1)}</b>
            </button>
          ) : null}
        </aside>
        <div className="space-detail-col">
        {currentVendor && currentVendor.members.length > 1 ? (
          <div className="range-pills space-ends" role="tablist" aria-label={`${currentVendor.name} 的各端`}>
            {(currentVendor.members as SpaceToolId[]).map((id) => (
              <button key={id} type="button" role="tab" aria-selected={tool === id} onClick={() => setTool(id)}>
                {toolNames[id]}
                <small>{id === "ds_bot" ? "计入 Harness" : scanOf(id) ? formatBytes(sizeOf(id), 1) : scanning ? "扫描中" : "未扫描"}</small>
              </button>
            ))}
          </div>
        ) : null}
        {tool === "local_ai" ? (
          <section className="space-detail" aria-label="本地模型与 AI 缓存">
            <header>
              <span className="space-mark large sys-mark" aria-hidden="true">
                <IconSpark />
              </span>
              <div>
                <strong>本地模型与 AI 缓存</strong>
                <span>各厂商以外的本地模型，以及 AI 应用放在系统缓存和日志目录里的文件</span>
              </div>
            </header>
            {localAi?.detail}
          </section>
        ) : cleanTool ? (
          <section className="space-detail" aria-label={toolNames[cleanTool]}>
            <DetailHead
              id={cleanTool}
              meta={
                active
                  ? `已知占用 ${formatBytes(active.protected_bytes + active.cleanable_bytes, 1)} · 阈值 ${thresholdGb} GB${active.unscanned > 0 ? ` · 另有 ${active.unscanned} 处未扫描` : ""}`
                  : scanning
                    ? "正在扫描…"
                    : "未扫描：在「工具」页打开空间扫描授权后，点右上角重新扫描"
              }
            />
            <StackBar protectedBytes={active?.protected_bytes ?? 0} cleanable={active?.cleanable_bytes ?? 0} known={Boolean(active)} />
            {entries.length === 0 ? (
              <p className="space-empty">{active ? "现在没有可清理的缓存。" : scanning ? "正在扫描，稍等片刻。" : "扫描后列出可清理的内容。"}</p>
            ) : (
              <>
              <h2>可清理</h2>
              <ul className="space-entries">
                {groupEntries(entries, cleanTool).map((group) => {
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
              </>
            )}
            <ProtectedParts id={cleanTool} scan={active} />
            {notice && <p className="space-notice">{notice}</p>}
            {entries.length > 0 ? (
            <footer>
              <span>
                <strong>
                  已选 {selected.length} 项 · 预计释放 {formatBytes(selectedBytes)}
                </strong>
                {quit ? <small>{quit}</small> : <small>建议先退出 {toolNames[cleanTool]}</small>}
              </span>
              <button type="button" disabled={cleanupDisabled} onClick={() => setConfirming(true)}>
                <IconTrash />
                {windows ? "Windows 暂不支持" : "清理所选项…"}
              </button>
            </footer>
            ) : null}
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
            id={tool as StatTool}
            scan={scanOf(tool)}
            scanning={scanning}
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

function StatOnlyDetail({
  id,
  scan,
  scanning,
  thresholdGb,
}: {
  id: StatTool;
  scan: SpaceScan | null;
  scanning: boolean;
  thresholdGb: number;
}) {
  const total = scan ? scan.protected_bytes + scan.cleanable_bytes : 0;
  return (
    <section className="space-detail" aria-label={toolNames[id]}>
      <DetailHead
        id={id}
        meta={
          scan
            ? `已知占用 ${formatBytes(total, 1)} · 阈值 ${thresholdGb} GB${scan.unscanned > 0 ? ` · 另有 ${scan.unscanned} 处未扫描` : ""}`
            : scanning
              ? "正在扫描…"
              : "未扫描：在「工具」页打开空间扫描授权后，点右上角重新扫描"
        }
      />
      <StackBar protectedBytes={scan ? total : 0} cleanable={0} known={Boolean(scan)} />
      <ProtectedParts id={id} scan={scan} />
      <p className="space-footnote">{statOnlyNote[id]}。</p>
    </section>
  );
}

/** 受保护的部分：按第一层目录列出最大的几项，看得出空间花在哪里、为什么不清理 */
function ProtectedParts({ id, scan }: { id: SpaceToolId; scan: SpaceScan | null }) {
  if (!scan || scan.protected_bytes <= 0) return null;
  const parts = (scan.parts ?? []).slice(0, 6);
  const listed = parts.reduce((sum, part) => sum + part.bytes, 0);
  const rest = Math.max(0, scan.protected_bytes - listed);
  return (
    <>
      <h2>
        受保护 · 不会清理<small>{formatBytes(scan.protected_bytes, 1)}</small>
      </h2>
      <ul className="space-protected">
        {parts.length === 0 ? (
          <li>
            <IconLock />
            <span>对话、设置与其余数据</span>
            <b>{formatBytes(scan.protected_bytes, 1)}</b>
          </li>
        ) : (
          parts.map((part) => (
            <li key={part.name}>
              <IconLock />
              <span>{partName(id, part.name)}</span>
              <b>{formatBytes(part.bytes, 1)}</b>
            </li>
          ))
        )}
        {parts.length > 0 && rest >= 1024 * 1024 ? (
          <li>
            <IconLock />
            <span>其余</span>
            <b>{formatBytes(rest, 1)}</b>
          </li>
        ) : null}
      </ul>
    </>
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
