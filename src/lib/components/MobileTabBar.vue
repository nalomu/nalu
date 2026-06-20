<script setup lang="ts">
import { useRoute, useRouter } from "vue-router";
import {
  LayoutDashboard,
  CheckSquare,
  StickyNote,
  Timer,
  User,
} from "lucide-vue-next";
import { useI18n } from "$lib/i18n";
import { useMobile } from "$lib/composables/useMobile";

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const { isRouteEnabled } = useMobile();

const tabs = [
  { id: "dashboard", path: "/", label: "dashboard", icon: LayoutDashboard },
  { id: "tasks", path: "/tasks", label: "tasks", icon: CheckSquare },
  { id: "notes", path: "/notes", label: "notes", icon: StickyNote },
  { id: "pomodoro", path: "/pomodoro", label: "pomodoro", icon: Timer },
  { id: "settings", path: "/settings", label: "mobileMine", icon: User },
];

function isActive(path: string) {
  if (path === "/") return route.path === "/";
  return route.path.startsWith(path);
}
</script>

<template>
  <nav
    class="fixed bottom-0 left-0 right-0 z-50 flex items-stretch gap-1 overflow-x-auto border-t border-border bg-background px-2 mobile-safe-bottom"
    style="height: calc(var(--mobile-tabbar-height) + var(--mobile-safe-bottom))"
  >
    <button
      v-for="tab in tabs.filter((item) => isRouteEnabled(item.id))"
      :key="tab.id"
      class="flex min-w-[64px] flex-1 flex-col items-center justify-center gap-0.5 px-2 py-1 transition-colors"
      :class="isActive(tab.path) ? 'text-primary' : 'text-muted-foreground'"
      @click="router.push(tab.path)"
    >
      <component :is="tab.icon" class="w-5 h-5" />
      <span class="text-[10px] leading-tight">{{ tab.label === "mobileMine" ? t("nav.mobileMine") : t(`nav.${tab.label}`) }}</span>
    </button>
  </nav>
</template>
