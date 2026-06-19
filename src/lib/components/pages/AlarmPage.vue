<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { AlarmClock, Bell, BellOff, Clock, FolderOpen, Play, Plus, SkipForward, Trash2, Volume2 } from 'lucide-vue-next'
import type { Alarm } from '$lib/types'
import { useI18n } from '$lib/i18n'
import { Input } from '$lib/components/ui/input'
import { useAiRefresh } from '$lib/composables/useAiRefresh'
import { useSettingsStore, type SoundChoice } from '$lib/stores/settingsStore'
import { PRESET_ALERT_SOUNDS, playAlertChime } from '$lib/utils/alertSound'

const { t } = useI18n()
const settings = useSettingsStore()
const alarms = ref<Alarm[]>([])
const newTime = ref('08:00')
const newLabel = ref('')
const newRepeat = ref('none')
const newSoundChoice = ref<SoundChoice | null>(null)
const quickTimes = ['07:30', '08:00', '09:00', '22:30']

interface CopiedSound {
  path: string
  name: string
}

const activeAlarms = computed(() => alarms.value.filter((alarm) => alarm.active))
const skippedAlarms = computed(() => alarms.value.filter((alarm) => alarm.active && alarm.skip_next))
const nextAlarm = computed(() => {
  return alarms.value
    .filter((alarm) => alarm.active && !alarm.skip_next)
    .map((alarm) => ({ alarm, date: nextAlarmDate(alarm) }))
    .filter((item): item is { alarm: Alarm; date: Date } => !!item.date)
    .sort((a, b) => a.date.getTime() - b.date.getTime())[0]
})

async function loadAlarms() {
  try {
    alarms.value = await invoke('get_alarms')
  } catch (error) {
    console.error(error)
  }
}

async function addAlarm() {
  if (!newTime.value) return
  await invoke('add_alarm', { time: newTime.value, label: newLabel.value.trim(), repeat: newRepeat.value, sound: encodeSound(newSoundChoice.value) })
  newLabel.value = ''
  newRepeat.value = 'none'
  newSoundChoice.value = null
  await loadAlarms()
}

async function skipNextAlarm(id: string) {
  await invoke('skip_next_alarm', { id })
  await loadAlarms()
}

async function toggleAlarm(id: string) {
  await invoke('toggle_alarm', { id })
  await loadAlarms()
}

async function deleteAlarm(id: string) {
  await invoke('delete_alarm', { id })
  await loadAlarms()
}

async function updateAlarmSound(id: string, choice: SoundChoice | null) {
  await invoke('update_alarm_sound', { id, sound: encodeSound(choice) })
  await loadAlarms()
}

function encodeSound(choice: SoundChoice | null) {
  return choice ? JSON.stringify(choice) : null
}

function decodeSound(sound?: string | null): SoundChoice | null {
  if (!sound) return null
  try {
    const parsed = JSON.parse(sound) as SoundChoice
    if (parsed.type === 'synth') return parsed
    if (parsed.type === 'preset' && parsed.id) return parsed
    if (parsed.type === 'custom' && parsed.path) return parsed
  } catch (error) {
    console.warn('[AlarmPage] invalid alarm sound:', error)
  }
  return null
}

function soundValue(choice: SoundChoice | null) {
  if (!choice) return 'default'
  if (choice.type === 'preset') return choice.id
  return choice.type
}

function soundName(choice: SoundChoice | null) {
  if (!choice) return t('sound.defaultAlarm')
  if (choice.type === 'custom') return choice.name
  if (choice.type === 'synth') return t('sound.synth')
  const preset = PRESET_ALERT_SOUNDS.find((item) => item.id === choice.id)
  return preset ? t(preset.labelKey) : t('sound.defaultAlarm')
}

function alarmSoundChoice(alarm: Alarm) {
  return decodeSound(alarm.sound)
}

function choiceFromValue(value: string): SoundChoice | null {
  if (value === 'default') return null
  if (value === 'synth') return { type: 'synth' }
  return { type: 'preset', id: value }
}

