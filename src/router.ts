import { createRouter, createWebHashHistory } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import AppLayout from "$lib/components/AppLayout.vue";
import { MOBILE_ROUTES, type RuntimePlatform } from "$lib/composables/useMobile";

const routes = [
  {
    path: "/",
    component: AppLayout,
    children: [
      { path: "", name: "dashboard", component: () => import("$lib/components/pages/DashboardPage.vue") },
      { path: "tasks", name: "tasks", component: () => import("$lib/components/pages/TasksPage.vue") },
      { path: "notes", name: "notes", component: () => import("$lib/components/pages/NotesPage.vue") },
      { path: "clipboard", name: "clipboard", component: () => import("$lib/components/pages/ClipboardPage.vue") },
      { path: "pomodoro", name: "pomodoro", component: () => import("$lib/components/pages/PomodoroPage.vue") },
      { path: "schedule", name: "schedule", component: () => import("$lib/components/pages/SchedulePage.vue") },
      { path: "mysql", name: "mysql", component: () => import("$lib/components/pages/MysqlPage.vue") },
      { path: "alarm", name: "alarm", component: () => import("$lib/components/pages/AlarmPage.vue") },
      { path: "ai", name: "ai", component: () => import("$lib/components/pages/AiPage.vue") },
      { path: "settings", name: "settings", component: () => import("$lib/components/pages/SettingsPage.vue") },
    ],
  },
  {
    path: "/clipboard-popup",
    name: "clipboard-popup",
    component: () => import("$lib/components/ClipboardPopup.vue"),
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

function normalizePlatform(value: unknown): RuntimePlatform | null {
  return value === "android" || value === "ios" || value === "desktop" ? value : null;
}

async function getRuntimePlatform(): Promise<RuntimePlatform> {
  try {
    const platform = normalizePlatform(await invoke("runtime_platform"));
    if (platform) return platform;
  } catch {
    if (typeof window !== "undefined") {
      const mocked = normalizePlatform(window.__NALU_RUNTIME_PLATFORM__);
      if (mocked) return mocked;
    }
  }
  return "desktop";
}

// Mobile route guard: redirect disabled features only on mobile runtime platforms.
router.beforeEach(async (to) => {
  if (to.name === "clipboard-popup") return;

  const platform = await getRuntimePlatform();
  if (platform !== "android" && platform !== "ios") return;

  const name = to.name as string;
  if (name && !MOBILE_ROUTES.includes(name)) {
    return { name: "dashboard" };
  }
});

export default router;
