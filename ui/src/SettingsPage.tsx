import { useState, type CSSProperties, type SelectHTMLAttributes } from "react";
import type { Settings, Snapshot } from "./api";
import { clearHistory, deleteKey, refreshUsage, revokeGrant, saveKey, saveSettings, setCapability } from "./api";
import { Check } from "./Check";
import { presenceLabel, specOf, vendors, type CapabilityId, type Presence, type VendorId } from "./catalog";
import { capChecked, grantableKind } from "./grants";
import { UpdateSection, type Updater } from "./update";
import "./settings.css";

export function SettingsPage({
  snapshot,
  onChange,
  updater,
  onTools,
}: {
  snapshot: Snapshot;
  onChange: (update: (current: Snapshot) => Snapshot) => void;
  onReload: () => void;
  /** 版本与更新；没有传时不显示这一节 */
  updater?: Updater;
  /** 授权已移到「工具」里：设置页顶部给一个入口 */
  onTools?: () => void;
}) {
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
    <div className="settings-page">
      {onTools ? (
        <div className="settings-banner">
          <span>各工具的授权已移到「工具」里，每个厂商一页管理。</span>
          <button type="button" className="text-button" onClick={onTools}>
            去工具
          </button>
        </div>
      ) : null}
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
              <SelectPill
                aria-label="用量历史保留"
                value={snapshot.settings.retention_months}
                onChange={(event) => update({ retention_months: Number(event.target.value) })}
              >
                {[1, 3, 6, 12, 24].map((months) => (
                  <option key={months} value={months}>
                    {months} 个月
                  </option>
                ))}
              </SelectPill>
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
      </div>
      <div className="settings-column">
        <section>
          <p className="section-label">外观</p>
          <div className="card">
            <div className="setting-row">
              <span className="setting-copy">
                <strong>主题</strong>
                <span>主窗口和菜单栏面板一起切换</span>
              </span>
              <span className="grow" />
              <div className="segmented" role="radiogroup" aria-label="主题">
                {(
                  [
                    ["system", "跟随系统"],
                    ["light", "浅色"],
                    ["dark", "深色"],
                  ] as const
                ).map(([value, label]) => (
                  <button
                    key={value}
                    type="button"
                    role="radio"
                    aria-checked={snapshot.settings.appearance === value}
                    onClick={() => update({ appearance: value })}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
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
              <SelectPill
                aria-label="默认摘要方式"
                value={snapshot.settings.summary_mode}
                onChange={(event) =>
                  update({ summary_mode: event.target.value as Settings["summary_mode"] })
                }
              >
                <option value="ask">每次询问</option>
                <option value="local">只在本地</option>
                <option value="ai">使用 AI</option>
              </SelectPill>
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
              <SelectPill
                aria-label="关闭窗口时"
                value={snapshot.settings.close_to_tray ? "tray" : "quit"}
                onChange={(event) => update({ close_to_tray: event.target.value === "tray" })}
              >
                <option value="tray">保留在菜单栏</option>
                <option value="quit">退出</option>
              </SelectPill>
            </div>
          </div>
        </section>
        {updater ? <UpdateSection updater={updater} /> : null}
      </div>
    </div>
    </div>
  );
}


/** 一个厂商下各端的授权：在「工具」页里显示。 */
export function VendorAccess({
  vendorId,
  snapshot,
  onChange,
  onReload,
}: {
  vendorId: VendorId;
  snapshot: Snapshot;
  onChange: (update: (current: Snapshot) => Snapshot) => void;
  onReload: () => void;
}) {
  const vendor = vendors.find((item) => item.id === vendorId);
  if (!vendor) return null;
  const members = vendor.members.map((id) => specOf(id)).filter((spec) => spec !== undefined);
  const presences = members.map((spec) => snapshot.tools.find((tool) => tool.tool === spec.id)?.presence ?? "missing");
  const open = true;
  const multi = members.length > 1;
  function toggleMeasured(kind: string, on: boolean) {
    void setCapability(kind, on).then(() => {
      onReload();
      // 刚授权的用量要马上读一次，不然用量还停在「未授权」
      if (on && kind.endsWith("_usage")) void refreshUsage().catch(() => undefined);
    });
  }
  return (
    <div className="tool-block vendor-access" id={`tool-${vendor.id}`}>
                {members.map((spec, index) => {
                  const presence = presences[index];
                  const grants = snapshot.grants.filter((grant) => grant.tool === spec.id);
                  return (
                    <div className={multi ? "tool-end" : "tool-end single"} key={spec.id}>
                      {multi || (open && spec.ends) ? (
                        <div className="end-head">
                          <strong>{multi ? spec.name : "覆盖"}</strong>
                          {multi ? <span className="end-state">{presenceLabel(presence)}</span> : null}
                          {open && spec.ends ? <span className="end-note">{spec.ends}</span> : null}
                        </div>
                      ) : null}
                      <div className="tag-row">
                        {spec.tags.map((tag) => {
                          const state = tagState(tag.badge, tag.capability, grants);
                          return (
                            <span key={tag.label} className={authTagClass(state)}>
                              {tag.badge === "已实测" ? tag.label : `${tag.label} · ${tag.badge}`}
                            </span>
                          );
                        })}
                      </div>
                      {open ? (
                        <div className="cap-list">
                          {spec.caps.map((cap) => {
                            const grant = grants.find((item) => item.capability === cap.capability);
                            const kind = grantableKind(spec.id, cap);
                            if (kind) {
                              const checked = capChecked(cap, grants);
                              return (
                                <div className="cap-row" key={cap.title}>
                                  <Check
                                    aria-label={multi ? `${spec.name} ${cap.title}` : cap.title}
                                    checked={checked}
                                    onChange={(event) => toggleMeasured(kind, event.target.checked)}
                                  />
                                  <span className="cap-copy">
                                    <strong>{cap.title}</strong>
                                    <span>{cap.detail}</span>
                                  </span>
                                </div>
                              );
                            }
                            return (
                              <div className="cap-row disabled" key={cap.title}>
                                <Check disabled aria-label={multi ? `${spec.name} ${cap.title}` : cap.title} />
                                <span className="cap-copy">
                                  <strong>{cap.title}</strong>
                                  <span>
                                    {grant ? (grant.status === "needs_reauth" ? "需要重新授权" : "已授权") : cap.detail}
                                  </span>
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
                                  <span className={authTagClass(cap.badge)}>{cap.badge}</span>
                                )}
                              </div>
                            );
                          })}
                          {spec.id === "codex" ? (
                            <div className="cap-row">
                              <Check
                                aria-label="ChatGPT app-server"
                                checked={snapshot.settings.codex_app_server}
                                onChange={(event) => toggleMeasured("codex_app_server", event.target.checked)}
                              />
                              <span className="cap-copy">
                                <strong>ChatGPT app-server</strong>
                                <span>读取额度窗口，只在启动和手动刷新时调用</span>
                              </span>
                            </div>
                          ) : null}
                        </div>
                      ) : null}
                    </div>
                  );
                })}
    </div>
  );
}

