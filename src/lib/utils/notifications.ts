/**
 * Global notification listeners for Pomodoro and Alarm.
 * Initialized once in AppLayout so notifications work regardless of the current page.
 *
 * Pomodoro timer-end events are emitted by the Rust backend (pomodoro.rs).
 * Alarm trigger events are emitted by the Rust backend (alarm.rs alarm checker).
 * All timing logic runs in Rust — immune to WebView JS throttling when the window is hidden.
 */
import { emit, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { sendNotification } from "@tauri-apps/plugin-notification";
import {
  ALERT_AUDIO_STOP_EVENT,
  playAlertChime,
  startLoopingAlert,
  stopAllAlertAudio,
} from "$lib/utils/alertSound";
import { showAlert, dismissAlert } from "$lib/stores/alertStore";
import { type SoundChoice, useSettingsStore } from "$lib/stores/settingsStore";
import { POMODORO_STATE_CHANGED_EVENT } from "$lib/utils/pomodoroEvents";
import type { Task } from "$lib/types";

let initialized = false;
const notificationWindowId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
const POMODORO_EVENT_CLAIM_PREFIX = "nalu-pomodoro-event-claim:";
const POMODORO_EVENT_CLAIM_TTL_MS = 8_000;
const notifiedScheduleTaskIds = new Set<string>();

// Guard against duplicate alarm fires (e.g. queued events from hidden webview)
let activeAlarmId: string | null = null;

interface AlarmPayload {
  id: string;
  time: string;
  label: string;
  repeat: string;
  active: boolean;
  sound?: string | null;
  created_at: string;
}

function parseAlarmSound(sound?: string | null): SoundChoice | null {
  if (!sound) return null;
  try {
    const parsed = JSON.parse(sound) as SoundChoice;
    if (parsed.type === "synth") return parsed;
    if (parsed.type === "preset" && parsed.id) return parsed;
    if (parsed.type === "custom" && parsed.path) return parsed;
  } catch (error) {
    console.warn("[notifications] invalid alarm sound payload:", error);
  }
  return null;
}

function fireAlarm(alarm: AlarmPayload) {
  const settings = useSettingsStore();
  // If the same alarm is already ringing, skip — prevents orphan loops
  // from queued events when the webview was hidden
  if (activeAlarmId === alarm.id) return;
  activeAlarmId = alarm.id;

  // CRITICAL: clean up any previous sound BEFORE starting new ones.
  // Without this, rapid-fire events overwrite loopTimer but leave the
  // old timer running → orphan loop that can never be stopped.
  stopAllAlertAudio("alarm-start");

  const body = alarm.label || "闹钟响了";
  sendNotification({ title: "闹钟响了！", body });
  startLoopingAlert(parseAlarmSound(alarm.sound) ?? settings.soundSettings.alarm, settings.soundSettings.volume);
  showAlert({
    title: "⏰ 闹钟响了！",
    body,
    buttonText: "关闭",
    snoozeText: "稍后提醒",
    onDismiss: () => {
      stopAllAlertAudio("alarm-dismiss");
      activeAlarmId = null;
    },
    onSnooze: () => {
      stopAllAlertAudio("alarm-snooze");
      activeAlarmId = null;
      dismissAlert();
      // Re-fire after 5 minutes
      setTimeout(() => fireAlarm(alarm), 5 * 60 * 1000);
    },
  });
}

function claimPomodoroEvent(key: string) {
  if (typeof window === "undefined") return true;
  try {
    const storageKey = `${POMODORO_EVENT_CLAIM_PREFIX}${key}`;
    const now = Date.now();
    const current = window.localStorage.getItem(storageKey);
    if (current) {
      const parsed = JSON.parse(current) as { ts?: number; owner?: string };
      if (parsed.ts && now - parsed.ts < POMODORO_EVENT_CLAIM_TTL_MS) return false;
    }
    const claim = JSON.stringify({ ts: now, owner: notificationWindowId });
    window.localStorage.setItem(storageKey, claim);
    return window.localStorage.getItem(storageKey) === claim;
  } catch {
    return true;
  }
}

async function resumePomodoro() {
  stopAllAlertAudio("pomodoro-resume");
  try {
    await invoke("pomodoro_start");
    await emit(POMODORO_STATE_CHANGED_EVENT);
  } catch (error) {
    console.error("Failed to resume pomodoro", error);
  }
}

async function canHandleGlobalNotifications() {
  try {
    return getCurrentWindow().label === "main";
  } catch {
    return true;
  }
}

function showPomodoroEndAlert(options: {
  key: string;
  notificationBody: string;
  title: string;
  body: string;
  buttonText: string;
}) {
  if (!claimPomodoroEvent(options.key)) return;
  const settings = useSettingsStore();
  stopAllAlertAudio("pomodoro-end");
  sendNotification({ title: "番茄钟", body: options.notificationBody, silent: true });
  playAlertChime(settings.soundSettings.pomodoroEnd, settings.soundSettings.volume);
  showAlert({
    title: options.title,
    body: options.body,
    buttonText: options.buttonText,
    onDismiss: () => {
      void resumePomodoro();
    },
  });
}

function formatLocalDateTime(date: Date) {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

async function checkScheduleReminders() {
  const settings = useSettingsStore();
  const now = Date.now();
  const end = new Date(now + 60 * 60 * 1000);
  let tasks: Task[];

  try {
    const result = await invoke<Task[]>("get_calendar_tasks", {
      startAt: formatLocalDateTime(new Date(now - 60 * 60 * 1000)),
      endAt: formatLocalDateTime(end),
    });
    tasks = Array.isArray(result) ? result : [];
  } catch (error) {
    console.error("Failed to check schedule reminders", error);
    return;
  }

  for (const task of tasks) {
    if (task.done || !task.scheduled_start_at || notifiedScheduleTaskIds.has(task.id)) continue;
    if ((task.reminder_minutes ?? 0) <= 0) continue;

    const startAt = new Date(task.scheduled_start_at).getTime();
    const reminderAt = startAt - task.reminder_minutes * 60_000;
    if (now >= reminderAt && now <= startAt + 30_000) {
      sendNotification({ title: "日程提醒", body: `${task.title} - 即将开始` });
      playAlertChime(settings.soundSettings.alarm, settings.soundSettings.volume);
      notifiedScheduleTaskIds.add(task.id);
    }
  }
}

/**
 * Initialize global event listeners for pomodoro and alarm.
 * Safe to call multiple times — only initializes once.
 */
export async function initGlobalNotifications() {
  if (initialized) return;
  initialized = true;
  if (!(await canHandleGlobalNotifications())) return;

  // ── Pomodoro: listen for timer-end events ──
  await listen<number>("pomodoro-work-end", ({ payload }) => {
    showPomodoroEndAlert({
      key: `work:${payload}`,
      notificationBody: "工作时段结束！该休息了。",
      title: "🍅 工作结束",
      body: "工作时段结束！该休息了。",
      buttonText: "开始休息",
    });
  });

  await listen<number>("pomodoro-break-end", ({ payload }) => {
    showPomodoroEndAlert({
      key: `break:${payload}`,
      notificationBody: "休息结束！继续工作吧。",
      title: "🍅 休息结束",
      body: "休息结束！继续工作吧。",
      buttonText: "开始专注",
    });
  });

  // ── Alarm: listen for alarm-triggered events from Rust backend ──
  await listen<AlarmPayload>("alarm-triggered", (event) => {
    fireAlarm(event.payload);
  });

  await listen<{ reason?: string; source?: string }>(ALERT_AUDIO_STOP_EVENT, (event) => {
    const reason = event.payload?.reason;
    if (reason === "alarm-dismiss" || reason === "alarm-snooze") {
      activeAlarmId = null;
      dismissAlert();
    }
  });

  // ── Schedule: poll calendar tasks globally so reminders work on every page ──
  void checkScheduleReminders();
  setInterval(() => {
    void checkScheduleReminders();
  }, 30_000);
}
