import { createRouter, createWebHashHistory } from "vue-router";
import AppLayout from "$lib/components/AppLayout.vue";
import { MOBILE_BREAKPOINT, MOBILE_ROUTES } from "$lib/composables/useMobile";

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

// Mobile route guard: redirect disabled features to dashboard.
router.beforeEach((to) => {
  if (typeof window !== "undefined" && window.innerWidth < MOBILE_BREAKPOINT) {
    const name = to.name as string;
    if (name && !MOBILE_ROUTES.includes(name)) {
      return { name: "dashboard" };
    }
  }
});

export default router;