function onNewSoundSelect(event: Event) {
  const value = event.target instanceof HTMLSelectElement ? event.target.value : 'default'
  newSoundChoice.value = choiceFromValue(value)
}

async function onAlarmSoundSelect(alarm: Alarm, event: Event) {
  const value = event.target instanceof HTMLSelectElement ? event.target.value : 'default'
  await updateAlarmSound(alarm.id, choiceFromValue(value))
}

async function pickCustomSound() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'ogg', 'm4a', 'aac', 'flac'] }],
  })
  if (typeof selected !== 'string') return null
  return invoke<CopiedSound>('copy_custom_sound', { path: selected })
}

async function chooseCustomSoundForNew() {
  const copied = await pickCustomSound()
  if (!copied) return
  newSoundChoice.value = { type: 'custom', path: copied.path, name: copied.name }
}

async function chooseCustomSoundForAlarm(id: string) {
  const copied = await pickCustomSound()
  if (!copied) return
  await updateAlarmSound(id, { type: 'custom', path: copied.path, name: copied.name })
}

function previewNewSound() {
  playAlertChime(newSoundChoice.value ?? settings.soundSettings.alarm, settings.soundSettings.volume)
}

function previewAlarmSound(alarm: Alarm) {
  playAlertChime(alarmSoundChoice(alarm) ?? settings.soundSettings.alarm, settings.soundSettings.volume)
}

function setQuickTime(time: string) {
  newTime.value = time
}

function parseAlarmTime(time: string) {
  const [hours, minutes] = time.split(':').map(Number)
  if (!Number.isFinite(hours) || !Number.isFinite(minutes)) return null
  return { hours, minutes }
}

function repeatMatchesDate(repeat: string, date: Date) {
  const day = date.getDay()
  if (repeat === 'weekdays') return day >= 1 && day <= 5
  if (repeat === 'weekends') return day === 0 || day === 6
  return true
}

function nextAlarmDate(alarm: Alarm) {
  const parsed = parseAlarmTime(alarm.time)
  if (!parsed) return null
  const now = new Date()
  for (let offset = 0; offset <= 7; offset += 1) {
    const candidate = new Date(now)
    candidate.setDate(now.getDate() + offset)
    candidate.setHours(parsed.hours, parsed.minutes, 0, 0)
    if (!repeatMatchesDate(alarm.repeat, candidate)) continue
    if (candidate.getTime() > now.getTime()) return candidate
  }
  return null
}

function dayLabel(date: Date) {
  const now = new Date()
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const target = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
  const diff = Math.round((target - today) / 86400000)
  if (diff === 0) return t('alarm.today')
  if (diff === 1) return t('alarm.tomorrow')
  return `${date.getMonth() + 1}/${date.getDate()}`
}

function nextLabel(alarm: Alarm) {
  if (!alarm.active) return t('alarm.disabled')
  if (alarm.skip_next) return t('alarm.skippedOnce')
  const date = nextAlarmDate(alarm)
  if (!date) return t('alarm.noNext')
  return `${dayLabel(date)} ${alarm.time}`
}

onMounted(loadAlarms)
useAiRefresh(loadAlarms)
</script>

