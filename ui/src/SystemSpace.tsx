import { useState } from "react";
import {
  openPrivacySettings,
  prepareSystemCleanup,
  runSystemCleanup,
  type CleanupItem,
  type SysItem,
  type SysPick,
  type SysScan,
  type SysSide,
} from "./api";
import { Check } from "./Check";
import { IconCheck, IconChevron, IconTrash } from "./icons";
import { formatBytes, splitSize } from "./SpacePage";
import "./sysdata.css";

/** 选择：整项（true）或其中几个子项 */
export type SysSelection = Record<string, true | string[]>;

export function sideItems(scan: SysScan | null, side: SysSide) {
  return (scan?.items ?? []).filter((item) => item.side === side);
}

export function sideTotal(scan: SysScan | null, side: SysSide) {
  return sideItems(scan, side)
    .filter((item) => item.state === "ok" || item.state === "denied")
    .reduce((sum, item) => sum + item.bytes, 0);
}

/** 值得占一行：有占用、读不到（要提示授权）、或有快照可说明 */
export function present(item: SysItem) {
  if (item.state === "missing") return false;
  if (item.state === "denied") return true;
  if (item.state === "info") return item.snapshots == null || item.snapshots > 0;
  return item.bytes > 0;
}

export function pickable(item: SysItem) {
  return item.action !== "manual" && item.state !== "missing" && item.state !== "info" && item.bytes > 0;
}

export function selectedBytes(items: SysItem[], selection: SysSelection) {
  let total = 0;
  for (const item of items) {
    const pick = selection[item.id];
    if (pick === true) total += item.bytes;
    else if (pick) total += item.children.filter((child) => pick.includes(child.name)).reduce((sum, child) => sum + child.bytes, 0);
  }
  return total;
}

export function toPicks(selection: SysSelection): SysPick[] {
  return Object.entries(selection).map(([id, pick]) => (pick === true ? { id } : { id, children: pick }));
}

const actionTag: Record<SysItem["action"], { text: string; tone: string }> = {
  safe: { text: "可清理", tone: "ok" },
  review: { text: "需确认", tone: "warn" },
  manual: { text: "手动处理", tone: "quiet" },
};

function sizeText(item: SysItem) {
  if (item.state === "info") return item.snapshots != null ? `${item.snapshots} 个快照` : "—";
  if (item.state === "missing") return "未发现";
  return formatBytes(item.bytes, 1);
}

/** 还没开启：先说明会做什么，再让用户开启。 */
export function SysEnable({ onEnable, compact = false }: { onEnable: () => void; compact?: boolean }) {
  return (
    <section className={compact ? "sys-enable compact" : "sys-enable"} aria-label="开启统计">
      <h2>统计本机其他占用</h2>
      <p>
        安扣会统计系统缓存、日志、开发工具、设备备份和本地模型等位置的大小，并分成「AI 工具」和「系统」两部分。
        只计算大小，不打开文件内容；清理时只移到废纸篓，需要命令或管理员权限的位置只给出处理方法。
      </p>
      <button type="button" className="sys-primary" onClick={onEnable}>
        开启统计
      </button>
    </section>
  );
}

