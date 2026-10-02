<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Trash2, Copy, Trash, Radio, FileText, Type, Bug } from 'lucide-vue-next'
import type { ClipboardEntry } from '$lib/types'
import { useClipboardStore } from '$lib/stores/clipboardStore'
import { useI18n } from '$lib/i18n'
import { useAiRefresh } from '$lib/composables/useAiRefresh'

const { t } = useI18n()
const store = useClipboardStore()
const { monitoring } = storeToRefs(store)
const entries = ref<ClipboardEntry[]>([])
let interval: ReturnType<typeof setInterval>
let unlistenHistory: UnlistenFn | undefined
let disposed = false

async function loadHistory() {
  try {
    entries.value = await invoke('get_clipboard_history', { limit: 200 })
  } catch (error) {
    console.error(error)
  }
}

async function copyEntry(entry: ClipboardEntry) { await invoke('write_clipboard_entry_to_system', { content: entry.content, contentType: entry.content_type }) }

async function deleteEntry(id: string) {
  await invoke('delete_clipboard_entry', { id })
  await loadHistory()
}

async function clearAll() {
  await invoke('clear_clipboard_history')
  await loadHistory()
}

async function openPopup() { await invoke('toggle_clipboard_popup') }

function imageSrc(content: string) {
  if (/^(data:|https?:|asset:)/.test(content)) return content
  return content.startsWith('file://') ? convertFileSrc(content.slice(7)) : content.startsWith('/') ? convertFileSrc(content) : content
}

onMounted(async () => {
  const unlisten = await listen('clipboard-history-changed', loadHistory)
  if (disposed) { unlisten(); return }
  unlistenHistory = unlisten
  await loadHistory()
  if (!disposed) interval = setInterval(loadHistory, 5000)
})
onBeforeUnmount(() => { disposed = true; clearInterval(interval); unlistenHistory?.() })
useAiRefresh(loadHistory)
</script>

<template>
  <div class="max-w-3xl mx-auto px-6 py-8">
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold">{{ t('clipboard.title') }}</h1>
      <div class="flex items-center gap-3">
        <button
          class="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
          :class="monitoring ? 'bg-green-50 text-green-600 hover:bg-green-100' : 'bg-secondary text-muted-foreground hover:bg-secondary/70'"
          @click="store.toggleMonitoring"
        ><span class="w-2 h-2 rounded-full" :class="monitoring ? 'bg-green-500' : 'bg-muted-foreground'" />
          <Radio class="w-3.5 h-3.5" />
          {{ monitoring ? t('clipboardExt.monitoring') : t('clipboardExt.monitorOff') }}
        </button>
        <button v-if="entries.length" class="flex items-center gap-1.5 text-xs text-red-400 transition-colors hover:text-red-500" @click="clearAll">
          <Trash class="w-3.5 h-3.5" />
          {{ t('clipboard.clearAll') }}
        </button>
        <button class="flex items-center gap-1.5 text-xs text-amber-500 transition-colors hover:text-amber-600" @click="openPopup">
          <Bug class="w-3.5 h-3.5" />
          {{ t('dashboardExt.debugPopup') }}
        </button>
      </div>
    </div>
    <div class="space-y-1.5">
      <div v-for="entry in entries" :key="entry.id" class="group flex items-center gap-3 px-4 py-3 rounded-lg bg-card border">
        <img v-if="entry.content_type === 'image' || entry.content_type === 'image_file'" :src="imageSrc(entry.content)" class="w-16 h-16 rounded object-cover" alt="clipboard image" />
        <FileText v-else-if="entry.content_type === 'file'" class="w-5 h-5 text-amber-500" />
        <Type v-else class="w-4 h-4 text-blue-500" />
        <div class="flex-1 text-sm truncate">{{
            entry.content_type.startsWith('image') ? t('clipboardExt.image') : entry.content_type === 'file' ? entry.content.split('/').pop() : entry.content
          }}
        </div>
        <span class="text-[10px] text-muted-foreground">{{ entry.created_at.split('T')[0] }}</span>
        <button class="opacity-0 group-hover:opacity-100 text-primary transition-colors hover:text-primary/80" @click="copyEntry(entry)">
          <Copy class="w-4 h-4" />
        </button>
        <button class="opacity-0 group-hover:opacity-100 text-red-400 transition-colors hover:text-red-500" @click="deleteEntry(entry.id)">
          <Trash2 class="w-4 h-4" />
        </button>
      </div>
      <div v-if="entries.length === 0" class="text-center py-12 text-muted-foreground text-sm">{{ t('clipboard.noHistory') }}</div>
    </div>
  </div>
</template>
