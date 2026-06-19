<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  LayoutDashboard, CheckSquare, FileText, Scissors, Timer, Calendar,
  Database, AlarmClock, Settings, Command, Sparkles
} from 'lucide-vue-next'
import { getVersion } from '@tauri-apps/api/app'
import { useI18n } from '$lib/i18n'
import { useMobile } from '$lib/composables/useMobile'

const emit = defineEmits<{ command: [] }>()
const route = useRoute()
const router = useRouter()
const { t, version } = useI18n()
const { isRouteEnabled } = useMobile()
const appVersion = ref('0.0.0')
onMounted(async () => { appVersion.value = await getVersion() })

const allItems = [
  { id: 'dashboard', path: '/', labelKey: 'dashboard', icon: LayoutDashboard },
  { id: 'tasks', path: '/tasks', labelKey: 'tasks', icon: CheckSquare },
  { id: 'notes', path: '/notes', labelKey: 'notes', icon: FileText },
  { id: 'clipboard', path: '/clipboard', labelKey: 'clipboard', icon: Scissors },
  { id: 'pomodoro', path: '/pomodoro', labelKey: 'pomodoro', icon: Timer },
  { id: 'schedule', path: '/schedule', labelKey: 'schedule', icon: Calendar },
  { id: 'mysql', path: '/mysql', labelKey: 'mysql', icon: Database },
  { id: 'alarm', path: '/alarm', labelKey: 'alarm', icon: AlarmClock },
  { id: 'ai', path: '/ai', labelKey: 'ai', icon: Sparkles },
  { id: 'settings', path: '/settings', labelKey: 'settings', icon: Settings },
]

const navItems = computed(() => {
  void version.value
  return allItems
    .filter((item) => isRouteEnabled(item.id))
    .map((item) => ({
      ...item,
      label: t(`nav.${item.labelKey}`),
    }))
})
</script>

<template>
  <aside class="w-56 h-full bg-card border-r flex flex-col">
    <div class="px-4 py-4 border-b">
      <div class="flex items-center gap-2.5">
        <div class="w-8 h-8 rounded-xl bg-brand-gradient flex items-center justify-center text-white font-bold text-sm shadow-sm shadow-primary/25">N</div>
        <span class="font-semibold text-sm tracking-tight">Nalu</span>
        <span class="text-[10px] text-muted-foreground ml-auto tabular-nums">v{{ appVersion }}</span>
      </div>
    </div>
    <nav class="flex-1 px-2.5 py-3 space-y-0.5 overflow-y-auto">
      <button
        v-for="item in navItems"
        :key="item.id"
        class="group w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors relative"
        :class="route.name === item.id ? 'bg-accent text-accent-foreground font-medium' : 'text-muted-foreground hover:bg-secondary hover:text-secondary-foreground'"
        @click="router.push(item.path)"
      >
        <span
          v-if="route.name === item.id"
          class="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-full bg-brand-gradient"
        />
        <component :is="item.icon" class="w-4 h-4 shrink-0" />
        <span>{{ item.label }}</span>
      </button>
    </nav>
    <div class="px-2.5 py-3 border-t">
      <button class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-xs text-muted-foreground hover:bg-secondary hover:text-secondary-foreground transition-colors" @click="emit('command')">
        <Command class="w-3.5 h-3.5" />
        <span>Cmd+K</span>
      </button>
    </div>
  </aside>
</template>