/** 一边（AI 或系统）的清单：按分组列出，可勾选、展开子项，底部清理。 */
export function SysList({
  scan,
  side,
  scanning,
  windows,
  onCleaned,
}: {
  scan: SysScan | null;
  side: SysSide;
  scanning: boolean;
  windows: boolean;
  onCleaned: () => void;
}) {
  const [selection, setSelection] = useState<SysSelection>({});
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [result, setResult] = useState<{ items: CleanupItem[]; bytes: number } | null>(null);
  const items = sideItems(scan, side);
  const shown = items.filter(present);
  const missing = items.filter((item) => !present(item));
  const groups = [...new Set(shown.map((item) => item.group))];
  const picked = items.filter((item) => selection[item.id]);
  const bytes = selectedBytes(items, selection);
  const reviews = picked.filter((item) => item.action === "review");

  function toggleItem(item: SysItem, on: boolean) {
    setSelection((current) => {
      const next = { ...current };
      if (on) next[item.id] = true;
      else delete next[item.id];
      return next;
    });
  }

  function toggleChild(item: SysItem, name: string, on: boolean) {
    setSelection((current) => {
      const next = { ...current };
      const now = next[item.id];
      let list = now === true ? item.children.map((child) => child.name) : now ? [...now] : [];
      list = on ? [...new Set([...list, name])] : list.filter((entry) => entry !== name);
      const whole = item.count > 0 && list.length === item.count;
      if (whole) next[item.id] = true;
      else if (list.length) next[item.id] = list;
      else delete next[item.id];
      return next;
    });
  }

  async function onCleanup() {
    setConfirming(false);
    setBusy(true);
    setNotice("");
    try {
      const picks = toPicks(selection);
      const token = await prepareSystemCleanup(picks);
      const done = await runSystemCleanup(token, picks);
      setResult({ items: done.items, bytes: done.total_bytes });
      setSelection({});
      onCleaned();
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err || "清理没有完成"));
    } finally {
      setBusy(false);
    }
  }

  if (!scan) {
    return <p className="sys-empty">{scanning ? "正在统计…" : "还没有统计结果，点右上角重新扫描"}</p>;
  }

  return (
    <section className="sys-list" aria-label={side === "ai" ? "本地模型与 AI 缓存" : "系统"}>
      <div className="sys-scroll">
        {groups.map((group) => {
          const rows = shown.filter((item) => item.group === group);
          const subtotal = rows.filter((item) => item.state !== "info").reduce((sum, item) => sum + item.bytes, 0);
          return (
            <div className="sys-group" key={group}>
              <div className="sys-group-head">
                <h3>{group}</h3>
                <span>{subtotal > 0 ? formatBytes(subtotal, 1) : ""}</span>
              </div>
              {rows.map((item) => {
                const pick = selection[item.id];
                const can = pickable(item) && !windows;
                const many = can && item.children.length > 1;
                const expanded = Boolean(open[item.id]);
                const tag = actionTag[item.action];
                return (
                  <div className={`sys-row${item.action === "manual" ? " manual" : ""}`} key={item.id}>
                    <div className="sys-row-main">
                      {can ? (
                        <Check
                          aria-label={item.title}
                          checked={pick === true}
                          indeterminate={Array.isArray(pick)}
                          onChange={(event) => toggleItem(item, event.target.checked)}
                        />
                      ) : (
                        <span className="sys-check-gap" aria-hidden="true" />
                      )}
                      <div className="sys-copy">
                        <strong>
                          {item.title}
                          <em className={`sys-tag ${tag.tone}`}>{tag.text}</em>
                        </strong>
                        <span>{item.note}</span>
                        {item.paths.length ? <small>{item.paths.join(" · ")}</small> : null}
                        {item.state === "denied" ? (
                          <small className="sys-denied">
                            部分位置读不到，需要「完全磁盘访问权限」
                            <button type="button" className="text-button" onClick={() => void openPrivacySettings().catch(() => undefined)}>
                              去设置
                            </button>
                          </small>
                        ) : null}
                      </div>
                      <b className={item.state === "info" || item.state === "missing" ? "quiet" : ""}>{sizeText(item)}</b>
                      {many ? (
                        <button
                          type="button"
                          className={expanded ? "sys-expand open" : "sys-expand"}
                          aria-expanded={expanded}
                          aria-label={`展开 ${item.title}`}
                          onClick={() => setOpen((current) => ({ ...current, [item.id]: !expanded }))}
                        >
                          <IconChevron />
                        </button>
                      ) : (
                        <span className="sys-expand-gap" aria-hidden="true" />
                      )}
                    </div>
                    {many && expanded ? (
                      <ul className="sys-children">
                        {item.children.map((child) => {
                          const on = pick === true || (Array.isArray(pick) && pick.includes(child.name));
                          return (
                            <li key={child.name}>
                              <label>
                                <Check
                                  aria-label={child.name}
                                  checked={on}
                                  onChange={(event) => toggleChild(item, child.name, event.target.checked)}
                                />
                                <span>{child.name}</span>
                                <b>{formatBytes(child.bytes, 1)}</b>
                              </label>
                            </li>
                          );
                        })}
                        {item.count > item.children.length ? (
                          <li className="sys-more">另有 {item.count - item.children.length} 项较小的内容，勾选整项时一起清理</li>
                        ) : null}
                      </ul>
                    ) : null}
                  </div>
                );
              })}
            </div>
          );
        })}
        {missing.length ? <p className="sys-missing">本机没有或没有占用：{missing.map((item) => item.title).join("、")}</p> : null}
      </div>
      {notice ? <p className="sys-notice">{notice}</p> : null}
      <footer className="sys-foot">
        <span>
          <strong>
            已选 {picked.length} 项 · 预计释放 {formatBytes(bytes, 1)}
          </strong>
          <small>移到废纸篓，确认不需要后再清倒；正在运行的应用的缓存会跳过</small>
        </span>
        <button type="button" className="sys-primary" disabled={windows || busy || scanning || picked.length === 0} onClick={() => setConfirming(true)}>
          <IconTrash />
          {windows ? "Windows 暂不支持" : "清理所选项…"}
        </button>
      </footer>
      {confirming ? (
        <dialog className="space-dialog" open>
          <div className="space-dialog-icon ochre">
            <IconTrash />
          </div>
          <h2>
            清理 {picked.length} 项，约 {formatBytes(bytes, 1)}？
          </h2>
          <p>将移到废纸篓，可以在废纸篓里找回。</p>
          {reviews.length ? (
            <ul className="sys-review">
              {reviews.map((item) => (
                <li key={item.id}>
                  <strong>{item.title}</strong>
                  <span>{item.note}</span>
                </li>
              ))}
            </ul>
          ) : null}
          <div>
            <button type="button" className="primary ochre" disabled={busy} onClick={() => void onCleanup()}>
              清理
            </button>
            <button type="button" className="secondary" onClick={() => setConfirming(false)}>
              取消
            </button>
          </div>
        </dialog>
      ) : null}
      {result ? (
        <dialog className="space-dialog" open>
          <div className="space-dialog-icon mist">
            <IconCheck />
          </div>
          <h2>清理完成</h2>
          <p>已移到废纸篓 {formatBytes(result.bytes, 1)}</p>
          <ul>
            {result.items.map((item) => (
              <li key={`${item.name}-${item.label}`}>
                <i className={item.label.startsWith("已清理") ? "dot ok" : item.label.includes("权限") ? "dot bad" : "dot skip"} />
                <span>{item.name}</span>
                <b>{item.label}</b>
              </li>
            ))}
          </ul>
          <div>
            <button type="button" className="primary mist" onClick={() => setResult(null)}>
              完成
            </button>
          </div>
        </dialog>
      ) : null}
    </section>
  );
}

