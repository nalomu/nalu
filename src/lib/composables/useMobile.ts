import { computed, readonly, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export const COMPACT_WIDTH_BREAKPOINT = 768;

export type RuntimePlatform = "desktop" | "android" | "ios";

export const DESKTOP_ROUTES = [
  "dashboard",
  "tasks",
  "notes",
  "clipboard",
  "pomodoro",
  "schedule",
  "mysql",
  "alarm",
  "ai",
  "settings",
];

export const MOBILE_ROUTES = [
  "dashboard",
  "tasks",
  "notes",
  "pomodoro",
  "ai",
  "settings",
];

const windowWidth = ref(typeof window !== "undefined" ? window.innerWidth : 1200);
const runtimePlatform = ref<RuntimePlatform>(detectFallbackPlatform());
let platformLoaded = false;

declare global {
  interface Window {
    __NALU_RUNTIME_PLATFORM__?: RuntimePlatform;
  }
}

function normalizePlatform(value: unknown): RuntimePlatform | null {
  return value === "android" || value === "ios" || value === "desktop" ? value : null;
}

function detectFallbackPlatform(): RuntimePlatform {
  if (typeof window !== "undefined") {
    const mocked = normalizePlatform(window.__NALU_RUNTIME_PLATFORM__);
    if (mocked) return mocked;
    if (/Android/i.test(window.navigator.userAgent)) return "android";
    if (/iPhone|iPad|iPod/i.test(window.navigator.userAgent)) return "ios";
  }
  return "desktop";
}

async function loadRuntimePlatform() {
  if (platformLoaded) return;
  platformLoaded = true;
  try {
    const platform = normalizePlatform(await invoke("runtime_platform"));
    if (platform) runtimePlatform.value = platform;
  } catch {
    runtimePlatform.value = detectFallbackPlatform();
  }
}

export function useMobile() {
  void loadRuntimePlatform();

  const isCompactWidth = computed(() => windowWidth.value < COMPACT_WIDTH_BREAKPOINT);
  const isMobilePlatform = computed(() => runtimePlatform.value === "android" || runtimePlatform.value === "ios");
  const isDesktopPlatform = computed(() => runtimePlatform.value === "desktop");
  const isMobile = isMobilePlatform;
  const enabledRoutes = computed(() =>
    isMobilePlatform.value ? MOBILE_ROUTES : DESKTOP_ROUTES,
  );

  function isRouteEnabled(name: string) {
    return enabledRoutes.value.includes(name);
  }

  return {
    isMobile: readonly(isMobile),
    isMobilePlatform: readonly(isMobilePlatform),
    isDesktopPlatform: readonly(isDesktopPlatform),
    isCompactWidth: readonly(isCompactWidth),
    runtimePlatform: readonly(runtimePlatform),
    enabledRoutes: readonly(enabledRoutes),
    isRouteEnabled,
    MOBILE_ROUTES,
    DESKTOP_ROUTES,
    COMPACT_WIDTH_BREAKPOINT,
  };
}

if (typeof window !== "undefined") {
  window.addEventListener("resize", () => {
    windowWidth.value = window.innerWidth;
  });
}
