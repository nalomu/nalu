<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart'
import { open } from '@tauri-apps/plugin-dialog'
import { mkdir, readFile, writeFile } from '@tauri-apps/plugin-fs'
import { appDataDir, basename, extname, join } from '@tauri-apps/api/path'
import { Save, Globe, Power, Sun, Moon, Monitor, Scissors, Volume2, FolderOpen, Play, Bot, ChevronRight } from 'lucide-vue-next'
import { getVersion } from '@tauri-apps/api/app'
import { useSettingsStore } from '$lib/stores/settingsStore'
import { useClipboardStore } from '$lib/stores/clipboardStore'
import { useSyncStore } from '$lib/stores/syncStore'
import { useI18n, type Locale } from '$lib/i18n'
import { Input } from '$lib/components/ui/input'
import type { ThemeMode } from '$lib/utils/theme'
import { PRESET_ALERT_SOUNDS, playAlertChime } from '$lib/utils/alertSound'
import { useMobile } from '$lib/composables/useMobile'

const settings = useSettingsStore()
const router = useRouter()
const clipboardStore = useClipboardStore()
const syncStore = useSyncStore()
const { locale, theme, aiConfig, clipboardRetention, soundSettings } = storeToRefs(settings)
const { t } = useI18n()
const { isDesktopPlatform, isMobilePlatform } = useMobile()
const autostartEnabled = ref(false)
const aiTestResult = ref('')
const aiTesting = ref(false)
const customDays = ref(clipboardRetention.value.days)
const customCount = ref(clipboardRetention.value.count)
const recordingShortcut = ref(false)
const DEFAULT_SHORTCUT = 'CmdOrCtrl+Shift+V'
type SoundTarget = 'pomodoroStart' | 'pomodoroEnd' | 'alarm'

interface CopiedSound {
  path: string
  name: string
}

