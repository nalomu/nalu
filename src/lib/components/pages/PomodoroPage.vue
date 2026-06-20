<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Eraser, Pause, Play, RotateCcw, SkipForward, Timer } from 'lucide-vue-next'
import type { PomodoroState } from '$lib/types'
import { useI18n } from '$lib/i18n'
import { Input } from '$lib/components/ui/input'
import { useSettingsStore } from '$lib/stores/settingsStore'
import { useMobile } from '$lib/composables/useMobile'
import { POMODORO_STATE_CHANGED_EVENT } from '$lib/utils/pomodoroEvents'

const { t } = useI18n()
const settings = useSettingsStore()
const { isMobilePlatform } = useMobile()
const timerState = ref<PomodoroState>({ is_running: false, is_break: false, remaining_seconds: 1500, work_duration: 1500, break_duration: 300, completed_count: 0 })
const workMinutes = ref(25)
const breakMinutes = ref(5)
const durationPulse = ref(false)
const presets = [
  { work: 25, break: 5 },
  { work: 50, break: 10 },
  { work: 90, break: 15 },
]
const minutes = computed(() => Math.floor(timerState.value.remaining_seconds / 60))
const seconds = computed(() => timerState.value.remaining_seconds % 60)
const progress = computed(() => {
  const duration = timerState.value.is_break ? timerState.value.break_duration : timerState.value.work_duration
  return duration ? Math.min(1, Math.max(0, 1 - timerState.value.remaining_seconds / duration)) : 0
})
const progressPercent = computed(() => Math.round(progress.value * 100))
const phaseLabel = computed(() => timerState.value.is_break ? t('pomodoro.break') : t('pomodoro.focus'))
const phaseDescription = computed(() => timerState.value.is_break ? t('pomodoro.breakDesc') : t('pomodoro.focusDesc'))
const statusLabel = computed(() => timerState.value.is_running ? t('pomodoro.running') : t('pomodoro.paused'))
const totalMinutes = computed(() => Math.round((timerState.value.is_break ? timerState.value.break_duration : timerState.value.work_duration) / 60))
let unlisten: UnlistenFn | undefined
let unlistenWorkEnd: UnlistenFn | undefined
let unlistenBreakEnd: UnlistenFn | undefined
let unlistenStateChanged: UnlistenFn | undefined
let durationPulseTimer: ReturnType<typeof setTimeout> | null = null

function vibrate(pattern: number | number[]) {
  if (!isMobilePlatform.value || typeof navigator === 'undefined' || !navigator.vibrate) return
  try { navigator.vibrate(pattern) } catch {}
}

function pulseDuration() {
  durationPulse.value = false
  if (durationPulseTimer) clearTimeout(durationPulseTimer)
  requestAnimationFrame(() => {
    durationPulse.value = true
    durationPulseTimer = setTimeout(() => {
      durationPulse.value = false
      durationPulseTimer = null
    }, 260)
  })
}

async function loadState() {
  try {
    timerState.value = await invoke('pomodoro_get_state')
  } catch (error) {
    console.error(error)
  }
}

async function start() {
  await invoke('pomodoro_start')
  await loadState()
}

async function pause() { timerState.value = await invoke('pomodoro_pause') }

async function reset() { timerState.value = await invoke('pomodoro_reset') }

async function skip() { timerState.value = await invoke('pomodoro_skip') }

async function resetRounds() { timerState.value = await invoke('pomodoro_reset_rounds') }

async function setDuration() {
  timerState.value = await invoke('pomodoro_set_duration', { workMinutes: workMinutes.value, breakMinutes: breakMinutes.value })
  saveDurations()
  pulseDuration()
  vibrate(8)
}

async function applyPreset(work: number, breakValue: number) {
  workMinutes.value = work
  breakMinutes.value = breakValue
  await setDuration()
}

function saveDurations() {
  try { localStorage.setItem('nalu-pomodoro-durations', JSON.stringify({ work: workMinutes.value, break: breakMinutes.value })) } catch {}
}

