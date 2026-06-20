/**
 * Tests for the global notifications module.
 * Mocks Tauri APIs and sound/alert modules.
 */
import { beforeEach, describe, it, expect, vi } from "vitest";

// ── Mock Tauri event listener ─────────────────────────────
const eventHandlers: Record<string, Function[]> = {};

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: Function) => {
    if (!eventHandlers[event]) eventHandlers[event] = [];
    eventHandlers[event].push(handler);
    return Promise.resolve(() => {});
  }),
  emit: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ label: "main" })),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  sendNotification: vi.fn(),
}));

vi.mock("$lib/utils/alertSound", () => ({
  ALERT_AUDIO_STOP_EVENT: "alert-audio-stop",
  playAlertChime: vi.fn(),
  startLoopingAlert: vi.fn(),
  stopAllAlertAudio: vi.fn(),
}));

vi.mock("$lib/stores/settingsStore", () => ({
  useSettingsStore: () => ({
    soundSettings: {
      pomodoroStart: { type: "preset", id: "gentle-bell" },
      pomodoroEnd: { type: "preset", id: "soft-rise" },
      pomodoro: { type: "preset", id: "gentle-bell" },
      alarm: { type: "preset", id: "warm-chime" },
      volume: 0.65,
    },
  }),
}));

vi.mock("$lib/stores/alertStore", () => ({
  showAlert: vi.fn(),
  dismissAlert: vi.fn(),
}));

// Import after mocking
const { initGlobalNotifications } = await import("$lib/utils/notifications");
const { invoke } = await import("@tauri-apps/api/core");
const { sendNotification } = await import("@tauri-apps/plugin-notification");
const { playAlertChime, startLoopingAlert, stopAllAlertAudio } = await import("$lib/utils/alertSound");
const { showAlert, dismissAlert } = await import("$lib/stores/alertStore");

