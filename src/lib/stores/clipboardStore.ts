import { defineStore } from "pinia";
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";

export const useClipboardStore = defineStore("clipboard", () => {
  const monitoring = ref(localStorage.getItem("nalu-clipboard-monitoring") === "true");
  const diskAccess = ref<"granted" | "denied" | "unknown" | "unsupported">("unsupported");
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

  return { monitoring, diskAccess, monitorError, toggleMonitoring, refreshDiskAccess, openDiskAccessSettings };
});
