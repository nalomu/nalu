import { ref, readonly, computed } from "vue";

export const MOBILE_BREAKPOINT = 768;

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
  "schedule",
  "ai",
  "settings",
];

// Reactive window width
const windowWidth = ref(typeof window !== "undefined" ? window.innerWidth : 1200);

export function useMobile() {
  const isMobile = computed(() => windowWidth.value < MOBILE_BREAKPOINT);
  const enabledRoutes = computed(() =>
    isMobile.value ? MOBILE_ROUTES : DESKTOP_ROUTES,
  );

  function isRouteEnabled(name: string) {
    return enabledRoutes.value.includes(name);
  }

  return {
    isMobile: readonly(isMobile),
    enabledRoutes: readonly(enabledRoutes),
    isRouteEnabled,
    MOBILE_ROUTES,
    DESKTOP_ROUTES,
  };
}

if (typeof window !== "undefined") {
  window.addEventListener("resize", () => {
    windowWidth.value = window.innerWidth;
  });
}
