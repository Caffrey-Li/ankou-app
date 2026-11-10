import { useState } from "react";
import mark from "../../brand/icon-macos.svg";
import type { Snapshot } from "./api";
import { finishSetup } from "./api";
import { presenceLabel, toolSpecs } from "./catalog";
import { ToolIcon } from "./tool-status";
import "./setup.css";
import "./settings.css";

export function SetupPage({
  snapshot,
  onDone,
}: {
  snapshot: Snapshot;
  onDone: () => void;
}) {
  const [open, setOpen] = useState<string>("");
  const found = snapshot.tools.filter((tool) => tool.installed).length;

  async function finish() {
    await finishSetup();
    onDone();
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
            <b aria-hidden="true">✓</b>发现工具
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
            const expanded = open === spec.id;
            return (
              <div key={spec.id}>
                <button
                  type="button"
                  className={expanded ? "setup-tool open" : "setup-tool"}
                  aria-expanded={expanded}
                  onClick={() => setOpen(expanded ? "" : spec.id)}
                >
                  <ToolIcon id={spec.id} className="mark lg" />
                  <span className="tool-copy">
                    <strong>{spec.name}</strong>
                    <span>{presenceLabel(presence)}</span>
                  </span>
                  <span className="chev">{expanded ? "收起" : "›"}</span>
                </button>
                {expanded ? (
                  <div className="cap-list">
                    {spec.caps.map((cap) => (
                      <div className="cap-row disabled" key={cap.title}>
                        <input className="check" type="checkbox" disabled aria-label={cap.title} />
                        <span className="cap-copy">
                          <strong>{cap.title}</strong>
                          <span>{cap.detail}</span>
                        </span>
                        <span className="grow" />
                        <span className={cap.badge === "未验证" ? "tag unverified" : "tag"}>
                          {cap.badge}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            );
          })}
          </div>
        </div>
        <div className="setup-foot">
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
