<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { CheckSquare, FileText, Calendar, Scissors, Timer, Database, AlarmClock, Settings, Circle, Radio, Copy, Type, Image as ImageIcon, Sparkles, Play, Pause, Bell, BellOff, Clock, ChevronRight, Volume2 } from 'lucide-vue-next'
import type { Task, Note, Schedule, ClipboardEntry, Alarm, PomodoroState } from '$lib/types'
import { useClipboardStore } from '$lib/stores/clipboardStore'
import { useSettingsStore } from '$lib/stores/settingsStore'
import { useI18n } from '$lib/i18n'
import AiChatWidget from '$lib/components/AiChatWidget.vue'
import { useAiRefresh } from '$lib/composables/useAiRefresh'
import { useMobile } from '$lib/composables/useMobile'
import { playAlertChime } from '$lib/utils/alertSound'

const router = useRouter()
const { t } = useI18n()
const { isMobile, isMobilePlatform, isRouteEnabled } = useMobile()
const clipboard = useClipboardStore()
const settings = useSettingsStore()
const { monitoring } = storeToRefs(clipboard)
const { soundSettings } = storeToRefs(settings)
const tasks = ref<Task[]>([])
const editingId = ref<string | null>(null)
const editTitle = ref('')
const notes = ref<Note[]>([])
const schedules = ref<Schedule[]>([])
const entries = ref<ClipboardEntry[]>([])
const alarms = ref<Alarm[]>([])
const pomodoro = ref<PomodoroState | null>(null)
const pendingTasks = computed(() => tasks.value.filter((task) => !task.done))
const doneTasks = computed(() => tasks.value.filter((task) => task.done))
const animatedPendingCount = ref(0)
const soundVolumePercent = computed(() => Math.round(soundSettings.value.volume * 100))
let interval: ReturnType<typeof setInterval>
let countTimer: ReturnType<typeof setInterval> | null = null

function vibrate(pattern: number | number[]) {
  if (!isMobilePlatform.value || typeof navigator === 'undefined' || !navigator.vibrate) return
  try { navigator.vibrate(pattern) } catch {}
}

const quickNav = [
  ['tasks', 'nav.tasks', CheckSquare, 'text-blue-500'], ['notes', 'nav.notes', FileText, 'text-green-500'], ['clipboard', 'nav.clipboard', Scissors, 'text-purple-500'], ['pomodoro', 'nav.pomodoro', Timer, 'text-red-500'],
  ['schedule', 'nav.schedule', Calendar, 'text-orange-500'], ['mysql', 'nav.mysql', Database, 'text-cyan-500'], ['alarm', 'nav.alarm', AlarmClock, 'text-yellow-500'], ['ai', 'nav.ai', Sparkles, 'text-violet-500'],
  ['settings', 'nav.settings', Settings, 'text-slate-500']
] as const

const pomodoroDisplay = computed(() => {
  const state = pomodoro.value
  if (!state) return null
  const mins = Math.floor(state.remaining_seconds / 60)
  const secs = state.remaining_seconds % 60
  const duration = state.is_break ? state.break_duration : state.work_duration
  const progress = duration ? 1 - state.remaining_seconds / duration : 0
  return {
    time: `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`,
    label: state.is_break ? t('pomodoro.break') : t('pomodoro.focus'),
    isRunning: state.is_running,
    isBreak: state.is_break,
    progress,
    completed: state.completed_count
  }
})

const upcomingSchedules = computed(() => {
  const now = Date.now()
  return schedules.value
    .filter((schedule) => !schedule.done && new Date(schedule.scheduled_at).getTime() > now)
    .sort((a, b) => new Date(a.scheduled_at).getTime() - new Date(b.scheduled_at).getTime())
    .slice(0, 4)
})

const nextAlarms = computed(() => {
  return alarms.value.filter((alarm) => alarm.active).slice(0, 4)
})

