import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyAppearance } from "./theme";
import { TrayPanel } from "./TrayPanel";
import "./shell.css";
import "./tokens.css";

const view = window.location.hash === "#tray" ? "tray" : new URLSearchParams(window.location.search).get("view");
if (view === "tray") {
  document.documentElement.dataset.view = "tray";
}
// 设置读出来之前先按系统深浅色画，避免启动时闪一下
applyAppearance("system");

const compare = import.meta.env.DEV && new URLSearchParams(window.location.search).get("compare") === "1";
if (compare) {
  document.documentElement.dataset.compare = "1";
  await import("./compare-fonts.css");
}

const root = document.getElementById("root");
if (!root) {
  throw new Error("缺少根节点");
}

createRoot(root).render(
  <StrictMode>
    {view === "tray" ? <TrayPanel /> : <App />}
  </StrictMode>,
);
