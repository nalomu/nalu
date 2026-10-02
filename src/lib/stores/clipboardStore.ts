import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";

export const useClipboardStore = defineStore("clipboard", () => {
  const monitoring = ref(localStorage.getItem("nalu-clipboard-monitoring") === "true");
  const diskAccess = ref<"granted" | "denied" | "unknown" | "unsupported">("unsupported");
  const diskGuideDismissed = ref(localStorage.getItem("nalu-clipboard-disk-guide-dismissed") === "true");
  const inaccessibleImages = ref(new Set<string>());
  // Show a warning only after a real clipboard image read fails, not an unrelated probe.
  const showDiskAccessNotice = computed(() => monitoring.value && inaccessibleImages.value.size > 0 && !diskGuideDismissed.value);
  function reportImageAccessDenied(id: string) { inaccessibleImages.value.add(id); }
  function reportImageReadable(id: string) { inaccessibleImages.value.delete(id); }
  function dismissDiskAccessNotice() {
    diskGuideDismissed.value = true;
    localStorage.setItem("nalu-clipboard-disk-guide-dismissed", "true");
  }
  const monitorError = ref("");

  async function refreshDiskAccess() {
    diskAccess.value = await invoke("check_full_disk_access");
  }

  async function openDiskAccessSettings() {
    await invoke("open_full_disk_access_settings");
  }

  watch(monitoring, (value) => {
    localStorage.setItem("nalu-clipboard-monitoring", String(value));
  });

  function toggleMonitoring() {
    monitoring.value = !monitoring.value;
  }

  return { monitoring, diskAccess, reportImageAccessDenied, reportImageReadable, diskGuideDismissed, showDiskAccessNotice, dismissDiskAccessNotice, monitorError, toggleMonitoring, refreshDiskAccess, openDiskAccessSettings };
});