function safeSoundFilename(value: string) {
  const fallback = `sound-${Date.now()}`
  const decoded = decodeURIComponent(value.split(/[/?#]/).filter(Boolean).pop() || fallback)
  const cleaned = decoded.replace(/[^\w.\-\u4e00-\u9fa5]+/g, '-').replace(/^-+|-+$/g, '')
  return cleaned || fallback
}

async function copySelectedSoundToAppData(selected: string): Promise<CopiedSound> {
  const data = await readFile(selected)
  const appDir = await appDataDir()
  const soundsDir = await join(appDir, 'sounds')
  await mkdir(soundsDir, { recursive: true })
  const rawName = safeSoundFilename(selected)
  const ext = await extname(rawName)
  const base = ext ? rawName.slice(0, -(ext.length + 1)) : rawName
  const filename = `${base || 'sound'}-${Date.now()}${ext ? `.${ext}` : ''}`
  const target = await join(soundsDir, filename)
  await writeFile(target, data)
  return { path: target, name: await basename(target) }
}

function startRecording() {
  recordingShortcut.value = true
}

function resetShortcut() {
  settings.setClipboardShortcut(DEFAULT_SHORTCUT)
  if (clipboardStore.monitoring) {
    invoke('register_clipboard_shortcut', { shortcut: DEFAULT_SHORTCUT }).catch(() => {})
  }
}

function recordKey(e: KeyboardEvent) {
  if (!recordingShortcut.value) return
  e.preventDefault()
  e.stopPropagation()
  if (['Meta', 'Control', 'Shift', 'Alt'].includes(e.key)) return

  // Esc or Backspace cancels recording
  if (e.key === 'Escape' || e.key === 'Backspace') {
    recordingShortcut.value = false
    return
  }

  const hasModifier = e.metaKey || e.ctrlKey || e.altKey
  if (!hasModifier) return

  const parts: string[] = []
  if (e.metaKey) parts.push('CmdOrCtrl')
  else if (e.ctrlKey) parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey) parts.push('Alt')

  // Use e.code to get the physical key, avoiding Alt+V → √ issue
  const code = e.code
  let key: string
  if (code.startsWith('Key')) key = code.slice(3)
  else if (code.startsWith('Digit')) key = code.slice(5)
  else if (code === 'Space') key = 'Space'
  else if (code === 'Enter') key = 'Enter'
  else if (code.startsWith('Arrow')) key = code.slice(5)
  else if (code.startsWith('F') && /^F\d+$/.test(code)) key = code
  else key = code

  parts.push(key)

  const shortcut = parts.join('+')
  settings.setClipboardShortcut(shortcut)
  recordingShortcut.value = false

  if (clipboardStore.monitoring) {
    invoke('register_clipboard_shortcut', { shortcut }).catch(() => {})
  }
}

function switchLocale(value: Locale) { settings.setLocale(value) }

function soundValue(target: SoundTarget) {
  const choice = soundSettings.value[target]
  return choice.type === 'preset' ? choice.id : choice.type
}

function customSoundName(target: SoundTarget) {
  const choice = soundSettings.value[target]
  return choice.type === 'custom' ? choice.name : ''
}

const soundTargets = computed<SoundTarget[]>(() =>
  isMobilePlatform.value ? ['pomodoroStart', 'pomodoroEnd'] : ['pomodoroStart', 'pomodoroEnd', 'alarm'],
)

const soundVolumePercent = computed(() => Math.round(soundSettings.value.volume * 100))

function setSound(target: SoundTarget, value: string) {
  soundSettings.value[target] = value === 'synth' ? { type: 'synth' } : { type: 'preset', id: value }
  settings.saveSoundSettings()
}

function onSoundSelect(target: SoundTarget, event: Event) {
  const value = event.target instanceof HTMLSelectElement ? event.target.value : 'synth'
  setSound(target, value)
}

function setSoundVolume(event: Event) {
  const value = event.target instanceof HTMLInputElement ? Number(event.target.value) : soundVolumePercent.value
  soundSettings.value.volume = Math.min(1, Math.max(0, value / 100))
  settings.saveSoundSettings()
}

async function chooseCustomSound(target: SoundTarget) {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'ogg', 'm4a', 'aac', 'flac'] }]
  })
  if (typeof selected !== 'string') return
  try {
    const copied = selected.startsWith('content://')
      ? await copySelectedSoundToAppData(selected)
      : await invoke<CopiedSound>('copy_custom_sound', { path: selected })
    soundSettings.value[target] = { type: 'custom', path: copied.path, name: copied.name }
  } catch (error) {
    console.error('Failed to copy custom sound:', error)
    const name = safeSoundFilename(selected)
    soundSettings.value[target] = { type: 'custom', path: selected, name }
  }
  settings.saveSoundSettings()
  previewSound(target)
}

function previewSound(target: SoundTarget) {
  playAlertChime(soundSettings.value[target], soundSettings.value.volume)
}

const themeOptions: Array<{ id: ThemeMode; icon: typeof Sun; label: string }> = [
  { id: 'light', icon: Sun, label: 'settings.themeLight' },
  { id: 'dark', icon: Moon, label: 'settings.themeDark' },
  { id: 'system', icon: Monitor, label: 'settings.themeSystem' }
]

function setRetentionMode(mode: 'none' | 'time' | 'count') {
  clipboardRetention.value.mode = mode
  settings.saveClipboardRetention()
  runCleanup()
}

function setRetentionDays(days: number) {
  clipboardRetention.value.days = days
  customDays.value = days
  settings.saveClipboardRetention()
  runCleanup()
}

function setRetentionCount(count: number) {
  clipboardRetention.value.count = count
  customCount.value = count
  settings.saveClipboardRetention()
  runCleanup()
}

function onCustomDays() {
  const v = Math.max(1, customDays.value)
  setRetentionDays(v)
}

function onCustomCount() {
  const v = Math.max(10, customCount.value)
  setRetentionCount(v)
}

function runCleanup() {
  const r = clipboardRetention.value
  if (r.mode === 'none') return
  invoke('cleanup_clipboard', { mode: r.mode, days: r.days, count: r.count }).catch(() => {})
}

