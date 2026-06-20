import { defineStore } from "pinia";
import { ref } from "vue";
import { type ThemeMode, setTheme as applyTheme } from "$lib/utils/theme";

type Locale = "zh" | "en";

export interface AiConfig {
  provider: string;
  api_key: string;
  api_url: string;
  model: string;
  reasoning_enabled: boolean;
  reasoning_effort: string;
  temperature: number;
}

export interface ClipboardRetention {
  mode: "none" | "time" | "count";
  days: number;
  count: number;
}

export type SoundChoice =
  | { type: "preset"; id: string }
  | { type: "custom"; path: string; name: string }
  | { type: "synth" };

export interface SoundSettings {
  pomodoroStart: SoundChoice;
  pomodoroEnd: SoundChoice;
  pomodoro: SoundChoice;
  alarm: SoundChoice;
  volume: number;
}

export type TaskGroupNamingStrategy = "date" | "dateWeekday" | "monthDay" | "defaultName";

export interface TaskGroupNamingSettings {
  strategy: TaskGroupNamingStrategy;
  fallbackName: string;
}

const defaultAiConfig: AiConfig = {
  provider: "deepseek",
  api_key: "",
  api_url: "https://api.deepseek.com/v1/chat/completions",
  model: "deepseek-chat",
  reasoning_enabled: false,
  reasoning_effort: "medium",
  temperature: 0.3,
};

const defaultClipboardRetention: ClipboardRetention = {
  mode: "none",
  days: 7,
  count: 200,
};

const defaultSoundSettings: SoundSettings = {
  pomodoroStart: { type: "preset", id: "gentle-bell" },
  pomodoroEnd: { type: "preset", id: "soft-rise" },
  pomodoro: { type: "preset", id: "gentle-bell" },
  alarm: { type: "preset", id: "warm-chime" },
  volume: 0.8,
};

const defaultTaskGroupNaming: TaskGroupNamingSettings = {
  strategy: "date",
  fallbackName: "新分组",
};

function readThemeMode(): ThemeMode {
  const stored = localStorage.getItem("nalu-theme");
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

export const useSettingsStore = defineStore("settings", () => {
  const locale = ref<Locale>((localStorage.getItem("nalu-locale") as Locale) || "zh");
  const theme = ref<ThemeMode>(readThemeMode());
  const aiConfig = ref<AiConfig>({ ...defaultAiConfig });
  const clipboardRetention = ref<ClipboardRetention>({ ...defaultClipboardRetention });
  const soundSettings = ref<SoundSettings>({ ...defaultSoundSettings });
  const clipboardShortcut = ref(localStorage.getItem("nalu-clipboard-shortcut") || "CmdOrCtrl+Shift+V");
  const taskGroupNaming = ref<TaskGroupNamingSettings>({ ...defaultTaskGroupNaming });

  const saved = localStorage.getItem("nalu-ai-config");
  if (saved) {
    try {
      aiConfig.value = { ...defaultAiConfig, ...JSON.parse(saved) };
    } catch {}
  }

  const savedRetention = localStorage.getItem("nalu-clipboard-retention");
  if (savedRetention) {
    try {
      clipboardRetention.value = { ...defaultClipboardRetention, ...JSON.parse(savedRetention) };
    } catch {}
  }

  const savedSoundSettings = localStorage.getItem("nalu-sound-settings");
  if (savedSoundSettings) {
    try {
      const parsed = JSON.parse(savedSoundSettings);
      soundSettings.value = {
        ...defaultSoundSettings,
        ...parsed,
        pomodoroStart: parsed.pomodoroStart ?? parsed.pomodoro ?? defaultSoundSettings.pomodoroStart,
        pomodoroEnd: parsed.pomodoroEnd ?? parsed.pomodoro ?? defaultSoundSettings.pomodoroEnd,
        volume: Math.min(1, Math.max(0, Number(parsed.volume ?? defaultSoundSettings.volume))),
      };
    } catch {}
  }

  const savedTaskGroupNaming = localStorage.getItem("nalu-task-group-naming");
  if (savedTaskGroupNaming) {
    try {
      taskGroupNaming.value = { ...defaultTaskGroupNaming, ...JSON.parse(savedTaskGroupNaming) };
    } catch {}
  }

  function setLocale(value: Locale) {
    locale.value = value;
    localStorage.setItem("nalu-locale", value);
  }

  function setThemeMode(value: ThemeMode) {
    theme.value = value;
    applyTheme(value);
  }

  function saveAiConfig() {
    localStorage.setItem("nalu-ai-config", JSON.stringify(aiConfig.value));
  }

  function saveClipboardRetention() {
    localStorage.setItem("nalu-clipboard-retention", JSON.stringify(clipboardRetention.value));
  }

  function saveSoundSettings() {
    localStorage.setItem("nalu-sound-settings", JSON.stringify(soundSettings.value));
  }

  function saveTaskGroupNaming() {
    const fallbackName = taskGroupNaming.value.fallbackName.trim() || defaultTaskGroupNaming.fallbackName;
    taskGroupNaming.value = { ...taskGroupNaming.value, fallbackName };
    localStorage.setItem("nalu-task-group-naming", JSON.stringify(taskGroupNaming.value));
  }

  function setClipboardShortcut(value: string) {
    clipboardShortcut.value = value;
    localStorage.setItem("nalu-clipboard-shortcut", value);
  }

  return { locale, theme, aiConfig, clipboardRetention, soundSettings, clipboardShortcut, taskGroupNaming, setLocale, setThemeMode, saveAiConfig, saveClipboardRetention, saveSoundSettings, saveTaskGroupNaming, setClipboardShortcut };
});
