<script setup lang="ts">
import { useRoute, useRouter } from "vue-router";
import {
  LayoutDashboard,
  CheckSquare,
  StickyNote,
  Calendar,
  Bot,
} from "lucide-vue-next";
import { useI18n } from "$lib/i18n";

const route = useRoute();
const router = useRouter();
const { t } = useI18n();

const tabs = [
  { id: "dashboard", path: "/", label: "dashboard", icon: LayoutDashboard },
  { id: "tasks", path: "/tasks", label: "tasks", icon: CheckSquare },
  { id: "notes", path: "/notes", label: "notes", icon: StickyNote },
  { id: "schedule", path: "/schedule", label: "schedule", icon: Calendar },
  { id: "ai", path: "/ai", label: "ai", icon: Bot },
];

function isActive(path: string) {
  if (path === "/") return route.path === "/";
  return route.path.startsWith(path);
}
</script>

<template>
  <nav
    class="fixed bottom-0 left-0 right-0 z-50 flex items-center justify-around bg-background border-t border-border safe-bottom"
    style="height: 56px; padding-bottom: env(safe-area-inset-bottom, 0)"
  >
    <button
      v-for="tab in tabs"
      :key="tab.id"
      class="flex flex-col items-center justify-center gap-0.5 min-w-0 px-2 py-1 transition-colors"
      :class="isActive(tab.path) ? 'text-primary' : 'text-muted-foreground'"
      @click="router.push(tab.path)"
    >
      <component :is="tab.icon" class="w-5 h-5" />
      <span class="text-[10px] leading-tight">{{ t(`nav.${tab.label}`) }}</span>
    </button>
  </nav>
</template>