async function toggleAutostart() {
  if (autostartEnabled.value) { await disable() } else { await enable() }
  autostartEnabled.value = !autostartEnabled.value
}

async function testAi() {
  settings.saveAiConfig()
  aiTesting.value = true
  aiTestResult.value = ''
  try {
    const response = await invoke<{ content: string }>('ai_chat', { config: aiConfig.value, messages: [{ role: 'user', content: 'Say hello in one sentence.' }] })
    aiTestResult.value = `Success: ${response.content}`
  } catch (error) {
    aiTestResult.value = `Error: ${error}`
  }
  aiTesting.value = false
}

// Sync state
const syncServerUrl = ref('')
const syncPairingCode = ref('')
const syncDeviceName = ref('')
const syncErrorText = computed(() =>
  syncStore.error === 'Error: INVALID_SERVER_URL' || syncStore.error === 'INVALID_SERVER_URL'
    ? t('settings.syncInvalidServerUrl')
    : syncStore.error
)

async function syncPair() {
  try {
    await syncStore.pair(syncServerUrl.value, syncPairingCode.value, syncDeviceName.value || (isDesktopPlatform.value ? 'Desktop' : 'Mobile'))
  } catch {
    // error handled by store
  }
}

const appVersion = ref('0.0.0')

onMounted(async () => {
  if (isDesktopPlatform.value) {
    try { autostartEnabled.value = await isEnabled() } catch {}
  }
  try { appVersion.value = await getVersion() } catch {}
  syncStore.loadConfig()
  if (isDesktopPlatform.value) runCleanup()
})
</script>

