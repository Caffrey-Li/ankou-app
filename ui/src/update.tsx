import { useCallback, useEffect, useRef, useState } from "react";

import {
  checkUpdate,
  inTauri,
  installUpdate,
  previewUpdate,
  trayAction,
  type UpdateInfo,
  type UpdateProgress,
} from "./api";
import "./update.css";

/** 启动时检查一次，之后每 6 小时检查一次。 */
const RECHECK_MS = 6 * 60 * 60 * 1000;

export type Updater = {
  info: UpdateInfo;
  checking: boolean;
  installing: boolean;
  progress: UpdateProgress | null;
  installError: string | null;
  checkedAt: number | null;
  check: () => Promise<void>;
  install: () => Promise<void>;
};

export function useUpdater(): Updater {
  const [info, setInfo] = useState<UpdateInfo>(previewUpdate);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [checkedAt, setCheckedAt] = useState<number | null>(null);
  const busy = useRef(false);

  const check = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setChecking(true);
    try {
      const next = await checkUpdate();
      // 检查失败时保留上次查到的结果（尤其是强制更新），只记下失败原因
      setInfo((current) =>
        next.error && current.latest ? { ...current, error: next.error } : next,
      );
      setCheckedAt(Date.now());
    } catch {
      setInfo((current) => ({ ...current, error: "检查更新失败" }));
    } finally {
      busy.current = false;
      setChecking(false);
    }
  }, []);

  const install = useCallback(async () => {
    setInstalling(true);
    setInstallError(null);
    setProgress({ downloaded: 0, total: null, stage: "download" });
    try {
      await installUpdate();
      // 安装成功后应用会自己重启，走不到这里
    } catch (error) {
      setInstallError(typeof error === "string" ? error : "安装更新失败");
      setInstalling(false);
      setProgress(null);
      // 待安装的更新已被取走，重新检查一次才能再装
      void check();
    }
  }, [check]);

  useEffect(() => {
    if (!inTauri()) return;
    void check();
    const timer = window.setInterval(() => void check(), RECHECK_MS);
    return () => window.clearInterval(timer);
  }, [check]);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<UpdateProgress>("update-progress", (event) => setProgress(event.payload)).then((stop) => {
        if (alive) unlisten = stop;
        else stop();
      }),
    );
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  return { info, checking, installing, progress, installError, checkedAt, check, install };
}

export function progressText(progress: UpdateProgress | null) {
  if (!progress) return "";
  if (progress.stage === "install") return "正在安装，完成后自动重启…";
  const mb = (value: number) => (value / 1024 / 1024).toFixed(1);
  if (progress.total && progress.total > 0) {
    const percent = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
    return `正在下载 ${percent}%（${mb(progress.downloaded)} / ${mb(progress.total)} MB）`;
  }
  return `正在下载 ${mb(progress.downloaded)} MB`;
}

function percentOf(progress: UpdateProgress | null) {
  if (!progress) return 0;
  if (progress.stage === "install") return 100;
  if (!progress.total) return 0;
  return Math.min(100, (progress.downloaded / progress.total) * 100);
}

function Notes({ text }: { text: string | null }) {
  if (!text) return null;
  return (
    <div className="update-notes">
      {text
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line, index) => (
          <p key={index}>{line.replace(/^[-*•]\s*/, "")}</p>
        ))}
    </div>
  );
}

function InstallButton({ updater, label }: { updater: Updater; label: string }) {
  return (
    <button
      type="button"
      className="primary-button"
      disabled={updater.installing}
      onClick={() => void updater.install()}
    >
      {updater.installing ? "正在更新…" : label}
    </button>
  );
}

function ProgressBar({ updater }: { updater: Updater }) {
  if (!updater.installing) return null;
  return (
    <div className="update-progress">
      <div className="update-bar" role="progressbar" aria-valuenow={Math.round(percentOf(updater.progress))}>
        <i style={{ width: `${percentOf(updater.progress)}%` }} />
      </div>
      <span>{progressText(updater.progress)}</span>
    </div>
  );
}

/** 强制更新：挡住整个应用，只能更新或退出。 */
export function UpdateGate({ updater }: { updater: Updater }) {
  const { info } = updater;
  if (!info.mandatory || !info.latest) return null;
  return (
    <div className="update-gate" role="dialog" aria-modal="true" aria-label="需要更新">
      <div className="update-gate-card">
        <h2>需要更新到 {info.latest}</h2>
        <p className="update-lead">
          当前版本 {info.current} 已停止支持{info.minimum ? `（最低要求 ${info.minimum}）` : ""}，更新后才能继续使用。
          你的授权、设置和记录都会保留。
        </p>
        <Notes text={info.notes} />
        <ProgressBar updater={updater} />
        {updater.installError ? <p className="update-error">{updater.installError}</p> : null}
        <div className="update-actions">
          <button
            type="button"
            className="pill-button"
            disabled={updater.installing}
            onClick={() => void trayAction("quit")}
          >
            退出
          </button>
          <InstallButton updater={updater} label="立即更新" />
        </div>
      </div>
    </div>
  );
}

function checkedLabel(at: number | null) {
  if (!at) return "尚未检查";
  const date = new Date(at);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `上次检查 ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 设置页里的「版本与更新」。 */
export function UpdateSection({ updater }: { updater: Updater }) {
  const { info } = updater;
  const has = Boolean(info.latest);
  return (
    <section aria-label="版本与更新">
      <p className="section-label">版本与更新</p>
      <div className="card">
        <div className="setting-row">
          <span className="setting-copy">
            <strong>当前版本 {info.current}</strong>
            <span>
              {updater.checking
                ? "正在检查…"
                : info.error
                  ? info.error
                  : has
                    ? `有新版本 ${info.latest}${info.mandatory ? "（必须更新）" : ""}`
                    : `已是最新版本 · ${checkedLabel(updater.checkedAt)}`}
            </span>
          </span>
          <span className="grow" />
          {has ? (
            <InstallButton updater={updater} label="更新并重启" />
          ) : (
            <button
              type="button"
              className="pill-button"
              disabled={updater.checking}
              onClick={() => void updater.check()}
            >
              检查更新
            </button>
          )}
        </div>
        {has && (info.notes || updater.installing || updater.installError) ? (
          <div className="setting-row update-detail">
            <div className="update-detail-body">
              <Notes text={info.notes} />
              <ProgressBar updater={updater} />
              {updater.installError ? <p className="update-error">{updater.installError}</p> : null}
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
}

/** 侧栏底部：有新版本时显示一个小入口。 */
export function UpdateBadge({ updater, onOpen }: { updater: Updater; onOpen: () => void }) {
  if (!updater.info.latest) return null;
  return (
    <button type="button" className="update-badge" onClick={onOpen}>
      <i aria-hidden="true" />
      新版本 {updater.info.latest}
    </button>
  );
}
