import { watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useClipboardStore } from "$lib/stores/clipboardStore";
import { useMobile } from "$lib/composables/useMobile";

const permissionGuideKey = "nalu-clipboard-full-disk-guide-v1";

export function useClipboardMonitor() {
  const store = useClipboardStore();
  const { isDesktopPlatform } = useMobile();
  // Queue transitions so rapid toggles cannot leave the native monitor in an older state.
  let pending = Promise.resolve();
  let previouslyEnabled = false;
  let initialized = false;
  watch([() => store.monitoring, isDesktopPlatform], ([enabled, desktop]) => {
    const requestGuide = enabled && desktop && (
      localStorage.getItem(permissionGuideKey) !== "true" || (initialized && !previouslyEnabled)
    );
    initialized = true;
    previouslyEnabled = enabled && desktop;
    pending = pending.then(async () => {
      await invoke("set_clipboard_monitoring", { enabled: enabled && desktop });
      store.monitorError = "";
      if (!enabled || !desktop) return;
      try {
        await store.refreshDiskAccess();
        // Existing users get one migration guide. Later launches do not reopen Settings.
        if (store.monitoring && !store.diskGuideDismissed && requestGuide && store.diskAccess !== "granted" && store.diskAccess !== "unsupported") {
          await store.openDiskAccessSettings();
          localStorage.setItem(permissionGuideKey, "true");
        } else if (store.diskAccess === "granted") {
          localStorage.setItem(permissionGuideKey, "true");
        }
      } catch (error) {
        store.diskAccess = "unknown";
        console.error("[Clipboard] disk access guide failed", error);
      }
    }).catch((error) => {
      store.monitorError = String(error);
      console.error("[Clipboard] native monitor configuration failed", error);
    });
  }, { immediate: true });
}
