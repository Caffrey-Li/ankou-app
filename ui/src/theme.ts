import type { Appearance } from "./api";

/** 当前外观设置；「跟随系统」时随系统深浅色变化。 */
let current: Appearance = "system";
let media: MediaQueryList | null = null;

function systemDark() {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

export function resolvedTheme(appearance: Appearance, dark = systemDark()): "light" | "dark" {
  if (appearance === "light" || appearance === "dark") return appearance;
  return dark ? "dark" : "light";
}

function paint() {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = resolvedTheme(current);
}

/** 应用外观：设置 html 的 data-theme；跟随系统时监听系统变化。 */
export function applyAppearance(appearance: Appearance) {
  current = appearance;
  paint();
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
  if (!media) {
    media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", () => {
      if (current === "system") paint();
    });
  }
}
