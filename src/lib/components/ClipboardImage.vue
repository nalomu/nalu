<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ImageOff } from 'lucide-vue-next'
import { useI18n } from '$lib/i18n'
import { useClipboardStore } from '$lib/stores/clipboardStore'

const props = defineProps<{ id: string; content: string }>()
const { t } = useI18n()
const store = useClipboardStore()
const src = ref('')
const failed = ref(false)
let revision = 0
async function load() {
  const current = ++revision
  src.value = ''
  failed.value = false
  try {
    const thumbnail = await invoke<string>('get_clipboard_image_thumbnail', { id: props.id })
    if (current === revision) {
      src.value = thumbnail
      store.reportImageReadable(props.id)
    }
  } catch (error) {
    if (current === revision) {
      failed.value = true
      if (String(error).includes('CLIPBOARD_IMAGE_ACCESS_DENIED')) store.reportImageAccessDenied(props.id)
    }
  }
}
watch(() => [props.id, props.content], load, { immediate: true })
</script>

<template>
  <img v-if="src && !failed" :src="src" :alt="t('clipboardExt.image')" class="rounded object-cover" @error="failed = true" />
  <button v-else-if="failed" type="button" class="flex shrink-0 items-center justify-center rounded bg-secondary text-muted-foreground" :title="t('clipboard.imageUnavailable')" :aria-label="t('clipboard.imageUnavailable')" @click.stop="load">
    <ImageOff class="h-4 w-4" />
  </button>
  <span v-else class="shrink-0 animate-pulse rounded bg-secondary" />
</template>