<template>
  <div class="mx-auto max-w-2xl px-4 py-6 sm:px-6 sm:py-8">
    <h1 class="text-2xl font-bold mb-6">{{ t('settings.title') }}</h1>
    <section v-if="isMobilePlatform" class="bg-card rounded-xl p-4 border mb-6">
      <h2 class="text-sm font-semibold mb-3">{{ t('settings.mobileEntries') }}</h2>
      <button class="flex w-full items-center justify-between gap-3 rounded-lg bg-secondary/70 px-3 py-3 text-left transition-colors hover:bg-secondary" @click="router.push('/ai')">
        <span class="flex min-w-0 items-center gap-2">
          <Bot class="w-4 h-4 text-violet-500" />
          <span class="text-sm font-medium">{{ t('nav.ai') }}</span>
        </span>
        <ChevronRight class="w-4 h-4 text-muted-foreground" />
      </button>
    </section>
    <section class="bg-card rounded-xl p-4 border mb-6">
      <h2 class="text-sm font-semibold mb-1 flex items-center gap-2">
        <Globe class="w-4 h-4" />
        {{ t('settings.language') }}
      </h2>
      <p class="text-xs text-muted-foreground mb-3">{{ t('settings.languageDesc') }}</p>
      <div class="flex gap-2">
        <button
          v-for="item in [{ id: 'zh', text: '中文' }, { id: 'en', text: 'English' }]"
          :key="item.id"
          class="px-4 py-2 rounded-lg text-sm font-medium transition-colors"
          :class="locale === item.id ? 'bg-primary text-primary-foreground' : 'bg-secondary hover:bg-secondary/70'"
          @click="switchLocale(item.id as Locale)"
        >{{ item.text }}
        </button>
      </div>
    </section>
    <section class="bg-card rounded-xl p-4 border mb-6">
      <h2 class="text-sm font-semibold mb-1 flex items-center gap-2">
        <Sun class="w-4 h-4" />
        {{ t('settings.theme') }}
      </h2>
      <p class="text-xs text-muted-foreground mb-3">{{ t('settings.themeDesc') }}</p>
      <div class="flex gap-2">
        <button
          v-for="opt in themeOptions"
          :key="opt.id"
          class="flex items-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium transition-colors"
          :class="theme === opt.id ? 'bg-primary text-primary-foreground' : 'bg-secondary hover:bg-secondary/70'"
          @click="settings.setThemeMode(opt.id)"
        >
          <component :is="opt.icon" class="w-3.5 h-3.5" />
          {{ t(opt.label) }}
        </button>
      </div>
    </section>
    <section class="bg-card rounded-xl p-4 border mb-6">
      <h2 class="text-sm font-semibold mb-1 flex items-center gap-2">
        <Volume2 class="w-4 h-4" />
        {{ t('sound.title') }}
      </h2>
      <p class="text-xs text-muted-foreground mb-3">{{ isMobilePlatform ? t('sound.mobileDesc') : t('sound.desc') }}</p>
      <div class="space-y-3">
        <div class="rounded-lg bg-secondary/50 p-3">
          <div class="mb-2 flex items-center justify-between gap-3">
            <span class="text-xs font-medium text-muted-foreground">{{ t('sound.volume') }}</span>
            <span class="font-mono text-sm font-semibold tabular-nums">{{ soundVolumePercent }}%</span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            step="5"
            class="h-2 w-full accent-primary"
            :value="soundVolumePercent"
            :aria-label="t('sound.volume')"
            @input="setSoundVolume"
          />
          <p class="mt-2 text-xs text-muted-foreground">{{ t('sound.volumeDesc') }}</p>
        </div>
        <div v-for="target in soundTargets" :key="target" class="grid gap-2 sm:grid-cols-[7rem_minmax(0,1fr)] sm:items-center">
          <span class="text-xs text-muted-foreground sm:w-28">{{ t(`sound.${target}`) }}</span>
          <div class="grid grid-cols-2 gap-2 sm:flex sm:flex-wrap sm:items-center">
          <select
            class="mobile-field col-span-2 min-w-0 rounded-lg border bg-background px-3 text-sm sm:col-span-1 sm:min-w-40"
            :value="soundValue(target)"
            @change="onSoundSelect(target, $event)"
          >
            <option v-if="customSoundName(target)" value="custom" disabled>{{ customSoundName(target) }}</option>
            <option v-for="sound in PRESET_ALERT_SOUNDS" :key="sound.id" :value="sound.id">{{ t(sound.labelKey) }}</option>
          </select>
          <button class="mobile-field inline-flex items-center justify-center gap-1.5 rounded-lg bg-secondary px-3 text-sm transition-colors hover:bg-secondary/70" @click="chooseCustomSound(target)">
            <FolderOpen class="w-3.5 h-3.5" />
            {{ t('sound.chooseCustom') }}
          </button>
          <button class="mobile-field inline-flex items-center justify-center gap-1.5 rounded-lg bg-secondary px-3 text-sm transition-colors hover:bg-secondary/70" @click="previewSound(target)">
            <Play class="w-3.5 h-3.5" />
            {{ t('sound.preview') }}
          </button>
          <span v-if="customSoundName(target)" class="col-span-2 truncate text-xs text-muted-foreground sm:max-w-48">{{ customSoundName(target) }}</span>
          </div>
        </div>
      </div>
    </section>
    <section v-if="isDesktopPlatform" class="bg-card rounded-xl p-4 border mb-6">
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-sm font-semibold flex items-center gap-2">
            <Power class="w-4 h-4" />
            {{ t('settings.autostart') }}
          </h2>
          <p class="text-xs text-muted-foreground mt-1">{{ t('settings.autostartDesc') }}</p></div>
        <button class="mobile-switch" :class="autostartEnabled ? 'bg-primary' : 'bg-input'" @click="toggleAutostart">
          <span class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform" :class="{ 'translate-x-5': autostartEnabled }" /></button>
      </div>
    </section>
    <section v-if="isDesktopPlatform" class="bg-card rounded-xl p-4 border mb-6">
      <div class="flex items-center justify-between mb-1">
        <h2 class="text-sm font-semibold flex items-center gap-2">
          <Scissors class="w-4 h-4" />
          {{ t('clipboardSettings.title') }}
        </h2>
        <button class="mobile-switch" :class="clipboardStore.monitoring ? 'bg-primary' : 'bg-input'" @click="clipboardStore.toggleMonitoring()">
          <span class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform" :class="{ 'translate-x-5': clipboardStore.monitoring }" /></button>
      </div>
      <p class="text-xs text-muted-foreground mb-3">{{ t('clipboardSettings.desc') }}</p>
      <div class="flex items-center gap-3 mb-3">
        <span class="text-xs text-muted-foreground">{{ t('clipboardSettings.shortcut') }}</span>
        <div
          class="px-3 py-1.5 rounded-lg text-sm border cursor-pointer min-w-[140px] text-center"
          :class="recordingShortcut ? 'border-ring bg-accent' : 'bg-secondary border-input'"
          tabindex="0"
          @click="startRecording"
          @keydown="recordKey"
        >{{ recordingShortcut ? t('clipboardSettings.pressKeys') : settings.clipboardShortcut }}</div>
        <button
          v-if="settings.clipboardShortcut !== DEFAULT_SHORTCUT"
          class="text-xs text-muted-foreground transition-colors hover:text-primary"
          @click="resetShortcut"
        >{{ t('clipboardSettings.reset') }}</button>
      </div>
      <div class="flex gap-2 mb-3">
        <button
          v-for="m in ['none', 'time', 'count'] as const" :key="m"
          class="px-3 py-1.5 rounded-lg text-sm transition-colors"
          :class="clipboardRetention.mode === m ? 'bg-primary text-primary-foreground' : 'bg-secondary hover:bg-secondary/70'"
          @click="setRetentionMode(m)"
        >{{ t(`clipboardSettings.mode${m[0].toUpperCase() + m.slice(1)}`) }}</button>
      </div>
      <div v-if="clipboardRetention.mode === 'time'" class="flex flex-wrap gap-2 items-center">
        <button
          v-for="d in [1, 3, 7, 30]" :key="d"
          class="px-3 py-1 rounded text-xs transition-colors"
          :class="clipboardRetention.days === d ? 'bg-accent text-accent-foreground' : 'bg-secondary text-muted-foreground hover:bg-secondary/70'"
          @click="setRetentionDays(d)"
        >{{ t(`clipboardSettings.day${d}`) }}</button>
        <div class="flex items-center gap-1">
          <Input v-model.number="customDays" type="number" min="1" class="w-16" @change="onCustomDays" />
          <span class="text-xs text-muted-foreground">{{ t('clipboardSettings.days') }}</span>
        </div>
      </div>
      <div v-if="clipboardRetention.mode === 'count'" class="flex flex-wrap gap-2 items-center">
        <button
          v-for="c in [100, 200, 300, 500]" :key="c"
          class="px-3 py-1 rounded text-xs transition-colors"
          :class="clipboardRetention.count === c ? 'bg-accent text-accent-foreground' : 'bg-secondary text-muted-foreground hover:bg-secondary/70'"
          @click="setRetentionCount(c)"
        >{{ t(`clipboardSettings.count${c}`) }}</button>
        <div class="flex items-center gap-1">
          <Input v-model.number="customCount" type="number" min="10" class="w-20" @change="onCustomCount" />
          <span class="text-xs text-muted-foreground">{{ t('clipboardSettings.items') }}</span>
        </div>
      </div>
    </section>
    <section class="bg-card rounded-xl p-4 border mb-6">
      <h2 class="text-sm font-semibold mb-3">{{ t('settings.aiConfig') }}</h2>
      <div class="space-y-3">
        <label class="block"><span class="block text-xs text-muted-foreground mb-1">{{ t('settings.provider') }}</span><select
          v-model="aiConfig.provider"
          class="mobile-field w-full rounded-lg border bg-background px-3 text-base"
          @change="settings.saveAiConfig"
        >
          <option value="deepseek">DeepSeek</option>
          <option value="openai">OpenAI</option>
          <option value="custom">Custom</option>
        </select></label>
        <label class="block"><span class="block text-xs text-muted-foreground mb-1">{{ t('settings.apiUrl') }}</span><Input
          v-model="aiConfig.api_url"
          class="mobile-field w-full"
          @blur="settings.saveAiConfig"
        /></label>
        <label class="block"><span class="block text-xs text-muted-foreground mb-1">{{ t('settings.apiKey') }}</span><Input
          v-model="aiConfig.api_key"
          type="password"
          class="mobile-field w-full"
          placeholder="sk-..."
          @blur="settings.saveAiConfig"
        /></label>
        <label class="block"><span class="block text-xs text-muted-foreground mb-1">{{ t('settings.model') }}</span><Input
          v-model="aiConfig.model"
          class="mobile-field w-full"
          @blur="settings.saveAiConfig"
        /></label>
        <div class="flex items-center justify-between py-2">
          <div>
            <span class="block text-xs text-muted-foreground">{{ t('settings.reasoningEnabled') }}</span>
            <span class="block text-[11px] text-muted-foreground mt-0.5">{{ t('settings.reasoningEnabledDesc') }}</span>
          </div>
          <button
            class="mobile-switch transition-colors"
            :class="aiConfig.reasoning_enabled ? 'bg-primary' : 'bg-input'"
            @click="aiConfig.reasoning_enabled = !aiConfig.reasoning_enabled; settings.saveAiConfig()"
          ><span class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform" :class="{ 'translate-x-5': aiConfig.reasoning_enabled }" /></button>
        </div>
        <label v-if="aiConfig.reasoning_enabled" class="block">
          <span class="block text-xs text-muted-foreground mb-1">{{ t('settings.reasoningEffort') }}</span>
          <div class="flex gap-2">
            <button
              v-for="level in ['low', 'medium', 'high']"
              :key="level"
            class="mobile-field px-3 rounded-lg text-sm font-medium transition-colors"
              :class="aiConfig.reasoning_effort === level ? 'bg-primary text-primary-foreground' : 'bg-secondary text-muted-foreground hover:bg-secondary/70'"
              @click="aiConfig.reasoning_effort = level; settings.saveAiConfig()"
            >{{ t('settings.effort.' + level) }}</button>
          </div>
        </label>
        <label class="block">
          <div class="flex items-center justify-between mb-1">
            <span class="block text-xs text-muted-foreground">{{ t('settings.temperature') }}</span>
            <span class="text-xs text-muted-foreground font-mono">{{ aiConfig.temperature.toFixed(1) }}</span>
          </div>
          <input
            v-model.number="aiConfig.temperature"
            type="range" min="0" max="2" step="0.1"
            class="w-full accent-primary"
            @change="settings.saveAiConfig"
          />
          <div class="flex justify-between text-[10px] text-muted-foreground mt-0.5">
            <span>{{ t('settings.temperatureLow') }}</span>
            <span>{{ t('settings.temperatureHigh') }}</span>
          </div>
        </label>
        <div class="flex items-center gap-3">
          <button class="mobile-field inline-flex items-center justify-center gap-1.5 rounded-lg bg-primary px-4 text-sm text-primary-foreground transition-colors hover:bg-primary/90" :disabled="aiTesting || !aiConfig.api_key" @click="testAi">
            <Save class="w-3.5 h-3.5" />
            {{ aiTesting ? t('settings.testing') : t('settings.testConnection') }}
          </button>
          <span v-if="aiTestResult" class="text-xs" :class="aiTestResult.startsWith('Success') ? 'text-green-500' : 'text-red-500'">{{ aiTestResult }}</span></div>
      </div>
    </section>
    <!-- Sync -->
    <section class="bg-card rounded-xl p-4 border">
      <h2 class="text-sm font-semibold mb-3 flex items-center gap-2">
        <Globe class="w-4 h-4" />
        {{ t('settings.sync') }}
      </h2>
      <div v-if="syncStore.isConfigured" class="space-y-3">
        <div class="flex items-center gap-2 text-sm">
          <span class="w-2 h-2 rounded-full" :class="syncStore.statusText === 'connected' ? 'bg-green-500' : syncStore.statusText === 'syncing' ? 'bg-yellow-500 animate-pulse' : 'bg-red-500'" />
          <span class="text-muted-foreground">{{ syncStore.config?.server_url }}</span>
          <span class="text-xs text-muted-foreground">({{ syncStore.config?.device_name }})</span>
        </div>
        <div class="flex items-center gap-2">
          <button
            class="mobile-field rounded-lg bg-primary px-4 text-sm text-primary-foreground transition-colors hover:bg-primary/90"
            :disabled="syncStore.isSyncing"
            @click="syncStore.syncNow()"
          >
            {{ syncStore.isSyncing ? t('settings.syncing') : t('settings.syncNow') }}
          </button>
          <button class="mobile-field rounded-lg bg-destructive/10 px-4 text-sm text-destructive" @click="syncStore.disconnect()">
            {{ t('settings.syncDisconnect') }}
          </button>
        </div>
        <p v-if="syncStore.lastSyncAt" class="text-xs text-muted-foreground">
          {{ t('settings.syncLastSync') }}: {{ new Date(syncStore.lastSyncAt).toLocaleString() }}
          <span v-if="syncStore.lastSyncResult">({{
            syncStore.lastSyncResult.pushed_count > 0 ? `↑${syncStore.lastSyncResult.pushed_count}` : ''
          }}{{
            syncStore.lastSyncResult.pulled_count > 0 ? ` ↓${syncStore.lastSyncResult.pulled_count}` : ''
          }}{{
            syncStore.lastSyncResult.conflict_count > 0 ? ` ⚠${syncStore.lastSyncResult.conflict_count}` : ''
          }})</span>
        </p>
      </div>
      <div v-else class="space-y-3">
        <p class="text-sm text-muted-foreground">{{ t('settings.syncDesc') }}</p>
        <label class="block">
          <span class="text-xs text-muted-foreground">{{ t('settings.syncServerUrl') }}</span>
          <input v-model="syncServerUrl" type="text" :placeholder="t('settings.syncServerUrlPlaceholder')" class="mobile-field mt-1 w-full border bg-background px-3 text-sm" />
        </label>
        <label class="block">
          <span class="text-xs text-muted-foreground">{{ t('settings.syncPairingCode') }}</span>
          <input v-model="syncPairingCode" type="text" placeholder="000000" maxlength="6" class="mobile-field mt-1 w-full border bg-background px-3 text-sm font-mono tracking-widest" />
        </label>
        <label class="block">
          <span class="text-xs text-muted-foreground">{{ t('settings.syncDeviceName') }}</span>
          <input v-model="syncDeviceName" type="text" :placeholder="t('settings.syncDeviceNamePlaceholder')" class="mobile-field mt-1 w-full border bg-background px-3 text-sm" />
        </label>
        <button
          class="mobile-field rounded-lg bg-primary px-4 text-sm text-primary-foreground transition-colors hover:bg-primary/90"
          :disabled="!syncServerUrl || !syncPairingCode"
          @click="syncPair()"
        >
          {{ t('settings.syncConnect') }}
        </button>
        <p v-if="syncErrorText" class="text-xs text-destructive">{{ syncErrorText }}</p>
      </div>
    </section>
    <section class="bg-card rounded-xl p-4 border"><h2 class="text-sm font-semibold mb-3">{{ t('settings.about') }}</h2>
      <div class="flex items-center gap-4"><img src="/nalu-logo.png" alt="Nalu" class="w-48" />
        <div class="text-sm text-muted-foreground"><p>Nalu v{{ appVersion }}</p>
          <p>{{ t('settings.aboutText1') }}</p>
          <p>{{ t('settings.aboutText2') }}</p>
          <p>{{ t('settings.aboutText3') }}</p></div>
      </div>
    </section>
  </div>
</template>
