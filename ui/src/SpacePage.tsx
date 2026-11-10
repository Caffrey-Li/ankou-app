import { useEffect, useState } from "react";
import { cancelScan, prepareCleanup, runCleanup, scanSpace, type CleanupItem, type SpaceEntry, type SpaceScan } from "./api";
import { Check } from "./Check";
import { IconCheck, IconLock, IconTrash } from "./icons";
import "./space.css";

const GB = 1_000_000_000;

export const fixtureSpaceSummary = {
  tool: "DeepSeek Harness",
  size: "6.40 GB",
  threshold: "5 GB",
  protected: "5.42 GB",
  cleanable: "0.98 GB",
};

const fixtureEntries: SpaceEntry[] = [
  { rel: "cache/job", bytes: 620_000_000, files: 20, dev: 1, ino: 1, weak: false },
  { rel: "logs/session.log", bytes: 180_000_000, files: 1, dev: 1, ino: 2, weak: false },
  { rel: "attachments/v1/tmp/clip", bytes: 100_000_000, files: 4, dev: 1, ino: 3, weak: false },
  { rel: "storages/session_projcache/blob", bytes: 80_000_000, files: 2, dev: 1, ino: 4, weak: true },
];

const fixtureScan: SpaceScan = {
  protected_bytes: 5_420_000_000,
  cleanable_bytes: 980_000_000,
  unscanned: 1,
  cancelled: false,
  quit: null,
  entries: fixtureEntries,
  trash_supported: true,
};

const copy: Record<string, { name: string; path: string; note: string }> = {
  "cache/job": { name: "运行缓存", path: "~/.dsh/cache", note: "下次启动自动重建" },
  "logs/session.log": { name: "日志", path: "~/.dsh/logs", note: "不影响对话" },
  "attachments/v1/tmp/clip": { name: "附件临时文件", path: "~/.dsh/attachments/v1/tmp", note: "未发送的临时附件会丢弃" },
  "storages/session_projcache/blob": {
    name: "会话项目缓存",
    path: "storages/session_projcache",
    note: "可能影响恢复速度",
  },
};

function formatBytes(bytes: number) {
  if (bytes >= GB) return `${(bytes / GB).toFixed(2)} GB`;
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  return `${bytes} B`;
}

function entryCopy(entry: SpaceEntry) {
  if (copy[entry.rel]) return copy[entry.rel];
  if (entry.rel.startsWith("cache/")) return { name: "运行缓存", path: entry.rel, note: "下次启动自动重建" };
  if (entry.rel.startsWith("logs/")) return { name: "日志", path: entry.rel, note: "不影响对话" };
  if (entry.rel.startsWith("attachments/v1/tmp/")) return { name: "附件临时文件", path: entry.rel, note: "未发送的临时附件会丢弃" };
  if (entry.rel.startsWith("storages/session_projcache/")) {
    return { name: "会话项目缓存", path: entry.rel, note: "可能影响恢复速度" };
  }
  return { name: entry.rel.split("/").pop() || entry.rel, path: entry.rel, note: "" };
}