function formatScheduleTime(value: string) {
  const date = new Date(value)
  const now = new Date()
  const isToday = date.toDateString() === now.toDateString()
  const tomorrow = new Date(now)
  tomorrow.setDate(tomorrow.getDate() + 1)
  const isTomorrow = date.toDateString() === tomorrow.toDateString()
  const time = `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
  if (isToday) return `${t('dashboardExt.today')} ${time}`
  if (isTomorrow) return `${t('dashboardExt.tomorrow')} ${time}`
  return `${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')} ${time}`
}

function scheduleDiff(value: string) {
  const diff = new Date(value).getTime() - Date.now()
  const minutes = Math.floor(diff / 60000)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  return `${Math.floor(hours / 24)}d`
}

async function loadData() {
  try { tasks.value = await invoke('get_tasks') } catch {}
  try { notes.value = await invoke('get_notes') } catch {}
  try { schedules.value = await invoke('get_schedules') } catch {}
  try { entries.value = await invoke('get_clipboard_history', { limit: 5 }) } catch {}
  try { alarms.value = await invoke('get_alarms') } catch {}
  try { pomodoro.value = await invoke('pomodoro_get_state') } catch {}
}

async function toggleTask(id: string) {
  vibrate(10)
  await invoke('toggle_task', { id })
  await loadData()
}

function startEdit(task: Task) {
  editingId.value = task.id
  editTitle.value = task.title
  vibrate(8)
  nextTick(() => {
    const input = document.querySelector(`[data-dashboard-task-input="${task.id}"]`) as HTMLInputElement
    input?.focus()
    input?.select()
  })
}

async function saveEdit() {
  if (!editingId.value || !editTitle.value.trim()) return
  await invoke('update_task', { id: editingId.value, title: editTitle.value.trim() })
  editingId.value = null
  await loadData()
}

function cancelEdit() {
  editingId.value = null
}

function onEditKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') saveEdit()
  else if (e.key === 'Escape') cancelEdit()
}

async function togglePomodoro() {
  if (pomodoro.value?.is_running) {
    pomodoro.value = await invoke('pomodoro_pause')
  } else {
    await invoke('pomodoro_start')
    pomodoro.value = await invoke('pomodoro_get_state')
  }
}

async function toggleAlarm(id: string) {
  await invoke('toggle_alarm', { id })
  alarms.value = await invoke('get_alarms')
}

function setDashboardSoundVolume(event: Event) {
  const value = event.target instanceof HTMLInputElement ? Number(event.target.value) : soundVolumePercent.value
  soundSettings.value.volume = Math.min(1, Math.max(0, value / 100))
  settings.saveSoundSettings()
}

function previewDashboardSound() {
  playAlertChime(soundSettings.value.alarm, soundSettings.value.volume)
}

onMounted(async () => {
  await loadData()
  interval = setInterval(loadData, 5000)
})
onBeforeUnmount(() => clearInterval(interval))
onBeforeUnmount(() => {
  if (countTimer) clearInterval(countTimer)
})
useAiRefresh(loadData)

watch(() => pendingTasks.value.length, (next) => {
  if (countTimer) clearInterval(countTimer)
  const start = animatedPendingCount.value
  const diff = next - start
  if (diff === 0) return
  const steps = Math.min(12, Math.max(4, Math.abs(diff) * 4))
  let step = 0
  countTimer = setInterval(() => {
    step += 1
    animatedPendingCount.value = Math.round(start + diff * (step / steps))
    if (step >= steps && countTimer) {
      clearInterval(countTimer)
      countTimer = null
      animatedPendingCount.value = next
    }
  }, 28)
}, { immediate: true })
</script>