/** 厂商一行的状态：各端里只要有一端已授权就算已授权，有失效的优先提示。 */
export function vendorPresence(presences: Presence[]) {
  const found = presences.filter((presence) => presence !== "missing").length;
  const label = presences.includes("needs_reauth")
    ? presenceLabel("needs_reauth")
    : presences.includes("authorized")
      ? presenceLabel("authorized")
      : presences.includes("unauthorized")
        ? presenceLabel("unauthorized")
        : presenceLabel("missing");
  return presences.length > 1 && found > 0 ? `${label} · 发现 ${found}/${presences.length} 端` : label;
}

export function tagState(
  badge: "未验证" | "暂不提供" | "已实测" | "复制后粘贴",
  capability: CapabilityId,
  grants: { capability: CapabilityId; status: string }[],
): "已授权" | "未授权" | "未验证" {
  if (badge === "未验证") return "未验证";
  if (badge === "暂不提供") return "未授权";
  if (badge === "复制后粘贴") return "已授权";
  return grants.some((grant) => grant.capability === capability && grant.status === "active") ? "已授权" : "未授权";
}

export function authTagClass(state: "已授权" | "未授权" | "未验证" | "暂不提供" | "已实测" | "复制后粘贴") {
  switch (state) {
    case "已授权":
    case "已实测":
    case "复制后粘贴":
      return "tag granted";
    case "未授权":
      return "tag unauthorized";
    case "未验证":
      return "tag unverified";
    case "暂不提供":
      return "tag withheld";
  }
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
          style={{ "--digits": Math.max(1, String(value).length) } as CSSProperties}
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

function SelectPill({ children, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  // 原生 select 按最长选项估宽度不准：按当前选中项的字数给宽度，箭头不压字
  const labels: string[] = [];
  const walk = (node: unknown) => {
    if (Array.isArray(node)) node.forEach(walk);
    else if (node && typeof node === "object" && "props" in node) {
      const item = node as { props: { value?: unknown; children?: unknown } };
      if (String(item.props.value) === String(props.value)) {
        const text = item.props.children;
        labels.push(Array.isArray(text) ? text.join("") : String(text ?? ""));
      }
    }
  };
  walk(children);
  // 中文按 12px、数字与空格按 7px 估
  const width = Math.max(
    24,
    ...labels.map((label) => [...label].reduce((sum, char) => sum + (/[\u4e00-\u9fff]/.test(char) ? 12 : 7), 0)),
  );
  return (
    <span className="select-pill">
      <select {...props} style={{ width: `${width + 38}px` }}>
        {children}
      </select>
      <i aria-hidden="true">⌄</i>
    </span>
  );
}