function loadSavedDurations() {
  try {
    const saved = localStorage.getItem('nalu-pomodoro-durations')
    if (saved) {
      const parsed = JSON.parse(saved)
      workMinutes.value = Number(parsed.work) || 25
      breakMinutes.value = Number(parsed.break) || 5
    }
  } catch {}
}

onMounted(async () => {
  loadSavedDurations()
  await loadState()
  // Apply saved durations to Rust state
  const w = workMinutes.value * 60
  const b = breakMinutes.value * 60
  if (w !== timerState.value.work_duration || b !== timerState.value.break_duration) {
    await setDuration()
  }
  unlisten = await listen<number>('pomodoro-tick', ({ payload }) => { timerState.value.remaining_seconds = payload })
  unlistenWorkEnd = await listen<number>('pomodoro-work-end', () => { void loadState() })
  unlistenBreakEnd = await listen('pomodoro-break-end', () => { void loadState() })
  unlistenStateChanged = await listen<PomodoroState>(POMODORO_STATE_CHANGED_EVENT, ({ payload }) => {
    if (payload && typeof payload === 'object') {
      timerState.value = payload
    } else {
      void loadState()
    }
  })
})
onBeforeUnmount(() => {
  unlisten?.()
  unlistenWorkEnd?.()
  unlistenBreakEnd?.()
  unlistenStateChanged?.()
  if (durationPulseTimer) clearTimeout(durationPulseTimer)
})
</script>

