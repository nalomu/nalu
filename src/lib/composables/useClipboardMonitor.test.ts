import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { effectScope, nextTick, type EffectScope } from "vue";
import { useClipboardMonitor } from "./useClipboardMonitor";
import { useClipboardStore } from "$lib/stores/clipboardStore";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("$lib/composables/useMobile", async () => {
  const { ref } = await import("vue");
  return { useMobile: () => ({ isDesktopPlatform: ref(true) }) };
});
let scope: EffectScope;
let access = "denied";
async function flush() {
  await nextTick();
  for (let i = 0; i < 12; i++) await Promise.resolve();
}
function start() {
  scope = effectScope();
  scope.run(useClipboardMonitor);
  return useClipboardStore();
}

describe("native clipboard monitoring and permission migration", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    access = "denied";
    mocks.invoke.mockReset().mockImplementation(async (command: string) => {
      if (command === "check_full_disk_access") return access;
    });
  });
  afterEach(() => scope?.stop());

  it("migrates already enabled users once, while keeping native monitoring enabled", async () => {
    localStorage.setItem("nalu-clipboard-monitoring", "true");
    start();
    await flush();
    expect(mocks.invoke).toHaveBeenCalledWith("set_clipboard_monitoring", { enabled: true });
    expect(mocks.invoke).toHaveBeenCalledWith("open_full_disk_access_settings");
    expect(localStorage.getItem("nalu-clipboard-full-disk-guide-v1")).toBe("true");
    scope.stop();
    mocks.invoke.mockClear();
    start();
    await flush();
    expect(mocks.invoke).not.toHaveBeenCalledWith("open_full_disk_access_settings");
  });

  it("requests access when switched on, and requests again on explicit re-enable", async () => {
    const store = start();
    await flush();
    expect(mocks.invoke).not.toHaveBeenCalledWith("open_full_disk_access_settings");
    store.toggleMonitoring();
    await flush();
    expect(mocks.invoke).toHaveBeenCalledWith("open_full_disk_access_settings");
    store.toggleMonitoring();
    await flush();
    expect(mocks.invoke).toHaveBeenLastCalledWith("set_clipboard_monitoring", { enabled: false });
    store.toggleMonitoring();
    await flush();
    expect(mocks.invoke.mock.calls.filter(([cmd]) => cmd === "open_full_disk_access_settings")).toHaveLength(2);
  });

  it("skips Settings when already authorized or on other operating systems", async () => {
    for (const status of ["granted", "unsupported"]) {
      access = status;
      localStorage.setItem("nalu-clipboard-monitoring", "true");
      setActivePinia(createPinia());
      mocks.invoke.mockClear();
      start();
      await flush();
      expect(mocks.invoke).not.toHaveBeenCalledWith("open_full_disk_access_settings");
      scope.stop();
    }
  });

  it("serializes a disable after a pending native start", async () => {
    let finishStart!: () => void;
    mocks.invoke.mockImplementation((command: string, args?: { enabled: boolean }) => {
      if (command === "set_clipboard_monitoring" && args?.enabled) return new Promise<void>((resolve) => { finishStart = resolve; });
      return Promise.resolve(command === "check_full_disk_access" ? "denied" : undefined);
    });
    localStorage.setItem("nalu-clipboard-monitoring", "true");
    const store = start();
    await flush();
    store.toggleMonitoring();
    await flush();
    finishStart();
    await flush();
    expect(mocks.invoke).toHaveBeenLastCalledWith("set_clipboard_monitoring", { enabled: false });
    expect(mocks.invoke).not.toHaveBeenCalledWith("open_full_disk_access_settings");
  });
});
