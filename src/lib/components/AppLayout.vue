<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { RouterView, useRoute, useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import Sidebar from './Sidebar.vue'
import MobileTabBar from './MobileTabBar.vue'
import CommandPalette from './CommandPalette.vue'
import { useMobile } from '$lib/composables/useMobile'
import type { CommandItem } from '$lib/types'
import { useI18n } from '$lib/i18n'
import { initGlobalNotifications } from '$lib/utils/notifications'
import { unlockAlertAudio } from '$lib/utils/alertSound'
import { useClipboardMonitor } from '$lib/composables/useClipboardMonitor'
import { useClipboardStore } from '$lib/stores/clipboardStore'
import { useSettingsStore } from '$lib/stores/settingsStore'

const router = useRouter()
const route = useRoute()
const { t, locale } = useI18n()
const { isMobile, isMobilePlatform, isDesktopPlatform } = useMobile()
const commandOpen = ref(false)
const clipboardStore = useClipboardStore()
const settingsStore = useSettingsStore()
useClipboardMonitor()

const destinations = ['tasks', 'notes', 'clipboard', 'pomodoro', 'schedule', 'mysql', 'alarm', 'ai', 'settings']
const commands: CommandItem[] = destinations.map((id) => ({
  id: `go-${id}`,
  name: `Go to ${id[0].toUpperCase()}${id.slice(1)}`,
  description: t('commandPalette.navigate'),
  action: () => router.push(`/${id}`)
}))

function handleKeydown(event: KeyboardEvent) {
  unlockAlertAudio()
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    commandOpen.value = !commandOpen.value
  }
}

function handlePointerdown() {
  unlockAlertAudio()
}

function syncTrayMenu() {
  if (!isDesktopPlatform.value) return
  const labels: Record<string, string> = {
    open: t('tray.open'),
    dashboard: t('nav.dashboard'),
    tasks: t('nav.tasks'),
    notes: t('nav.notes'),
    clipboard: t('nav.clipboard'),
    pomodoro: t('nav.pomodoro'),
    schedule: t('nav.schedule'),
    alarm: t('nav.alarm'),
    ai: t('nav.ai'),
    mysql: t('nav.mysql'),
    settings: t('nav.settings'),
    clipboardPopup: t('tray.clipboardPopup'),
    quit: t('tray.quit'),
  }
  invoke('update_tray_menu', { labels }).catch(() => {})
}

let unlistenTrayNav: UnlistenFn | null = null
let unlistenMobileBack: UnlistenFn | null = null
const mobileRouteStack: string[] = []
let restoringMobileRoute = false

function pushMobileRoute(path: string) {
  if (!isMobilePlatform.value) return
  if (restoringMobileRoute) {
    restoringMobileRoute = false
    return
  }
  if (mobileRouteStack[mobileRouteStack.length - 1] === path) return
  mobileRouteStack.push(path)
  if (mobileRouteStack.length > 30) mobileRouteStack.shift()
}

function handleMobileBack() {
  if (commandOpen.value) {
    commandOpen.value = false
    return
  }
  if (mobileRouteStack.length > 1) {
    mobileRouteStack.pop()
    const previous = mobileRouteStack[mobileRouteStack.length - 1] ?? '/'
    restoringMobileRoute = true
    void router.replace(previous)
    return
  }
  if (route.path !== '/') {
    restoringMobileRoute = true
    void router.replace('/')
  }
}

async function ensureMobileBackListener() {
  if (!isMobilePlatform.value || unlistenMobileBack) return
  pushMobileRoute(route.fullPath)
  unlistenMobileBack = await listen('nalu-back-requested', handleMobileBack)
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown)
  window.addEventListener('pointerdown', handlePointerdown)
  void initGlobalNotifications()
  syncTrayMenu()
  if (isDesktopPlatform.value) {
    unlistenTrayNav = await listen<string>('tray-navigate', ({ payload }) => {
      if (typeof payload === 'string' && payload.startsWith('/')) {
        void router.push(payload)
      }
    })
  }
  await ensureMobileBackListener()
  // Register clipboard shortcut if monitoring is enabled
  if (isDesktopPlatform.value && clipboardStore.monitoring) {
    invoke('register_clipboard_shortcut', { shortcut: settingsStore.clipboardShortcut }).catch(() => {})
  }
})

watch(locale, syncTrayMenu)

watch(() => route.fullPath, pushMobileRoute, { immediate: true })

watch(isMobilePlatform, () => {
  void ensureMobileBackListener()
}, { immediate: true })

watch(() => clipboardStore.monitoring, (enabled) => {
  if (!isDesktopPlatform.value) return
  if (enabled) {
    invoke('register_clipboard_shortcut', { shortcut: settingsStore.clipboardShortcut }).catch(() => {})
  } else {
    invoke('unregister_clipboard_shortcut').catch(() => {})
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeydown)
  window.removeEventListener('pointerdown', handlePointerdown)
  unlistenTrayNav?.()
  unlistenMobileBack?.()
})
</script>

<template>
  <!-- Desktop layout -->
  <div v-if="!isMobile" class="h-screen flex bg-background text-foreground overflow-hidden">
    <div data-tauri-drag-region class="fixed top-0 left-0 right-0 h-9 z-40 bg-background" />
    <Sidebar class="pt-9" @command="commandOpen = true" />
    <main class="flex-1 overflow-y-auto pt-9">
      <RouterView v-slot="{ Component }">
        <Transition name="page" mode="out-in">
          <component :is="Component" />
        </Transition>
      </RouterView>
    </main>
  </div>

  <!-- Mobile layout -->
  <div v-if="isMobile" class="mobile-shell h-screen flex flex-col bg-background text-foreground overflow-hidden">
    <main class="mobile-main flex-1 overflow-y-auto">
      <RouterView v-slot="{ Component }">
        <component :is="Component" />
      </RouterView>
    </main>
    <MobileTabBar />
  </div>

  <CommandPalette :open="commandOpen" :commands="commands" @close="commandOpen = false" @execute="command => command.action()" />
</template>
