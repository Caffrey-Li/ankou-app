import { useState } from "react";
import mark from "../../brand/mark.svg";
import {
  IconBubble,
  IconChart,
  IconClock,
  IconDisk,
  IconHome,
  IconRefresh,
  IconSearch,
  IconSet,
} from "./icons";

const pages = [
  { id: "overview", label: "概览", icon: <IconHome /> },
  { id: "usage", label: "额度与用量", icon: <IconChart /> },
  { id: "handoff", label: "对话与交接", icon: <IconBubble /> },
  { id: "space", label: "空间管理", icon: <IconDisk /> },
  { id: "settings", label: "设置", icon: <IconSet /> },
] as const;

type PageId = (typeof pages)[number]["id"];

const tools = [
  { letter: "H", name: "DeepSeek Harness" },
  { letter: "B", name: "DS Bot" },
  { letter: "C", name: "Cursor" },
  { letter: "Cx", name: "Codex" },
];

const weekdays = ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"];

function formatDate(date: Date) {
  return `${weekdays[date.getDay()]} · ${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const current = pages.find((item) => item.id === page) ?? pages[0];
  const windows =
    typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);

  return (
    <div className="window">
      <aside className="sidebar">
        {windows ? null : <div className="traffic" aria-hidden="true" />}
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
              {item.icon}
              {item.label}
            </button>
          ))}
        </nav>
        <div className="nav-gap" aria-hidden="true" />
        <p className="tools-label">工具</p>
        <ul className="tools">
          {tools.map((tool) => (
            <li key={tool.name}>
              <span className="letter" aria-hidden="true">
                {tool.letter}
              </span>
              <span className="tool-name">{tool.name}</span>
              <span className="tool-dot" role="img" aria-label="未检测" />
            </li>
          ))}
        </ul>
        <p className="local">
          <span>本机运行</span>
          <span>数据只保存在这台电脑</span>
        </p>
      </aside>
      <main>
        <header className="toolbar">
          <div className="title-block">
            <h1>{current.label}</h1>
            <p className="subtitle">{formatDate(new Date())}</p>
          </div>
          <div className="toolbar-actions">
            <div className="glass-pill">
              <IconClock />
              <span>更新于 —</span>
              <span className="pill-rule" aria-hidden="true" />
              <button type="button" className="icon-hit" aria-label="刷新">
                <IconRefresh />
              </button>
            </div>
            <button type="button" className="glass-icon" aria-label="搜索">
              <IconSearch />
            </button>
          </div>
        </header>
        <section className="empty" aria-label="页面内容">
          <p className="lead">主窗口已就绪，这一页还没有数据。</p>
          <p className="detail">
            用量、对话和空间会在后续里程碑接入。当前没有读取任何工具的本机数据。
          </p>
          <span className="badge">未验证</span>
        </section>
      </main>
    </div>
  );
}
