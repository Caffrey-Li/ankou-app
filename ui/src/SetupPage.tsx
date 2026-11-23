import { useState } from "react";
import mark from "../../brand/icon-macos.svg";
import type { Snapshot } from "./api";
import { finishSetup, openPrivacySettings, refreshUsage, setCapability } from "./api";
import { Check } from "./Check";
import { presenceLabel, toolSpecs, type CapRow, type ToolId } from "./catalog";
import { capChecked, grantableKind, grantedCount } from "./grants";
import { IconCheck, IconLock } from "./icons";
import { ToolIcon } from "./tool-status";
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
  const [open, setOpen] = useState<ToolId | "">("");
  const [error, setError] = useState("");
  const found = snapshot.tools.filter((tool) => tool.installed).length;

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

  return (
    <div className="setup">
      <section className="setup-card" data-compare-frame="setup" aria-label="首次启动">
        <img className="setup-mark" src={mark} alt="" width={84} height={84} />
        <div className="setup-title">
          <h1>欢迎使用万象小助手</h1>
          <p>看清用量 · 延续对话 · 管理空间</p>
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
            <b>3</b>开始使用
          </span>
        </div>
        <div className="setup-list">
          <div className="setup-heading">
            <p>在这台电脑上找到 {found} 个工具</p>
            <span>只检查是否安装，未读取任何内容</span>
          </div>
          <div className="setup-tools">
            {toolSpecs.map((spec) => {
              const live = snapshot.tools.find((tool) => tool.tool === spec.id);
              const presence = live?.presence ?? "missing";
              const grants = snapshot.grants.filter((grant) => grant.tool === spec.id);
              const expanded = open === spec.id;
              const count = grantedCount(spec.id, spec.caps, snapshot.grants);
              const parts = [presence === "missing" ? presenceLabel(presence) : count > 0 ? `已授权 ${count} 项` : "未授权"];
              if (spec.id === "ds_bot") parts.push("随 Harness");
              if (expanded && spec.id === "cursor") parts.push("能力逐项授权");
              const head = (
                <button
                  type="button"
                  className="setup-tool"
                  aria-expanded={expanded}
                  onClick={() => setOpen(expanded ? "" : spec.id)}
                >
                  <ToolIcon id={spec.id} className="mark lg" />
                  <span className="tool-copy">
                    <strong>{spec.name}</strong>
                    <span>{parts.join(" · ")}</span>
                  </span>
                  <span className={expanded ? "chev text" : "chev"}>{expanded ? "收起" : "›"}</span>
                </button>
              );
              if (!expanded) {
                return (
                  <div className="setup-row" key={spec.id}>
                    {head}
                  </div>
                );
              }
              return (
                <div className="setup-panel" key={spec.id}>
                  {head}
                  {spec.caps.map((cap) => (
                    <SetupCap
                      key={cap.title}
                      tool={spec.id}
                      cap={cap}
                      checked={capChecked(cap, grants)}
                      onToggle={toggle}
                    />
                  ))}
                </div>
              );
            })}
          </div>
          {error ? <p className="setup-error">{error}</p> : null}
        </div>
        <div className="setup-foot">
          <IconLock />
          <p>不会读取登录凭据、密钥或验证码 · 授权可在设置中随时撤销</p>
          <span className="grow" />
          <button type="button" className="pill-button" onClick={finish}>
            稍后
          </button>
          <button type="button" className="pill-button primary" onClick={finish}>
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
