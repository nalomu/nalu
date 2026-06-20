import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "nalu-theme";
const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)";
const SYSTEM_THEME_CHANGED_EVENT = "nalu://system-theme-changed";

type ResolvedTheme = "light" | "dark";
type LegacyMediaQueryList = MediaQueryList & {
  addListener?: (listener: (event: MediaQueryListEvent) => void) => void;
  removeListener?: (listener: (event: MediaQueryListEvent) => void) => void;
};

let systemThemeOverride: ResolvedTheme | null = null;

function darkMedia(): LegacyMediaQueryList {
  return window.matchMedia(DARK_MEDIA_QUERY) as LegacyMediaQueryList;
}

function isDarkPreferred(): boolean {
  if (systemThemeOverride) return systemThemeOverride === "dark";
  return darkMedia().matches;
}

function apply(mode: ThemeMode): void {
  const shouldDark = mode === "dark" || (mode === "system" && isDarkPreferred());
  document.documentElement.classList.toggle("dark", shouldDark);
  document.documentElement.style.colorScheme = shouldDark ? "dark" : "light";
}

/** Read the stored theme (defaults to "system"). */
function read(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

function applySystemTheme(theme: ResolvedTheme | null | undefined): void {
  if (theme !== "light" && theme !== "dark") return;
  systemThemeOverride = theme;
  if (read() === "system") apply("system");
}

/** Persist and apply a new theme mode. */
export function setTheme(mode: ThemeMode): void {
  localStorage.setItem(STORAGE_KEY, mode);
  apply(mode);
}

/** Initialise theme on app startup. Call once from main.ts / popup entry. */
export function initTheme(): () => void {
  const mode = read();
  apply(mode);

  // When mode is "system", re-evaluate whenever the OS preference changes.
  const handler = () => {
    if (read() === "system") apply("system");
  };
  const media = darkMedia();
  const supportsModernMediaListener = typeof media.addEventListener === "function";
  const cleanupMedia = supportsModernMediaListener
    ? () => media.removeEventListener("change", handler)
    : () => media.removeListener?.(handler);

  let cleanupTauriTheme: (() => void) | undefined;
  let cleanupNaluTheme: (() => void) | undefined;

  if (supportsModernMediaListener) {
    media.addEventListener("change", handler);
  } else {
    media.addListener?.(handler);
  }

  try {
    const win = getCurrentWindow();
    void win
      .theme()
      .then((t) => applySystemTheme(t))
      .catch((error) => console.warn("[theme] Tauri window theme unavailable", error));
    void win
      .onThemeChanged(({ payload }) => applySystemTheme(payload))
      .then((unlisten) => {
        cleanupTauriTheme = unlisten;
      })
      .catch((error) => console.warn("[theme] Tauri window theme unavailable", error));
  } catch (error) {
    console.warn("[theme] Tauri window theme unavailable", error);
  }

  void (async () => {
    try {
      applySystemTheme(await invoke<ResolvedTheme>("get_system_theme"));
      cleanupNaluTheme = await listen<ResolvedTheme>(SYSTEM_THEME_CHANGED_EVENT, ({ payload }) => {
        applySystemTheme(payload);
      });
    } catch (error) {
      console.warn("[theme] Tauri system theme command unavailable", error);
    }
  })();

  return () => {
    cleanupMedia();
    cleanupTauriTheme?.();
    cleanupNaluTheme?.();
  };
}

/** Returns the currently effective theme ("light" | "dark"). */
export function effectiveTheme(): "light" | "dark" {
  const mode = read();
  if (mode === "system") return isDarkPreferred() ? "dark" : "light";
  return mode;
}
