import { useState } from "react";
import mark from "../../brand/mark.svg";

const pages = [
  { id: "overview", label: "概览" },
  { id: "usage", label: "额度与用量" },
  { id: "handoff", label: "对话与交接" },
  { id: "space", label: "空间管理" },
  { id: "settings", label: "设置" },
] as const;

type PageId = (typeof pages)[number]["id"];

const tools = [
  { letter: "H", name: "DeepSeek Harness" },
  { letter: "B", name: "DS Bot" },
  { letter: "C", name: "Cursor" },
  { letter: "Cx", name: "Codex" },
];

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const current = pages.find((item) => item.id === page) ?? pages[0];
  const mac =
    typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

  return (
    <div className="window">
      <aside className={mac ? "sidebar mac" : "sidebar"}>
        <div className="brand">
          <img src={mark} alt="" width={22} height={22} />
          <span>万象小助手</span>
        </div>
        <nav aria-label="主导航">
          {pages.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-current={item.id === current.id ? "page" : undefined}
              onClick={() => setPage(item.id)}
            >
              {item.label}
            </button>
          ))}
        </nav>
        <p className="tools-label">工具</p>
        <ul className="tools">
          {tools.map((tool) => (
            <li key={tool.name}>
              <span className="letter" aria-hidden="true">
                {tool.letter}
              </span>
              <span className="tool-name">{tool.name}</span>
              <span className="tool-state">未检测</span>
            </li>
          ))}
        </ul>
        <p className="local">
          <span>本机运行</span>
          <span>数据只保存在这台电脑</span>
        </p>
      </aside>
      <main>
        <h1>{current.label}</h1>
        <p className="lead">主窗口已就绪，这一页还没有数据。</p>
        <p className="detail">
          用量、对话和空间会在后续里程碑接入。当前没有读取任何工具的本机数据。
        </p>
        <span className="badge">未验证</span>
      </main>
    </div>
  );
}