/** 「系统」页：上面一条总览（磁盘、系统占用、可清理、需确认），下面是清单。 */
export function SystemPanel({
  scan,
  enabled,
  scanning,
  windows,
  aiBytes,
  onEnable,
  onCleaned,
}: {
  scan: SysScan | null;
  enabled: boolean;
  scanning: boolean;
  windows: boolean;
  /** AI 工具一侧的总占用（各厂商 + 本地模型），用来画磁盘条 */
  aiBytes: number;
  onEnable: () => void;
  onCleaned: () => void;
}) {
  if (!enabled) {
    return (
      <div className="sys-page">
        <SysEnable onEnable={onEnable} />
      </div>
    );
  }
  const items = sideItems(scan, "system");
  const system = sideTotal(scan, "system");
  const safe = items.filter((item) => item.action === "safe").reduce((sum, item) => sum + item.bytes, 0);
  const review = items.filter((item) => item.action === "review").reduce((sum, item) => sum + item.bytes, 0);
  const manual = items.filter((item) => item.action === "manual" && item.state !== "info").reduce((sum, item) => sum + item.bytes, 0);
  const disk = scan?.disk ?? null;
  const used = disk ? disk.total - disk.available : 0;
  const other = Math.max(0, used - aiBytes - system);
  return (
    <div className="sys-page">
      <section className="sys-summary" aria-label="磁盘">
        <div className="sys-disk">
          <div className="sys-disk-head">
            <span>磁盘</span>
            <strong>{disk ? `可用 ${formatBytes(disk.available, 1)}` : "—"}</strong>
            <small>{disk ? `共 ${formatBytes(disk.total, 0)}` : ""}</small>
          </div>
          <div className="sys-disk-bar" aria-hidden="true">
            {disk && disk.total > 0 ? (
              <>
                <i className="ai" style={{ flexGrow: aiBytes }} />
                <i className="system" style={{ flexGrow: system }} />
                <i className="other" style={{ flexGrow: other }} />
                <i className="free" style={{ flexGrow: disk.available }} />
              </>
            ) : null}
          </div>
          <p className="sys-legend">
            <span><i className="ai" />AI 工具 {formatBytes(aiBytes, 1)}</span>
            <span><i className="system" />系统 {formatBytes(system, 1)}</span>
            <span><i className="other" />其他文件 {disk ? formatBytes(other, 1) : "—"}</span>
            <span><i className="free" />可用</span>
          </p>
        </div>
        <Cell label="可直接清理" bytes={scan ? safe : null} tone="ok" />
        <Cell label="清理前需确认" bytes={scan ? review : null} tone="warn" />
        <Cell label="只能手动处理" bytes={scan ? manual : null} tone="quiet" />
      </section>
      <SysList scan={scan} side="system" scanning={scanning} windows={windows} onCleaned={onCleaned} />
    </div>
  );
}

function Cell({ label, bytes, tone }: { label: string; bytes: number | null; tone: string }) {
  const shown = bytes == null ? null : splitSize(bytes);
  return (
    <div className={`sys-cell ${tone}`}>
      <span>{label}</span>
      <strong>
        {shown ? shown.value : "—"}
        {shown?.unit ? <small>{shown.unit}</small> : null}
      </strong>
    </div>
  );
}