describe("notifications", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("registers event listeners on first init", async () => {
    await initGlobalNotifications();
    expect(eventHandlers["pomodoro-work-end"]).toBeDefined();
    expect(eventHandlers["pomodoro-work-end"]!.length).toBeGreaterThanOrEqual(1);
    expect(eventHandlers["pomodoro-break-end"]).toBeDefined();
    expect(eventHandlers["alarm-triggered"]).toBeDefined();
  });

  it("is idempotent — calling init twice only registers once", async () => {
    const countBefore = eventHandlers["pomodoro-work-end"]?.length ?? 0;
    await initGlobalNotifications(); // second call
    const countAfter = eventHandlers["pomodoro-work-end"]?.length ?? 0;
    expect(countAfter).toBe(countBefore); // no new handlers added
  });

  it("pomodoro-work-end triggers notification + sound + alert", () => {
    vi.mocked(sendNotification).mockClear();
    vi.mocked(playAlertChime).mockClear();
    vi.mocked(showAlert).mockClear();

    eventHandlers["pomodoro-work-end"]?.forEach((h) => h({ payload: 0 }));

    expect(sendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: "番茄钟" })
    );
    expect(playAlertChime).toHaveBeenCalled();
    expect(playAlertChime).toHaveBeenCalledWith({ type: "preset", id: "soft-rise" }, 0.65);
    expect(showAlert).toHaveBeenCalledWith(
      expect.objectContaining({ body: expect.stringContaining("工作") })
    );
  });

  it("pomodoro confirmation resumes the next phase", () => {
    vi.mocked(invoke).mockClear();
    vi.mocked(showAlert).mockClear();

    eventHandlers["pomodoro-work-end"]?.forEach((h) => h({ payload: 1 }));

    const alertConfig = vi.mocked(showAlert).mock.calls.at(-1)?.[0];
    alertConfig?.onDismiss?.();

    expect(invoke).toHaveBeenCalledWith("pomodoro_start");
  });

  it("pomodoro-break-end triggers notification + sound + alert", () => {
    vi.mocked(sendNotification).mockClear();
    vi.mocked(playAlertChime).mockClear();
    vi.mocked(showAlert).mockClear();

    eventHandlers["pomodoro-break-end"]?.forEach((h) => h({ payload: undefined }));

    expect(sendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: "番茄钟" })
    );
    expect(playAlertChime).toHaveBeenCalled();
    expect(showAlert).toHaveBeenCalledWith(
      expect.objectContaining({ body: expect.stringContaining("休息") })
    );
  });

  it("alarm-triggered fires alarm with looping sound + snooze", () => {
    vi.mocked(sendNotification).mockClear();
    vi.mocked(startLoopingAlert).mockClear();
    vi.mocked(stopAllAlertAudio).mockClear();
    vi.mocked(showAlert).mockClear();

    const alarmPayload = {
      id: "test-alarm",
      time: "08:00",
      label: "起床",
      repeat: "daily",
      active: true,
      created_at: "2026-01-01",
    };

    eventHandlers["alarm-triggered"]?.forEach((h) => h({ payload: alarmPayload }));

    expect(sendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: "闹钟响了！" })
    );
    expect(startLoopingAlert).toHaveBeenCalled();
    expect(startLoopingAlert).toHaveBeenCalledWith({ type: "preset", id: "warm-chime" }, 0.65);
    expect(stopAllAlertAudio).toHaveBeenCalledWith("alarm-start");
    expect(showAlert).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "⏰ 闹钟响了！",
        body: "起床",
        buttonText: "关闭",
        snoozeText: "稍后提醒",
      })
    );
  });

  it("alarm-triggered prefers per-alarm sound over the global alarm sound", () => {
    vi.mocked(startLoopingAlert).mockClear();
    vi.mocked(showAlert).mockClear();

    eventHandlers["alarm-triggered"]?.forEach((h) =>
      h({
        payload: {
          id: "custom-sound-test",
          time: "08:00",
          label: "起床",
          repeat: "daily",
          active: true,
          sound: JSON.stringify({ type: "preset", id: "soft-rise" }),
          created_at: "2026-01-01",
        },
      })
    );

    expect(startLoopingAlert).toHaveBeenCalledWith({ type: "preset", id: "soft-rise" }, 0.65);
  });

  it("alarm dismiss stops alert audio globally", () => {
    vi.mocked(stopAllAlertAudio).mockClear();
    vi.mocked(showAlert).mockClear();

    eventHandlers["alarm-triggered"]?.forEach((h) =>
      h({ payload: { id: "dismiss-test", time: "08:00", label: "起床", repeat: "daily", active: true, created_at: "" } })
    );

    const alertConfig = vi.mocked(showAlert).mock.calls.at(-1)?.[0];
    alertConfig?.onDismiss?.();

    expect(stopAllAlertAudio).toHaveBeenCalledWith("alarm-dismiss");
  });

  it("alarm snooze stops alert audio globally before scheduling the retry", () => {
    vi.useFakeTimers();
    vi.mocked(stopAllAlertAudio).mockClear();
    vi.mocked(showAlert).mockClear();

    eventHandlers["alarm-triggered"]?.forEach((h) =>
      h({ payload: { id: "snooze-test", time: "08:00", label: "起床", repeat: "daily", active: true, created_at: "" } })
    );

    const alertConfig = vi.mocked(showAlert).mock.calls.at(-1)?.[0];
    alertConfig?.onSnooze?.();

    expect(stopAllAlertAudio).toHaveBeenCalledWith("alarm-snooze");
    expect(dismissAlert).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("global audio stop event from alarm start does not dismiss the new alert", () => {
    vi.mocked(dismissAlert).mockClear();

    eventHandlers["alert-audio-stop"]?.forEach((h) => h({ payload: { reason: "alarm-start" } }));

    expect(dismissAlert).not.toHaveBeenCalled();
  });

  it("global audio stop event from pomodoro cleanup does not dismiss the pomodoro alert", () => {
    vi.mocked(dismissAlert).mockClear();

    eventHandlers["alert-audio-stop"]?.forEach((h) => h({ payload: { reason: "pomodoro-end" } }));

    expect(dismissAlert).not.toHaveBeenCalled();
  });

  it("global audio stop event clears ringing alarm UI state on dismiss", () => {
    vi.mocked(dismissAlert).mockClear();

    eventHandlers["alert-audio-stop"]?.forEach((h) => h({ payload: { reason: "alarm-dismiss" } }));

    expect(dismissAlert).toHaveBeenCalled();
  });

  it("alarm without label uses default body text", () => {
    vi.mocked(showAlert).mockClear();

    eventHandlers["alarm-triggered"]?.forEach((h) =>
      h({ payload: { id: "x", time: "09:00", label: "", repeat: "none", active: true, created_at: "" } })
    );

    expect(showAlert).toHaveBeenCalledWith(
      expect.objectContaining({ body: "闹钟响了" })
    );
  });
});
