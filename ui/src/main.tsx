import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { TrayPanel } from "./TrayPanel";
import "./shell.css";
import "./tokens.css";

function fitWindowFill() {
  const width = window.innerWidth;
  const height = window.innerHeight;
  const sidebarHeight = Math.max(0, height - 20);
  const hole = [
    "M25 9H217",
    "A16 16 0 0 1 233 25",
    `V${9 + sidebarHeight - 16}`,
    `A16 16 0 0 1 217 ${9 + sidebarHeight}`,
    `H25`,
    `A16 16 0 0 1 9 ${9 + sidebarHeight - 16}`,
    "V25",
    "A16 16 0 0 1 25 9Z",
  ].join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><path fill="white" fill-rule="evenodd" d="M0 0H${width}V${height}H0Z${hole}"/></svg>`;
  document.documentElement.style.setProperty(
    "--window-fill-mask",
    `url("data:image/svg+xml,${encodeURIComponent(svg)}")`,
  );
}

const view = window.location.hash === "#tray" ? "tray" : new URLSearchParams(window.location.search).get("view");
if (view === "tray") {
  document.documentElement.dataset.view = "tray";
} else {
  fitWindowFill();
  window.addEventListener("resize", fitWindowFill);
}

const compare = new URLSearchParams(window.location.search).get("compare") === "1";
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