<template>
  <div class="max-w-4xl mx-auto px-6 py-8">
    <header class="mb-8">
      <h1 class="text-2xl font-bold tracking-tight">{{ t('dashboard.title') }}</h1>
      <p class="text-sm text-muted-foreground mt-1">{{ t('dashboard.welcome') }}, Nalomu</p>
    </header>

    <!-- Quick controls -->
    <section class="mb-8">
      <div class="mb-3 flex items-center justify-between gap-3">
        <h2 class="text-xs font-semibold text-muted-foreground uppercase tracking-wider">{{ t('dashboardExt.quickControls') }}</h2>
        <button class="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-primary" @click="router.push('/settings')">
          {{ t('dashboardExt.allSettings') }}
          <ChevronRight class="w-3.5 h-3.5" />
        </button>
      </div>
      <div class="grid gap-3 lg:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]">
        <div class="rounded-xl border bg-card p-4">
          <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div class="min-w-0">
              <div class="flex items-center gap-2 text-sm font-semibold">
                <Volume2 class="w-4 h-4 text-primary" />
                {{ t('sound.volume') }}
              </div>
              <p class="mt-1 text-xs text-muted-foreground">{{ t('dashboardExt.soundControlDesc') }}</p>
            </div>
            <div class="font-mono text-2xl font-bold tabular-nums">{{ soundVolumePercent }}%</div>
          </div>
          <div class="mt-4 grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
            <input
              type="range"
              min="0"
              max="100"
              step="5"
              class="h-2 w-full accent-primary"
              :value="soundVolumePercent"
              :aria-label="t('sound.volume')"
              @input="setDashboardSoundVolume"
            />
            <button class="inline-flex h-9 items-center justify-center gap-1.5 rounded-lg bg-secondary px-3 text-sm transition-colors hover:bg-secondary/70" @click="previewDashboardSound">
              <Play class="w-3.5 h-3.5" />
              {{ t('sound.preview') }}
            </button>
          </div>
        </div>

        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-1">
          <button v-if="!isMobile" class="flex items-center justify-between gap-3 rounded-xl border bg-card p-4 text-left transition-colors hover:border-primary/40" @click="clipboard.toggleMonitoring">
            <div>
              <div class="flex items-center gap-2 text-sm font-semibold">
                <Radio class="w-4 h-4" :class="monitoring ? 'text-success' : 'text-muted-foreground'" />
                {{ t('dashboardExt.clipboardStatus') }}
              </div>
              <p class="mt-1 text-xs text-muted-foreground">{{ monitoring ? t('dashboardExt.clipboardMonitoring') : t('dashboardExt.clipboardOff') }}</p>
            </div>
            <span class="relative h-6 w-11 rounded-full" :class="monitoring ? 'bg-primary' : 'bg-input'">
              <span class="absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform" :class="{ 'translate-x-5': monitoring }" />
            </span>
          </button>

          <button class="flex items-center justify-between gap-3 rounded-xl border bg-card p-4 text-left transition-colors hover:border-primary/40" @click="togglePomodoro">
            <div>
              <div class="flex items-center gap-2 text-sm font-semibold">
                <Timer class="w-4 h-4" :class="pomodoroDisplay?.isRunning ? 'text-primary' : 'text-muted-foreground'" />
                {{ t('nav.pomodoro') }}
              </div>
              <p class="mt-1 text-xs text-muted-foreground">{{ pomodoroDisplay?.isRunning ? t('pomodoro.running') : t('pomodoro.paused') }}</p>
            </div>
            <span class="rounded-full bg-secondary p-2">
              <Pause v-if="pomodoroDisplay?.isRunning" class="w-4 h-4 text-muted-foreground" />
              <Play v-else class="w-4 h-4 text-muted-foreground" />
            </span>
          </button>
        </div>
      </div>
    </section>

    <!-- Quick nav -->
    <section class="mb-8">
      <h2 class="text-xs font-semibold text-muted-foreground mb-3 uppercase tracking-wider">{{ t('dashboardExt.quickNav') }}</h2>
      <div class="grid grid-cols-3 sm:grid-cols-4 gap-2.5">
        <button v-for="[id, label, icon, color] in quickNav.filter(([id]) => isRouteEnabled(id))" :key="id" class="flex flex-col items-center gap-2 px-3 py-3.5 rounded-xl bg-card border cursor-pointer transition-all duration-200 hover:border-primary/40 hover:shadow-sm hover:-translate-y-0.5 active:translate-y-0 active:shadow-none" @click="router.push(`/${id}`)">
          <component :is="icon" class="w-5 h-5" :class="color" />
          <span class="text-xs text-muted-foreground">{{ t(label) }}</span>
        </button>
      </div>
    </section>

    <!-- Time-critical row: Pomodoro + Next Schedule (desktop only pomodoro) -->
    <section class="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-6">
      <button
        v-if="!isMobile"
        class="text-left bg-card rounded-xl p-4 border hover:shadow-sm transition relative overflow-hidden"
        @click="router.push('/pomodoro')"
      >
        <div
          v-if="pomodoroDisplay"
          class="absolute inset-x-0 bottom-0 h-1"
          :class="pomodoroDisplay.isBreak ? 'bg-success/20' : 'bg-destructive/20'"
        >
          <div
            class="h-full transition-all"
            :class="pomodoroDisplay.isBreak ? 'bg-success' : 'bg-destructive'"
            :style="{ width: `${(pomodoroDisplay.progress * 100).toFixed(1)}%` }"
          />
        </div>
        <div class="flex items-center justify-between mb-2">
          <div class="flex items-center gap-1.5 text-xs font-medium" :class="pomodoroDisplay?.isBreak ? 'text-success' : 'text-destructive'">
            <Timer class="w-3.5 h-3.5" />
            {{ t('nav.pomodoro') }}
          </div>
          <button
            v-if="pomodoroDisplay"
            class="p-1 rounded-md hover:bg-secondary"
            @click.stop="togglePomodoro"
          >
            <Pause v-if="pomodoroDisplay.isRunning" class="w-3.5 h-3.5 text-muted-foreground" />
            <Play v-else class="w-3.5 h-3.5 text-muted-foreground" />
          </button>
        </div>
        <div class="text-3xl font-mono font-bold tabular-nums">{{ pomodoroDisplay?.time ?? '25:00' }}</div>
        <div class="text-xs text-muted-foreground mt-1">
          {{ pomodoroDisplay?.label ?? t('pomodoro.focus') }} ·
          {{ pomodoroDisplay?.completed ?? 0 }} {{ t('pomodoro.completed') }}
        </div>
      </button>

      <button
        v-if="!isMobile"
        class="text-left bg-card rounded-xl p-4 border hover:shadow-sm transition"
        @click="router.push('/schedule')"
      >
        <div class="flex items-center justify-between mb-2">
          <div class="flex items-center gap-1.5 text-orange-500 text-xs font-medium">
            <Calendar class="w-3.5 h-3.5" />
            {{ t('dashboard.upcoming') }}
          </div>
          <ChevronRight class="w-3.5 h-3.5 text-muted-foreground/50" />
        </div>
        <div v-if="upcomingSchedules.length === 0" class="text-sm text-muted-foreground py-4">
          {{ t('schedule.noEvents') }}
        </div>
        <div v-else class="space-y-1.5">
          <div
            v-for="schedule in upcomingSchedules.slice(0, 3)"
            :key="schedule.id"
            class="flex items-center gap-2 text-xs"
          >
            <Clock class="w-3 h-3 text-muted-foreground shrink-0" />
            <span class="truncate font-medium">{{ schedule.title }}</span>
            <span class="ml-auto text-muted-foreground shrink-0 tabular-nums">{{ scheduleDiff(schedule.scheduled_at) }}</span>
          </div>
        </div>
      </button>
    </section>

    <!-- Alarms row -->
    <section v-if="!isMobile" class="mb-6">
      <div class="flex justify-between items-center mb-3">
        <h2 class="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
          <AlarmClock class="w-3.5 h-3.5" />
          {{ t('nav.alarm') }}
        </h2>
        <button class="text-xs text-muted-foreground hover:text-primary transition-colors" @click="router.push('/alarm')">{{ t('alarm.allAlarms') }}</button>
      </div>
      <div v-if="nextAlarms.length === 0" class="text-center py-4 text-muted-foreground text-xs bg-card rounded-xl border">
        {{ t('alarm.noAlarms') }}
      </div>
      <div v-else class="grid grid-cols-2 gap-2.5">
        <div
          v-for="alarm in nextAlarms"
          :key="alarm.id"
          class="flex items-center gap-3 px-3 py-2.5 rounded-xl bg-card border"
        >
          <div class="text-xl font-mono font-bold tabular-nums">{{ alarm.time }}</div>
          <div class="flex-1 min-w-0">
            <div class="text-xs font-medium truncate">{{ alarm.label || t('alarm.title') }}</div>
            <div class="text-[10px] text-muted-foreground">{{ t(`alarm.repeatOptions.${alarm.repeat}`) }}</div>
          </div>
          <button @click="toggleAlarm(alarm.id)">
            <Bell v-if="alarm.active" class="w-3.5 h-3.5 text-primary" />
            <BellOff v-else class="w-3.5 h-3.5 text-muted-foreground" />
          </button>
        </div>
      </div>
    </section>

    <!-- Counts row -->
    <div class="grid gap-4 mb-8" :class="isMobile ? 'grid-cols-2' : 'grid-cols-3'">
      <button class="text-left bg-card rounded-xl p-4 border hover:shadow-sm transition" @click="router.push('/tasks')">
        <div class="text-blue-500 text-xs font-medium mb-2">{{ t('nav.tasks') }}</div>
        <div class="text-3xl font-bold tabular-nums transition-transform duration-150" :key="animatedPendingCount">{{ animatedPendingCount }}</div>
        <div class="text-xs text-muted-foreground">{{ doneTasks.length }} {{ t('dashboard.completed') }}</div>
      </button>
      <button class="text-left bg-card rounded-xl p-4 border hover:shadow-sm transition" @click="router.push('/notes')">
        <div class="text-green-500 text-xs font-medium mb-2">{{ t('nav.notes') }}</div>
        <div class="text-3xl font-bold tabular-nums">{{ notes.length }}</div>
      </button>
      <button v-if="!isMobile" class="text-left bg-card rounded-xl p-4 border hover:shadow-sm transition" @click="router.push('/schedule')">
        <div class="text-orange-500 text-xs font-medium mb-2">{{ t('dashboard.upcoming') }}</div>
        <div class="text-3xl font-bold tabular-nums">{{ schedules.filter(item => !item.done).length }}</div>
      </button>
    </div>

    <!-- Clipboard -->
    <section v-if="!isMobile" class="mb-6">
      <div class="flex justify-between mb-3">
        <h2 class="text-xs font-semibold text-muted-foreground uppercase tracking-wider">{{ t('dashboardExt.clipboardStatus') }}</h2>
        <div class="flex items-center gap-3">
          <button class="text-xs text-muted-foreground hover:text-primary transition-colors" @click="router.push('/clipboard')">{{ t('dashboardExt.openClipboardPage') }}</button>
          <button class="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-primary transition-colors" @click="clipboard.toggleMonitoring">
            <Radio class="w-3 h-3" :class="monitoring ? 'text-success' : ''" />
            {{ monitoring ? t('dashboardExt.clipboardMonitoring') : t('dashboardExt.clipboardOff') }}
          </button>
        </div>
      </div>
      <div v-for="entry in entries" :key="entry.id" class="group flex items-center gap-3 px-3 py-2 rounded-xl bg-card border mb-1.5 cursor-pointer hover:border-primary/40 transition-colors" @click="writeText(entry.content)">
        <ImageIcon v-if="entry.content_type.startsWith('image')" class="w-4 h-4 text-purple-400" />
        <FileText v-else-if="entry.content_type === 'file'" class="w-4 h-4 text-amber-500" />
        <Type v-else class="w-4 h-4 text-blue-400" />
        <span class="text-sm flex-1 truncate">{{ entry.content }}</span>
        <Copy class="w-3.5 h-3.5 opacity-0 group-hover:opacity-100 text-muted-foreground transition-opacity" />
      </div>
    </section>

    <!-- Tasks -->
    <section class="mb-6">
      <h2 class="text-xs font-semibold text-muted-foreground mb-3 uppercase tracking-wider">{{ t('dashboard.recentTasks') }}</h2>
      <TransitionGroup name="task-row" tag="div">
        <div v-for="task in tasks.slice(0, 5)" :key="task.id" class="flex items-center gap-3 px-3 py-2 rounded-xl bg-card border mb-1.5 transition-all duration-200">
          <button class="grid h-8 w-8 place-items-center rounded-lg transition-colors active:bg-secondary" @click="toggleTask(task.id)">
            <CheckSquare v-if="task.done" class="w-4 h-4 text-success transition-all duration-200 scale-110" />
            <Circle v-else class="w-4 h-4 text-muted-foreground/40 transition-all duration-200" />
          </button>
          <input
            v-if="editingId === task.id"
            v-model="editTitle"
            :data-dashboard-task-input="task.id"
            class="text-sm flex-1 bg-transparent border-b border-primary outline-none px-1"
            @keydown="onEditKeydown"
            @blur="saveEdit"
          />
          <span
            v-else
            class="text-sm flex-1 cursor-text transition-colors"
            :class="{ 'line-through text-muted-foreground': task.done }"
            @click="startEdit(task)"
          >{{ task.title }}</span>
        </div>
      </TransitionGroup>
    </section>

    <!-- Notes -->
    <section class="mb-6">
      <h2 class="text-xs font-semibold text-muted-foreground mb-3 uppercase tracking-wider">{{ t('dashboard.recentNotes') }}</h2>
      <div class="grid grid-cols-2 gap-2.5">
        <div v-for="note in notes.slice(0, 4)" :key="note.id" class="bg-card rounded-xl p-3 border">
          <div class="font-medium text-sm truncate">{{ note.title }}</div>
          <div class="text-xs text-muted-foreground line-clamp-2">{{ note.content || t('notes.empty') }}</div>
        </div>
      </div>
    </section>
    <AiChatWidget />
  </div>
</template>

<style scoped>
.task-row-enter-active,
.task-row-leave-active,
.task-row-move {
  transition: opacity 180ms ease, transform 180ms ease;
}

.task-row-enter-from,
.task-row-leave-to {
  opacity: 0;
  transform: translateY(6px) scale(0.98);
}
</style>
