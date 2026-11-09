import { useState } from "react";
import type { Settings, Snapshot } from "./api";
import { clearHistory, deleteKey, revokeGrant, saveKey, saveSettings } from "./api";
import { presenceLabel, toolSpecs, type ToolId } from "./catalog";
import "./settings.css";

export function SettingsPage({
  snapshot,
  onChange,
}: {
  snapshot: Snapshot;
  onChange: (update: (current: Snapshot) => Snapshot) => void;
}) {
  const [openTool, setOpenTool] = useState<ToolId | "">("");
  const [keyOpen, setKeyOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);

  function update(patch: Partial<Settings>) {
    let previous: Settings | null = null;
    let next: Settings | null = null;
    onChange((current) => {
      previous = current.settings;
      next = { ...current.settings, ...patch };
      return { ...current, settings: next };
    });
    if (!next) return;
    const saved = next;
    const prior = previous;
    void saveSettings(saved).catch(() => {
      onChange((current) => (current.settings === saved && prior ? { ...current, settings: prior } : current));
    });
  }

  function toggle(key: keyof Settings) {
    const current = snapshot.settings[key];
    if (typeof current !== "boolean") return;
    update({ [key]: !current });
  }

  return (
    <div className="settings-layout">
      <div className="settings-column">
        <section>
          <p className="section-label">提醒</p>
          <div className="card">
            <NumberRow
              title="上下文提醒阈值"
              detail="比例未知时仍可手动交接"
              value={snapshot.settings.context_threshold_percent}
              suffix="%"
              onChange={(value) => update({ context_threshold_percent: value })}
            />
            <NumberRow
              title="空间提醒阈值"
              detail="统一适用于每个工具"
              value={snapshot.settings.space_threshold_gb}
              suffix="GB"
              onChange={(value) => update({ space_threshold_gb: value })}
            />
          </div>
        </section>
        <section>
          <p className="section-label">刷新与历史</p>
          <div className="card">
            <SwitchRow
              title="启动时刷新"
              checked={snapshot.settings.refresh_on_launch}
              onClick={() => toggle("refresh_on_launch")}
            />
            <SwitchRow
              title="运行中定期刷新"
              detail="间隔按各工具限制确定"
              checked={snapshot.settings.periodic_refresh}
              onClick={() => toggle("periodic_refresh")}
            />
            <div className="setting-row">
              <span className="setting-copy">
                <strong>用量历史保留</strong>
                <span>只保存在这台电脑</span>
              </span>
              <span className="grow" />
              <select
                className="select-field"
                aria-label="用量历史保留"
                value={snapshot.settings.retention_months}
                onChange={(event) => update({ retention_months: Number(event.target.value) })}
              >
                {[1, 3, 6, 12, 24].map((months) => (
                  <option key={months} value={months}>
                    {months} 个月
                  </option>
                ))}
              </select>
            </div>
            <div className="setting-row">
              <span className="setting-copy">
                <strong>清除历史</strong>
                <span>不影响各工具自身的数据</span>
              </span>
              <span className="grow" />
              <button type="button" className="text-button" onClick={() => setConfirmClear(true)}>
                清除…
              </button>
            </div>
            {confirmClear ? (
              <div className="confirm-line">
                <span>清除这台电脑上的用量历史和清理记录？</span>
                <span className="grow" />
                <button type="button" className="text-button" onClick={() => setConfirmClear(false)}>
                  取消
                </button>
                <button
                  type="button"
                  className="text-button"
                  onClick={() => {
                    void clearHistory().then(() => setConfirmClear(false));
                  }}
                >
                  清除
                </button>
              </div>
            ) : null}
          </div>
        </section>
        <section>
          <p className="section-label">AI 摘要</p>
          <div className="card">
            <div className="setting-row">
              <span className="setting-copy">
                <strong>默认摘要方式</strong>
                <span>选择 AI 时，每次发送前确认范围</span>
              </span>
              <span className="grow" />
              <select
                className="select-field"
                aria-label="默认摘要方式"
                value={snapshot.settings.summary_mode}
                onChange={(event) =>
                  update({ summary_mode: event.target.value as Settings["summary_mode"] })
                }
              >
                <option value="ask">每次询问</option>
                <option value="local">只在本地</option>
                <option value="ai">使用 AI</option>
              </select>
            </div>
            <div className="setting-row">
              <span className="setting-copy">
                <strong>DeepSeek API 密钥</strong>
                <span>保存在系统钥匙串 · 不读取其他工具的凭据</span>
              </span>
              <span className="grow" />
              <button type="button" className="pill-button" onClick={() => setKeyOpen((open) => !open)}>
                管理
              </button>
            </div>
            {keyOpen ? (
              <form
                className="key-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  const key = draft;
                  void saveKey(key).then(() => {
                    setDraft("");
                    onChange((current) => ({ ...current, keySaved: true }));
                  });
                }}
              >
                <span>{snapshot.keySaved ? "已保存" : "未保存"}</span>
                <input
                  type="password"
                  autoComplete="off"
                  aria-label="DeepSeek API 密钥"
                  value={draft}
                  onChange={(event) => setDraft(event.target.value)}
                />
                <button type="submit" className="pill-button primary">
                  保存
                </button>
                {snapshot.keySaved ? (
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => {
                      void deleteKey().then(() => onChange((current) => ({ ...current, keySaved: false })));
                    }}
                  >
                    删除
                  </button>
                ) : null}
              </form>
            ) : null}
          </div>
        </section>
        <section>
          <p className="section-label">常驻与通知</p>
          <div className="card">
            <SwitchRow
              title="开机启动"
              detail="默认关闭"
              checked={snapshot.settings.launch_at_login}
              onClick={() => toggle("launch_at_login")}
            />
            <SwitchRow
              title="系统通知"
              detail="提醒默认只在应用内显示"
              checked={snapshot.settings.notifications}
              onClick={() => toggle("notifications")}
            />
            <SwitchRow
              title="菜单栏图标"
              checked={snapshot.settings.menu_bar_icon}
              onClick={() => toggle("menu_bar_icon")}
            />
            <div className="setting-row">
              <span className="setting-copy">
                <strong>关闭窗口时</strong>
              </span>
              <span className="grow" />
              <select
                className="select-field"
                aria-label="关闭窗口时"
                value={snapshot.settings.close_to_tray ? "tray" : "quit"}
                onChange={(event) => update({ close_to_tray: event.target.value === "tray" })}
              >
                <option value="tray">保留在菜单栏</option>
                <option value="quit">退出</option>
              </select>
            </div>
          </div>
        </section>
      </div>
      <aside className="settings-side" aria-label="工具与授权">
        <p className="section-label">工具与授权</p>
        <div className="card">
          {toolSpecs.map((spec) => {
            const live = snapshot.tools.find((tool) => tool.tool === spec.id);
            const presence = live?.presence ?? "missing";
            const grants = snapshot.grants.filter((grant) => grant.tool === spec.id);
            return (
              <div className="tool-block" key={spec.id}>
                <div className="tool-head">
                  <span className="mark sm" aria-hidden="true">
                    {spec.letter}
                  </span>
                  <span className="tool-copy">
                    <strong>{spec.name}</strong>
                    <span>{presenceLabel(presence)}</span>
                  </span>
                  <span className="grow" />
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => setOpenTool(openTool === spec.id ? "" : spec.id)}
                  >
                    管理
                  </button>
                </div>
                <div className="tag-row">
                  {spec.tags.map((tag) => (
                    <span key={tag.label} className={tag.badge === "未验证" ? "tag unverified" : "tag"}>
                      {tag.badge === "暂不提供" ? `${tag.label} · 暂不提供` : tag.label}
                    </span>
                  ))}
                </div>
                {openTool === spec.id ? (
                  <div className="cap-list">
                    {spec.caps.map((cap) => {
                      const grant = grants.find((item) => item.capability === cap.capability);
                      return (
                        <div className="cap-row disabled" key={cap.title}>
                          <input className="check" type="checkbox" disabled aria-label={cap.title} />
                          <span className="cap-copy">
                            <strong>{cap.title}</strong>
                            <span>{grant ? (grant.status === "needs_reauth" ? "需要重新授权" : "已授权") : cap.detail}</span>
                          </span>
                          <span className="grow" />
                          {grant ? (
                            <button
                              type="button"
                              className="text-button"
                              onClick={() => {
                                void revokeGrant(grant.id).then(() =>
                                  onChange((current) => ({
                                    ...current,
                                    grants: current.grants.filter((item) => item.id !== grant.id),
                                  })),
                                );
                              }}
                            >
                              撤销
                            </button>
                          ) : (
                            <span className={cap.badge === "未验证" ? "tag unverified" : "tag"}>{cap.badge}</span>
                          )}
                        </div>
                      );
                    })}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
        <div className="legend">
          <span className="tag">已授权</span>
          <span className="tag">未授权</span>
          <span className="tag unverified">未验证</span>
        </div>
      </aside>
    </div>
  );
}

function NumberRow({
  title,
  detail,
  value,
  suffix,
  onChange,
}: {
  title: string;
  detail: string;
  value: number;
  suffix: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="setting-row">
      <span className="setting-copy">
        <strong>{title}</strong>
        <span>{detail}</span>
      </span>
      <span className="grow" />
      <span className="field-pair">
        <input
          className="number-field"
          aria-label={title}
          inputMode="numeric"
          value={value}
          onChange={(event) => {
            const next = Number(event.target.value);
            if (Number.isFinite(next) && next > 0) onChange(next);
          }}
        />
        <span className="field-suffix">{suffix}</span>
      </span>
    </div>
  );
}

function SwitchRow({
  title,
  detail,
  checked,
  onClick,
}: {
  title: string;
  detail?: string;
  checked: boolean;
  onClick: () => void;
}) {
  return (
    <div className="setting-row">
      <span className="setting-copy">
        <strong>{title}</strong>
        {detail ? <span>{detail}</span> : null}
      </span>
      <span className="grow" />
      <button type="button" className="switch" role="switch" aria-checked={checked} aria-label={title} onClick={onClick}>
        <i />
      </button>
    </div>
  );
}
