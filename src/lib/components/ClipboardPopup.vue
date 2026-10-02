<script setup lang="ts">
import ClipboardImage from './ClipboardImage.vue'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { Copy, FileText, Type, Search, X, Scissors, TriangleAlert } from 'lucide-vue-next'
import type { ClipboardEntry } from '$lib/types'
import { useI18n } from '$lib/i18n'
import { initTheme } from '$lib/utils/theme'

const { t } = useI18n()
const entries = ref<ClipboardEntry[]>([])
const search = ref('')
const searchInput = ref<HTMLInputElement>()
const selectedIndex = ref(0)
const listContainer = ref<HTMLDivElement>()
const permissionDenied = ref(false)
let unlisten: (() => void) | undefined
let unlistenPanel: (() => void) | undefined
let cleanupTheme: (() => void) | undefined
let isClosing = false

const filteredEntries = computed(() => {
  const query = search.value.trim().toLowerCase()
  return query
    ? entries.value.filter((entry) => entry.content.toLowerCase().includes(query) || entry.content_type.includes(query))
    : entries.value
})

watch(filteredEntries, () => { selectedIndex.value = 0 })

watch(selectedIndex, async () => {
  await nextTick()
  const container = listContainer.value
  if (!container) return
  const selected = container.children[selectedIndex.value] as HTMLElement | undefined
  selected?.scrollIntoView({ block: 'nearest' })
})

async function loadEntries() {
  try {
    entries.value = await invoke('get_clipboard_history', { limit: 50 })
  } catch {
    entries.value = []
  }
}

async function closePopup() {
  isClosing = true
  try {
    await invoke('activate_previous_app')
  } finally {
    isClosing = false
  }
}

function isImage(entry: ClipboardEntry) {
  return entry.content_type === 'image' || entry.content_type.startsWith('image/') || entry.content_type === 'image_file'
}


async function copyEntry(entry: ClipboardEntry) {
  try {
    await invoke('copy_and_paste', { content: entry.content, contentType: entry.content_type })
  } catch (err) {
    if (String(err).includes('ACCESSIBILITY_PERMISSION_DENIED')) {
      permissionDenied.value = true
      return
    }
    throw err
  }
}

async function openSettings() {
  await invoke('open_accessibility_settings')
  permissionDenied.value = false
}

async function deleteEntry(id: string) {
  await invoke('delete_clipboard_entry', { id })
  await loadEntries()
}

function preview(entry: ClipboardEntry) {
  return isImage(entry)
    ? t('clipboardExt.image')
    : entry.content_type === 'file'
      ? entry.content.split('/').pop() || entry.content
      : entry.content.length > 60 ? `${entry.content.slice(0, 60)}...` : entry.content
}

function parseTimestamp(value: string) {
  const normalized = value.replace(' ', 'T')
  // If there is already a timezone designator (Z, +HH:MM, -HH:MM after the time component) use it as-is.
  // Otherwise SQLite stored a naive UTC datetime — append Z so it parses as UTC, not local.
  const hasTimezone = /(Z|[+-]\d{2}:?\d{2})$/.test(normalized)
  return new Date(hasTimezone ? normalized : `${normalized}Z`)
}

function timeAgo(value: string) {
  const parsed = parseTimestamp(value)
  const diff = Math.max(0, Math.floor((Date.now() - parsed.getTime()) / 1000))
  if (Number.isNaN(diff)) return ''
  return diff < 60 ? `${diff}s` : diff < 3600 ? `${Math.floor(diff / 60)}m` : diff < 86400 ? `${Math.floor(diff / 3600)}h` : `${Math.floor(diff / 86400)}d`
}

function keydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    void closePopup()
    return
  }

  if (event.key === 'ArrowDown') {
    event.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, filteredEntries.value.length - 1)
    return
  }

  if (event.key === 'ArrowUp') {
    event.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
    return
  }

  if (event.key === 'Enter' && !event.shiftKey) {
    const entry = filteredEntries.value[selectedIndex.value]
    if (entry) {
      event.preventDefault()
      void copyEntry(entry)
    }
    return
  }

  // For any other printable key, focus the search input so the character lands there.
  if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
    const input = searchInput.value
    if (input && document.activeElement !== input) {
      input.focus()
    }
  }
}

