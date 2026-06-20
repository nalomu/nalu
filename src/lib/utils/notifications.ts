/**
 * Global notification listeners for Pomodoro and Alarm.
 * Initialized once in AppLayout so notifications work regardless of the current page.
 *
 * Pomodoro timer-end events are emitted by the Rust backend (pomodoro.rs).
 * Alarm trigger events are emitted by the Rust backend (alarm.rs alarm checker).
 * All timing logic runs in Rust — immune to WebView JS throttling when the window is hidden.
 */
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { sendNotification } from "@tauri-apps/plugin-notification";
import {
  ALERT_AUDIO_STOP_EVENT,
  playAlertChime,
  startLoopingAlert,
  stopAllAlertAudio,
} from "$lib/utils/alertSound";
import { showAlert, dismissAlert } from "$lib/stores/alertStore";
import { type SoundChoice, useSettingsStore } from "$lib/stores/settingsStore";
import type { Task } from "$lib/types";

let initialized = false;
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

function playPomodoroStartSound() {
  const settings = useSettingsStore();
  playAlertChime(settings.soundSettings.pomodoroStart, settings.soundSettings.volume);
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

function resumePomodoro() {
  invoke("pomodoro_start")
    .then(() => {
      playPomodoroStartSound();
    })
    .catch((error) => {
      console.error("Failed to resume pomodoro", error);
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

  // ── Pomodoro: listen for timer-end events ──
  await listen<number>("pomodoro-work-end", () => {
    const settings = useSettingsStore();
    sendNotification({ title: "番茄钟", body: "工作时段结束！该休息了。" });
    playAlertChime(settings.soundSettings.pomodoroEnd, settings.soundSettings.volume);
    showAlert({
      title: "🍅 工作结束",
      body: "工作时段结束！该休息了。",
      buttonText: "开始休息",
      onDismiss: resumePomodoro,
    });
  });

  await listen("pomodoro-break-end", () => {
    const settings = useSettingsStore();
    sendNotification({ title: "番茄钟", body: "休息结束！继续工作吧。" });
    playAlertChime(settings.soundSettings.pomodoroEnd, settings.soundSettings.volume);
    showAlert({
      title: "🍅 休息结束",
      body: "休息结束！继续工作吧。",
      buttonText: "开始专注",
      onDismiss: resumePomodoro,
    });
  });

  // ── Alarm: listen for alarm-triggered events from Rust backend ──
  await listen<AlarmPayload>("alarm-triggered", (event) => {
    fireAlarm(event.payload);
  });

  await listen<{ reason?: string }>(ALERT_AUDIO_STOP_EVENT, (event) => {
    if (event.payload?.reason === "alarm-start") return;
    activeAlarmId = null;
    dismissAlert();
  });

  // ── Schedule: poll calendar tasks globally so reminders work on every page ──
  void checkScheduleReminders();
  setInterval(() => {
    void checkScheduleReminders();
  }, 30_000);
}
