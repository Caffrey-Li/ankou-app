import { useState } from "react";
import { Logo } from "./Logo";
import type { Snapshot } from "./api";
import { finishSetup, openPrivacySettings, refreshUsage, setCapability } from "./api";
import { Check } from "./Check";
import { presenceLabel, specOf, vendors, type CapRow, type ToolId, type Vendor, type VendorId } from "./catalog";
import { capChecked, grantableKind, grantedCount } from "./grants";
import { IconCheck, IconLock } from "./icons";
import { ToolIcon } from "./tool-status";
import { EndIcon } from "./ToolsPage";
import "./console.css";
import "./setup.css";
import "./settings.css";

export function SetupPage({
  snapshot,
  onDone,
  onReload,
}: {
  snapshot: Snapshot;
  onDone: () => void;
  onReload: () => void;
}) {
  const [open, setOpen] = useState<VendorId | "">("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const presenceOf = (id: ToolId) => snapshot.tools.find((tool) => tool.tool === id)?.presence ?? "missing";
  const found = vendors.filter((vendor) => vendor.members.some((id) => presenceOf(id) !== "missing"));

  async function finish() {
    await finishSetup();
    onDone();
  }

  function toggle(kind: string, on: boolean) {
    setError("");
    void setCapability(kind, on)
      .then(() => {
        if (on && kind.endsWith("_usage")) void refreshUsage().catch(() => undefined);
      })
      .catch((reason) => setError(String(reason)))
      .finally(onReload);
  }

  /** 一个厂商的所有可授权项（只读），只对本机发现了的端。 */
  function kindsOf(vendor: Vendor) {
    return vendor.members
      .filter((id) => presenceOf(id) !== "missing")
      .flatMap((id) => (specOf(id)?.caps ?? []).map((cap) => grantableKind(id, cap)))
      .filter((kind) => kind != null) as string[];
  }

  function vendorOn(vendor: Vendor) {
    return vendor.members.some((id) => grantedCount(id, specOf(id)?.caps ?? [], snapshot.grants) > 0);
  }

  async function setVendor(vendor: Vendor, on: boolean) {
    setBusy(true);
    setError("");
    try {
      for (const kind of kindsOf(vendor)) await setCapability(kind, on);
      if (on) void refreshUsage().catch(() => undefined);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
      onReload();
    }
  }

  async function connectAll() {
    for (const vendor of found) if (!vendorOn(vendor)) await setVendor(vendor, true);
  }

  const chosen = found.filter(vendorOn).length;

  return (
    <div className="setup">
      <div className="setup-traffic" data-tauri-drag-region aria-hidden="true" />
      <section className="setup-card" data-compare-frame="setup" aria-label="首次启动">
        <header className="setup-top">
          <Logo className="setup-mark" size={44} />
          <div className="setup-title">
            <h1>欢迎使用安扣助手</h1>
            <p>
              在这台电脑上发现了 {found.length} 个 AI 工具。打开开关即接入（只读），随时可以在「工具」里改。
            </p>
          </div>
          <div className="steps" aria-label="步骤">
            <span className="step done">
              <b aria-hidden="true">
                <IconCheck />
              </b>
              发现工具
            </span>
            <i className="step-line" />
            <span className="step current">
              <b>2</b>选择授权
            </span>
            <i className="step-line" />
            <span className="step">
              <b>3</b>完成
            </span>
          </div>
        </header>

        <div className="setup-grid">
          {vendors.map((vendor) => {
            const members = vendor.members.map((id) => specOf(id)).filter((spec) => spec !== undefined);
            const seen = members.filter((spec) => presenceOf(spec.id) !== "missing");
            const on = vendorOn(vendor);
            const reads = [
              ...new Set(
                seen.flatMap((spec) =>
                  spec.caps.filter((cap) => grantableKind(spec.id, cap)).map((cap) => cap.title.replace(/与.*$/, "")),
                ),
              ),
            ];
            const expanded = open === vendor.id;
            return (
              <div key={vendor.id} className={on ? "setup-vendor on" : seen.length ? "setup-vendor" : "setup-vendor missing"}>
                <div className="setup-vendor-head">
                  <ToolIcon id={vendor.icon} className="c-icon" />
                  <strong>{vendor.name}</strong>
                  {seen.length ? (
                    <button
                      type="button"
                      className="switch"
                      role="switch"
                      aria-checked={on}
                      aria-label={`接入 ${vendor.name}`}
                      disabled={busy || reads.length === 0}
                      onClick={() => void setVendor(vendor, !on)}
                    >
                      <i />
                    </button>
                  ) : (
                    <span className="setup-missing">{presenceLabel("missing")}</span>
                  )}
                </div>
                <div className="setup-ends">
                  {members.map((spec) => (
                    <span key={spec.id} className={presenceOf(spec.id) === "missing" ? "dim" : undefined}>
                      <EndIcon id={spec.id} />
                      {members.length > 1 ? spec.name : spec.ends ?? spec.name}
                    </span>
                  ))}
                </div>
                {seen.length ? (
                  <p className="setup-reads">将读取：{reads.length ? reads.join(" · ") : "暂不提供"}</p>
                ) : null}
                {seen.length ? (
                  <button type="button" className="text-button setup-more" aria-expanded={expanded} onClick={() => setOpen(expanded ? "" : vendor.id)}>
                    {expanded ? "收起" : "逐项选择"}
                  </button>
                ) : null}
                {expanded
                  ? seen.map((spec) => {
                      const grants = snapshot.grants.filter((grant) => grant.tool === spec.id);
                      return (
                        <div key={spec.id} className="setup-caps">
                          {seen.length > 1 ? <p className="setup-end-name">{spec.name}</p> : null}
                          {spec.caps.map((cap) => (
                            <SetupCap key={cap.title} tool={spec.id} cap={cap} checked={capChecked(cap, grants)} onToggle={toggle} />
                          ))}
                        </div>
                      );
                    })
                  : null}
              </div>
            );
          })}
        </div>
        {error ? <p className="setup-error">{error}</p> : null}

        <div className="setup-note">
          <IconLock />
          <p>只读：不读取登录凭据、密钥或验证码；数据只保存在这台电脑；授权随时可以撤销。</p>
        </div>
        <div className="setup-foot">
          <span className="setup-count">已接入 {chosen} 个厂商</span>
          <span className="grow" />
          <button type="button" className="pill-button" disabled={busy} onClick={() => void connectAll()}>
            全部接入
          </button>
          <button type="button" className="pill-button primary" disabled={busy} onClick={finish}>
            继续
          </button>
        </div>
      </section>
    </div>
  );
}

function SetupCap({
  tool,
  cap,
  checked,
  onToggle,
}: {
  tool: ToolId;
  cap: CapRow;
  checked: boolean;
  onToggle: (kind: string, on: boolean) => void;
}) {
  const kind = grantableKind(tool, cap);
  if (cap.capability === "protected_location") {
    return (
      <div className="cap-row">
        <Check disabled aria-label={cap.title} />
        <span className="cap-copy">
          <strong>{cap.title}</strong>
          <span>{cap.detail}</span>
        </span>
        <span className="grow" />
        <button type="button" className="text-button" onClick={() => void openPrivacySettings()}>
          打开系统设置
        </button>
      </div>
    );
  }
  return (
    <div className={kind ? "cap-row" : "cap-row disabled"}>
      <Check
        aria-label={cap.title}
        disabled={!kind}
        checked={kind ? checked : false}
        onChange={(event) => kind && onToggle(kind, event.target.checked)}
      />
      <span className="cap-copy">
        <strong>{cap.title}</strong>
        <span>{cap.detail}</span>
      </span>
      <span className="grow" />
      <span className={capTagClass(cap, kind)}>{capTag(cap, kind)}</span>
    </div>
  );
}

export function capTag(cap: CapRow, kind: string | null) {
  if (cap.badge === "未验证") return "未验证";
  if (cap.badge === "暂不提供") return "暂不提供";
  if (cap.badge === "复制后粘贴") return "复制后粘贴";
  if (cap.capability === "cleanup") return "每次确认";
  return kind ? "可用" : "未验证";
}

function capTagClass(cap: CapRow, kind: string | null) {
  if (cap.badge === "未验证") return "tag unverified";
  if (cap.badge === "复制后粘贴") return "tag granted";
  if (cap.badge === "暂不提供" || !kind) return "tag";
  return "tag granted";
}