onMounted(async () => {
  cleanupTheme = initTheme()
  // This popup runs in a transparent Tauri window; the global body background
  // (set for the opaque main window) would paint it opaque and capture clicks
  // in the margin, breaking paste. Keep this window's body transparent.
  document.body.style.backgroundColor = 'transparent'
  window.addEventListener('keydown', keydown)
  unlistenPanel = await listen('panel-shown', () => {
    search.value = ''
    selectedIndex.value = 0
    void loadEntries()
    requestAnimationFrame(() => {
      searchInput.value?.focus({ preventScroll: true })
    })
  })
  unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (!focused && !isClosing) {
      void closePopup()
    }
  })
  await loadEntries()
})

onBeforeUnmount(() => {
  document.body.style.backgroundColor = ''
  window.removeEventListener('keydown', keydown)
  unlisten?.()
  unlistenPanel?.()
  cleanupTheme?.()
})
</script>

<template>
  <div class="h-screen p-3 bg-transparent">
    <div class="h-full flex flex-col bg-card/95 backdrop-blur-xl text-foreground rounded-2xl overflow-hidden border">
      <div data-tauri-drag-region class="flex items-center gap-2 px-4 py-3 border-b">
        <Scissors class="w-4 h-4 text-primary" />
        <span class="text-sm font-semibold flex-1">{{ t('clipboardExt.popup') }}</span>
        <button class="p-1.5 text-muted-foreground hover:text-red-500" @click="closePopup()">
          <X class="w-4 h-4" />
        </button>
      </div>
      <div v-if="permissionDenied" class="mx-3 mt-2 p-3 rounded-lg bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30">
        <div class="flex items-start gap-2">
          <TriangleAlert class="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
          <div class="flex-1 min-w-0">
            <p class="text-xs font-medium text-amber-700 dark:text-amber-400">{{ t('clipboardExt.permTitle') }}</p>
            <p class="text-[11px] text-amber-600/80 dark:text-amber-400/70 mt-0.5">{{ t('clipboardExt.permBody') }}</p>
            <button class="mt-2 px-2.5 py-1 rounded-md bg-amber-500 text-white text-[11px] font-medium transition-colors hover:bg-amber-600" @click="openSettings">
              {{ t('clipboardExt.permOpen') }}
            </button>
          </div>
        </div>
      </div>
      <div class="px-3 py-2 border-b">
        <div class="flex items-center gap-2 px-2 py-1.5 rounded-lg bg-secondary">
          <Search class="w-3.5 h-3.5 text-muted-foreground" />
          <input ref="searchInput" v-model="search" class="flex-1 bg-transparent text-sm outline-none" :placeholder="t('clipboardExt.filterPlaceholder')" />
          <button v-if="search" @click="search = ''">
            <X class="w-3 h-3" />
          </button>
        </div>
      </div>
      <div ref="listContainer" class="flex-1 overflow-y-auto p-1.5 space-y-0.5">
        <div
          v-for="(entry, index) in filteredEntries"
          :key="entry.id"
          class="group flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer transition-colors"
          :class="index === selectedIndex
            ? 'bg-accent text-accent-foreground ring-1 ring-ring'
            : 'hover:bg-accent'"
          role="button"
          tabindex="0"
          @click="copyEntry(entry)"
          @mouseenter="selectedIndex = index"
        >
          <ClipboardImage v-if="isImage(entry)" :id="entry.id" :content="entry.content" class="w-10 h-10" />
          <FileText v-else-if="entry.content_type === 'file'" class="w-4 h-4 text-amber-500" />
          <Type v-else class="w-4 h-4 text-blue-400" />
          <span class="flex-1 min-w-0 text-xs truncate">{{ preview(entry) }}</span>
          <span class="text-[10px] text-muted-foreground">{{ timeAgo(entry.created_at) }}</span>
          <div class="flex opacity-0 group-hover:opacity-100">
            <button class="p-1 text-primary" @click.stop="copyEntry(entry)">
              <Copy class="w-3 h-3" />
            </button>
            <button class="p-1 text-red-400" @click.stop="deleteEntry(entry.id)">
              <X class="w-3 h-3" />
            </button>
          </div>
        </div>
        <div v-if="filteredEntries.length === 0" class="text-center py-8 text-muted-foreground text-xs">
          {{ search ? t('clipboardExt.noMatch') : t('clipboard.noHistory') }}
        </div>
      </div>
      <div class="px-3 py-1.5 border-t flex justify-between text-[10px] text-muted-foreground">
        <span>{{ filteredEntries.length }} {{ t('clipboardExt.items') }}</span>
        <span>↑↓ {{ t('clipboardExt.navigate') }} · Enter {{ t('clipboardExt.paste') }}</span>
      </div>
    </div>
  </div>
</template>