<template>
  <div class="max-w-5xl mx-auto px-4 py-6 sm:px-6 sm:py-8">
    <header class="mb-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
      <div>
        <h1 class="text-2xl font-bold flex items-center gap-2">
          <AlarmClock class="w-6 h-6 text-primary" />
          {{ t('alarm.title') }}
        </h1>
        <p class="mt-1 text-sm text-muted-foreground">{{ t('alarm.subtitle') }}</p>
      </div>
      <div class="flex gap-2 text-xs text-muted-foreground">
        <span class="rounded-full bg-secondary px-3 py-1">{{ activeAlarms.length }} {{ t('alarm.activeCount') }}</span>
        <span class="rounded-full bg-secondary px-3 py-1">{{ skippedAlarms.length }} {{ t('alarm.skippedCount') }}</span>
      </div>
    </header>

    <section class="mb-6 grid gap-3 md:grid-cols-[1.2fr_1fr]">
      <div class="rounded-lg border bg-card p-4">
        <div class="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
          <Clock class="w-4 h-4" />
          {{ t('alarm.nextRing') }}
        </div>
        <div class="mt-3 text-3xl font-mono font-bold">
          {{ nextAlarm ? nextAlarm.alarm.time : '--:--' }}
        </div>
        <p class="mt-1 text-sm text-muted-foreground">
          {{ nextAlarm ? `${dayLabel(nextAlarm.date)} · ${nextAlarm.alarm.label || t('alarm.untitled')}` : t('alarm.noNext') }}
        </p>
      </div>

      <form class="rounded-lg border bg-card p-4" @submit.prevent="addAlarm">
        <div class="mb-3 flex items-center justify-between gap-3">
          <h2 class="text-sm font-semibold">{{ t('alarm.setAlarm') }}</h2>
          <div class="flex flex-wrap justify-end gap-1">
            <button
              v-for="time in quickTimes"
              :key="time"
              type="button"
              class="rounded-md bg-secondary px-2.5 py-1 text-xs text-secondary-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
              @click="setQuickTime(time)"
            >
              {{ time }}
            </button>
          </div>
        </div>
        <div class="grid gap-3 sm:grid-cols-[120px_minmax(0,1fr)]">
          <label>
            <span class="block text-xs text-muted-foreground mb-1">{{ t('alarm.time') }}</span>
            <Input v-model="newTime" type="time" class="w-full" />
          </label>
          <label>
            <span class="block text-xs text-muted-foreground mb-1">{{ t('alarm.label') }}</span>
            <Input v-model="newLabel" type="text" class="w-full" :placeholder="t('alarm.labelPlaceholder')" />
          </label>
          <label class="sm:col-span-2">
            <span class="block text-xs text-muted-foreground mb-1">{{ t('alarm.repeat') }}</span>
            <select v-model="newRepeat" class="h-10 w-full rounded-md border bg-background px-3 text-sm">
              <option value="none">{{ t('alarm.repeatOptions.none') }}</option>
              <option value="daily">{{ t('alarm.repeatOptions.daily') }}</option>
              <option value="weekdays">{{ t('alarm.repeatOptions.weekdays') }}</option>
              <option value="weekends">{{ t('alarm.repeatOptions.weekends') }}</option>
            </select>
          </label>
          <div class="sm:col-span-2">
            <span class="block text-xs text-muted-foreground mb-1">{{ t('alarm.sound') }}</span>
            <div class="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_auto]">
              <select
                class="h-10 w-full rounded-md border bg-background px-3 text-sm"
                :value="soundValue(newSoundChoice)"
                @change="onNewSoundSelect"
              >
                <option value="default">{{ t('sound.defaultAlarm') }}</option>
                <option v-if="newSoundChoice?.type === 'custom'" value="custom" disabled>{{ newSoundChoice.name }}</option>
                <option v-for="sound in PRESET_ALERT_SOUNDS" :key="sound.id" :value="sound.id">{{ t(sound.labelKey) }}</option>
              </select>
              <button type="button" class="inline-flex h-10 items-center justify-center gap-1.5 rounded-md bg-secondary px-3 text-sm transition-colors hover:bg-secondary/70" @click="chooseCustomSoundForNew">
                <FolderOpen class="w-4 h-4" />
                {{ t('sound.chooseCustom') }}
              </button>
              <button type="button" class="inline-flex h-10 items-center justify-center gap-1.5 rounded-md bg-secondary px-3 text-sm transition-colors hover:bg-secondary/70" @click="previewNewSound">
                <Play class="w-4 h-4" />
                {{ t('sound.preview') }}
              </button>
            </div>
          </div>
          <button type="submit" class="sm:col-span-2 inline-flex h-10 items-center justify-center gap-1.5 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90">
            <Plus class="w-4 h-4" />
            {{ t('common.add') }}
          </button>
        </div>
      </form>
    </section>

    <section>
      <div class="mb-3 flex items-center justify-between">
        <h2 class="text-sm font-semibold">{{ t('alarm.allAlarms') }}</h2>
        <span class="text-xs text-muted-foreground">{{ alarms.length }} {{ t('alarm.totalCount') }}</span>
      </div>
      <div class="grid gap-2">
        <div
          v-for="alarm in alarms"
          :key="alarm.id"
          class="grid gap-3 rounded-lg border bg-card p-4 sm:grid-cols-[132px_minmax(0,1fr)_auto] sm:items-center"
          :class="{ 'opacity-60': !alarm.active }"
        >
          <div class="font-mono text-4xl font-bold leading-none">{{ alarm.time }}</div>
          <div class="min-w-0">
            <div class="truncate text-sm font-medium">{{ alarm.label || t('alarm.untitled') }}</div>
            <div class="mt-1 flex flex-wrap gap-1.5 text-xs">
              <span class="rounded-full bg-secondary px-2 py-1 text-muted-foreground">{{ t(`alarm.repeatOptions.${alarm.repeat}`) }}</span>
              <span
                class="rounded-full px-2 py-1"
                :class="alarm.active ? 'bg-accent text-accent-foreground' : 'bg-secondary text-muted-foreground'"
              >
                {{ nextLabel(alarm) }}
              </span>
              <span class="inline-flex items-center gap-1 rounded-full bg-secondary px-2 py-1 text-muted-foreground">
                <Volume2 class="w-3 h-3" />
                {{ soundName(alarmSoundChoice(alarm)) }}
              </span>
            </div>
          </div>
          <div class="flex flex-wrap items-center justify-end gap-1">
            <select
              class="h-9 max-w-[168px] rounded-md border bg-background px-2 text-xs"
              :value="soundValue(alarmSoundChoice(alarm))"
              :title="t('alarm.sound')"
              @change="onAlarmSoundSelect(alarm, $event)"
            >
              <option value="default">{{ t('sound.defaultAlarm') }}</option>
              <option v-if="alarmSoundChoice(alarm)?.type === 'custom'" value="custom" disabled>{{ soundName(alarmSoundChoice(alarm)) }}</option>
              <option v-for="sound in PRESET_ALERT_SOUNDS" :key="sound.id" :value="sound.id">{{ t(sound.labelKey) }}</option>
            </select>
            <button class="rounded-md p-2 transition-colors hover:bg-secondary" :title="t('sound.chooseCustom')" @click="chooseCustomSoundForAlarm(alarm.id)">
              <FolderOpen class="w-4 h-4 text-muted-foreground" />
            </button>
            <button class="rounded-md p-2 transition-colors hover:bg-secondary" :title="t('sound.preview')" @click="previewAlarmSound(alarm)">
              <Play class="w-4 h-4 text-muted-foreground" />
            </button>
            <button
              v-if="alarm.active"
              class="rounded-md p-2 transition-colors hover:bg-secondary"
              :title="alarm.skip_next ? t('alarm.cancelSkip') : t('alarm.skipNext')"
              @click="skipNextAlarm(alarm.id)"
            >
              <SkipForward class="w-4 h-4" :class="alarm.skip_next ? 'text-warning' : 'text-muted-foreground'" />
            </button>
            <button class="rounded-md p-2 transition-colors hover:bg-secondary" :title="alarm.active ? t('alarm.disable') : t('alarm.enable')" @click="toggleAlarm(alarm.id)">
              <Bell v-if="alarm.active" class="w-4 h-4 text-primary" />
              <BellOff v-else class="w-4 h-4 text-muted-foreground" />
            </button>
            <button class="rounded-md p-2 text-muted-foreground/60 transition-colors hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-500/10" :title="t('common.delete')" @click="deleteAlarm(alarm.id)">
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        </div>
        <div v-if="alarms.length === 0" class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">{{ t('alarm.noAlarms') }}</div>
      </div>
    </section>
  </div>
</template>