<template>
  <div class="max-w-5xl mx-auto px-4 py-6 sm:px-6 sm:py-8">
    <header class="mb-6 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
      <div>
        <h1 class="text-2xl font-bold flex items-center gap-2">
          <Timer class="w-6 h-6 text-primary" />
          {{ t('pomodoro.title') }}
        </h1>
        <p class="mt-1 text-sm text-muted-foreground">{{ t('pomodoro.subtitle') }}</p>
      </div>
      <div class="flex gap-2 text-xs">
        <span class="rounded-full bg-accent px-3 py-1 text-accent-foreground">{{ phaseLabel }}</span>
        <span class="rounded-full bg-secondary px-3 py-1 text-muted-foreground">{{ statusLabel }}</span>
      </div>
    </header>

    <section class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
      <div class="rounded-lg border bg-card p-5 sm:p-6">
        <div class="grid gap-6 md:grid-cols-[280px_minmax(0,1fr)] md:items-center">
          <div class="relative mx-auto aspect-square w-64 max-w-full">
            <svg class="h-full w-full -rotate-90" viewBox="0 0 100 100">
              <circle cx="50" cy="50" r="44" fill="none" stroke="currentColor" stroke-width="2" class="text-muted" />
              <circle
                cx="50"
                cy="50"
                r="44"
                fill="none"
                stroke="currentColor"
                stroke-width="4"
                stroke-linecap="round"
                :class="timerState.is_break ? 'text-success' : 'text-primary'"
                :stroke-dasharray="2 * Math.PI * 44"
                :stroke-dashoffset="2 * Math.PI * 44 * (1 - progress)"
              />
            </svg>
            <div class="absolute inset-0 flex flex-col items-center justify-center text-center">
              <span class="font-mono text-5xl font-bold leading-none">{{ String(minutes).padStart(2, '0') }}:{{ String(seconds).padStart(2, '0') }}</span>
              <span class="mt-2 text-xs text-muted-foreground">{{ progressPercent }}% · {{ totalMinutes }} {{ t('pomodoro.min') }}</span>
            </div>
          </div>

          <div class="text-center md:text-left">
            <div class="text-xs font-medium uppercase text-muted-foreground">{{ t('pomodoro.currentPhase') }}</div>
            <h2 class="mt-2 text-3xl font-bold">{{ phaseLabel }}</h2>
            <p class="mt-2 text-sm text-muted-foreground">{{ phaseDescription }}</p>
            <div class="mt-5 grid grid-cols-2 gap-2 text-sm">
              <div class="rounded-lg bg-secondary/60 p-3">
                <div class="text-xs text-muted-foreground">{{ t('pomodoro.completedRounds') }}</div>
                <div class="mt-1 text-2xl font-semibold">{{ timerState.completed_count }}</div>
              </div>
              <div class="rounded-lg bg-secondary/60 p-3 transition-transform duration-200" :class="{ 'duration-pulse': durationPulse }">
                <div class="text-xs text-muted-foreground">{{ t('pomodoro.cycle') }}</div>
                <div class="mt-1 text-2xl font-semibold">{{ workMinutes }}/{{ breakMinutes }}</div>
              </div>
            </div>
          </div>
        </div>

        <div class="mt-6 flex items-center justify-center gap-4">
          <button class="rounded-full p-3 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" :title="t('pomodoro.reset')" @click="reset">
            <RotateCcw class="w-5 h-5" />
          </button>
          <button v-if="timerState.is_running" class="rounded-full bg-primary p-4 text-primary-foreground shadow-lg transition-colors hover:bg-primary/90" :title="t('pomodoro.pause')" @click="pause">
            <Pause class="w-6 h-6" />
          </button>
          <button v-else class="rounded-full bg-primary p-4 text-primary-foreground shadow-lg transition-colors hover:bg-primary/90" :title="t('pomodoro.start')" @click="start">
            <Play class="w-6 h-6" />
          </button>
          <button class="rounded-full p-3 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" :title="t('pomodoro.skip')" @click="skip">
            <SkipForward class="w-5 h-5" />
          </button>
        </div>
      </div>

      <aside class="space-y-3">
        <div class="rounded-lg border bg-card p-4">
          <h2 class="text-sm font-semibold">{{ t('pomodoro.durationSettings') }}</h2>
          <div class="mt-3 grid grid-cols-3 gap-1">
            <button
              v-for="preset in presets"
              :key="`${preset.work}-${preset.break}`"
              type="button"
              class="rounded-md px-2 py-2 text-xs transition-all hover:bg-accent hover:text-accent-foreground"
              :class="workMinutes === preset.work && breakMinutes === preset.break ? 'bg-primary text-primary-foreground shadow-sm shadow-primary/20' : 'bg-secondary'"
              @click="applyPreset(preset.work, preset.break)"
            >
              {{ preset.work }}/{{ preset.break }}
            </button>
          </div>
          <div class="mt-4 grid gap-3">
            <label class="grid grid-cols-[1fr_88px] items-center gap-3 text-sm">
              <span class="text-muted-foreground">{{ t('pomodoro.work') }}</span>
              <Input v-model.number="workMinutes" type="number" min="1" max="120" class="text-center" @change="setDuration" />
            </label>
            <label class="grid grid-cols-[1fr_88px] items-center gap-3 text-sm">
              <span class="text-muted-foreground">{{ t('pomodoro.break') }}</span>
              <Input v-model.number="breakMinutes" type="number" min="1" max="60" class="text-center" @change="setDuration" />
            </label>
          </div>
        </div>

        <button class="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-secondary/70 px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" @click="resetRounds">
          <Eraser class="w-4 h-4" />
          {{ t('pomodoro.resetRounds') }}
        </button>
      </aside>
    </section>
  </div>
</template>

<style scoped>
.duration-pulse {
  animation: duration-pulse 260ms ease;
}

@keyframes duration-pulse {
  0% {
    transform: scale(1);
    box-shadow: 0 0 0 0 hsl(var(--primary) / 0.28);
  }
  45% {
    transform: scale(1.03);
    box-shadow: 0 0 0 6px hsl(var(--primary) / 0.12);
  }
  100% {
    transform: scale(1);
    box-shadow: 0 0 0 0 hsl(var(--primary) / 0);
  }
}
</style>