export function SpacePage({
  fixture,
  windows,
  sheet,
  thresholdGb,
  scanRequest,
  onStamp,
}: {
  fixture: boolean;
  windows: boolean;
  sheet: "" | "confirm" | "result";
  thresholdGb: number;
  scanRequest: number;
  onStamp: (stamp: string) => void;
}) {
  const [live, setLive] = useState<SpaceScan | null>(null);
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [scanning, setScanning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [confirming, setConfirming] = useState(sheet === "confirm");
  const [result, setResult] = useState<CleanupItem[] | null>(sheet === "result" ? fixtureResult() : null);
  const [releasedBytes, setReleasedBytes] = useState(0);
  const [estimateBytes, setEstimateBytes] = useState(0);
  const scan = fixture ? fixtureScan : live;
  const entries = scan?.entries ?? [];
  const selected = entries.filter((entry) => checked[entry.rel]);
  const selectedBytes = selected.reduce((sum, entry) => sum + entry.bytes, 0);
  const known = (scan?.protected_bytes ?? 0) + (scan?.cleanable_bytes ?? 0);
  const cleanable = scan?.cleanable_bytes ?? 0;
  const protectedBytes = scan?.protected_bytes ?? 0;
  const over = known > thresholdGb * GB;
  const quit = scan?.quit ?? null;
  const cleanupDisabled = windows || selected.length === 0 || busy || scanning || Boolean(quit);
  const cleanShare = known > 0 ? Math.round((cleanable / known) * 100) : 0;
  const dialog = confirming || sheet === "confirm" ? "confirm" : result || sheet === "result" ? "result" : "";

  useEffect(() => {
    if (scanRequest === 0) return;
    void onScan();
  }, [scanRequest]);

  async function onScan() {
    if (fixture) {
      onStamp("19:10");
      return;
    }
    setScanning(true);
    setNotice("");
    try {
      const next = await scanSpace("deepseek_harness");
      setLive(next);
      setChecked({});
      onStamp(stampNow());
      if (next.cancelled) setNotice("扫描已取消");
    } catch (err) {
      setNotice(err instanceof Error ? err.message : "扫描没有完成");
    } finally {
      setScanning(false);
    }
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
      setLive(await scanSpace("deepseek_harness"));
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
        <Metric label="已知占用" value={scan ? formatBytes(known) : "尚未扫描"} warn={false} />
        <Metric label="超过阈值" value={scan ? (over ? "1 个工具" : "0 个工具") : "—"} warn={over} />
        <Metric label="可清理" value={scan ? formatBytes(cleanable) : "—"} warn={false} />
        <Metric label="未扫描" value={scan ? `${scan.unscanned} 处` : "—"} warn={false} />
        <span className="space-metrics-note">重复目录只统计一次；对话、记忆与账户数据始终受保护</span>
      </section>
      <div className="space-body">
        <aside className="space-tools">
          <ToolRow mark="H" name="DeepSeek Harness" size={scan ? formatBytes(known) : "尚未扫描"} active over={over} share={cleanShare} meter={Boolean(scan)} />
          <ToolRow mark="C" name="Cursor" size="未扫描" active={false} over={false} share={0} meter={false} />
          <ToolRow mark="Cx" name="Codex" size="只统计" active={false} over={false} share={0} meter={false} />
          <ToolRow mark="B" name="DS Bot" size="—" active={false} over={false} share={0} meter={false} note="已计入 Harness（共用 ~/.dsh）" />
          <p>Codex 无可安全清理的证据，只统计不清理</p>
        </aside>
        <section className="space-detail">
          <header>
            <span className="space-mark">H</span>
            <div>
              <strong>DeepSeek Harness</strong>
              <span>
                {scan
                  ? `已知占用 ${formatBytes(known)} · 阈值 ${thresholdGb} GB · 另有 ${scan.unscanned} 处未扫描`
                  : "尚未扫描"}
              </span>
            </div>
          </header>
          <div className="space-stack" aria-hidden="true">
            <i style={{ width: `${100 - cleanShare}%` }} />
            <b style={{ width: `${cleanShare}%` }} />
          </div>
          <p className="space-legend">
            <span>受保护 {scan ? formatBytes(protectedBytes) : "—"}</span>
            <span>可清理 {scan ? formatBytes(cleanable) : "—"}</span>
          </p>
          <h2>可清理</h2>
          {entries.length === 0 ? (
            <p className="space-empty">这一项还没有可清理的内容。</p>
          ) : (
            <ul className="space-entries">
              {entries.map((entry) => {
                const item = entryCopy(entry);
                return (
                  <li key={entry.rel}>
                    <label>
                      <Check
                        checked={Boolean(checked[entry.rel])}
                        onChange={(event) => setChecked((current) => ({ ...current, [entry.rel]: event.target.checked }))}
                      />
                      <span>
                        <strong>
                          {item.name}
                          {entry.weak && <em>证据不足</em>}
                        </strong>
                        <small>
                          {item.path} · {item.note}
                        </small>
                      </span>
                      <b>{formatBytes(entry.bytes)}</b>
                    </label>
                  </li>
                );
              })}
            </ul>
          )}
          <h2>受保护</h2>
          <p className="space-protected-row">
            <IconLock />
            <span>其余用户数据</span>
            <b>{scan ? formatBytes(protectedBytes) : "尚未扫描"}</b>
          </p>
          {notice && <p className="space-notice">{notice}</p>}
          <footer>
            <span>
              已选 {selected.length} 项 · 预计释放 {formatBytes(selectedBytes)}
              {quit && <small>{quit}</small>}
            </span>
            <button type="button" disabled={cleanupDisabled} onClick={() => setConfirming(true)}>
              <IconTrash />
              {windows ? "Windows 暂不支持" : "清理所选项"}
            </button>
          </footer>
        </section>
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
            实际释放 {fixture || sheet === "result" ? "800 MB" : formatBytes(releasedBytes)}（预计{" "}
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

function Metric({ label, value, warn }: { label: string; value: string; warn: boolean }) {
  return (
    <div>
      <span>{label}</span>
      <strong className={warn ? "warn" : ""}>{value}</strong>
    </div>
  );
}

function ToolRow({
  mark,
  name,
  size,
  active,
  over,
  share,
  meter,
  note,
}: {
  mark: string;
  name: string;
  size: string;
  active: boolean;
  over: boolean;
  share: number;
  meter: boolean;
  note?: string;
}) {
  return (
    <button type="button" className={active ? "active" : ""}>
      <span className="space-mark">{mark}</span>
      <span className="space-tool-copy">
        <strong>
          {name}
          {over && <em>超过阈值</em>}
        </strong>
        <i>
          {meter ? (
            <>
              <b style={{ width: `${100 - share}%` }} />
              <b className="clean" style={{ width: `${share}%` }} />
            </>
          ) : null}
        </i>
        {note && <small>{note}</small>}
      </span>
      <b>{size}</b>
    </button>
  );
}
