import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface SyncConfig {
  server_url: string;
  device_id: string;
  device_name: string;
}

export interface SyncResult {
  pushed_count: number;
  pulled_count: number;
  conflict_count: number;
}

function normalizeServerUrl(serverUrl: string) {
  const value = serverUrl.trim().replace(/\/+$/, "");
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw new Error("Unsupported protocol");
    }
    return url.toString().replace(/\/+$/, "");
  } catch {
    throw new Error("INVALID_SERVER_URL");
  }
}

export const useSyncStore = defineStore("sync", () => {
  const isConfigured = ref(false);
  const isSyncing = ref(false);
  const lastSyncResult = ref<SyncResult | null>(null);
  const lastSyncAt = ref<string | null>(null);
  const error = ref<string | null>(null);
  const config = ref<SyncConfig | null>(null);

  const statusText = computed(() => {
    if (!isConfigured.value) return "not configured";
    if (isSyncing.value) return "syncing";
    if (error.value) return "error";
    return "connected";
  });

  async function loadConfig() {
    try {
      const c = await invoke<SyncConfig | null>("sync_get_config");
      if (c) {
        config.value = c;
        isConfigured.value = true;
      }
    } catch {
      // Not configured yet
    }
  }

  async function pair(serverUrl: string, pairingCode: string, deviceName: string) {
    try {
      error.value = null;
      const normalizedServerUrl = normalizeServerUrl(serverUrl);
      const c = await invoke<SyncConfig>("sync_pair", {
        serverUrl: normalizedServerUrl,
        pairingCode: pairingCode.trim(),
        deviceName: deviceName.trim(),
      });
      config.value = c;
      isConfigured.value = true;
    } catch (e: any) {
      error.value = String(e);
      throw e;
    }
  }

  async function syncNow() {
    try {
      error.value = null;
      isSyncing.value = true;
      const result = await invoke<SyncResult>("sync_now");
      lastSyncResult.value = result;
      lastSyncAt.value = new Date().toISOString();
    } catch (e: any) {
      error.value = String(e);
    } finally {
      isSyncing.value = false;
    }
  }

  async function disconnect() {
    try {
      await invoke("sync_disconnect");
      isConfigured.value = false;
      config.value = null;
      lastSyncResult.value = null;
      error.value = null;
    } catch (e: any) {
      error.value = String(e);
    }
  }

  return {
    isConfigured,
    isSyncing,
    lastSyncResult,
    lastSyncAt,
    error,
    config,
    statusText,
    loadConfig,
    pair,
    syncNow,
    disconnect,
  };
});
