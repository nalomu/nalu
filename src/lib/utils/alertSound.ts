/**
 * Alert sound module for Nalu.
 *
 * Uses a generation counter to guarantee that stopLoopingAlert() kills
 * ALL running loops, even orphan ones from rapid-fire event queues
 * (e.g. when the webview was hidden and events pile up).
 */

import { convertFileSrc } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { SoundChoice } from "$lib/stores/settingsStore";

export const ALERT_AUDIO_STOP_EVENT = "alert-audio-stop";

const audioWindowId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
let audioCtx: AudioContext | null = null;
let loopTimer: ReturnType<typeof setTimeout> | null = null;
let unlocked = false;
const activeAudio = new Set<HTMLAudioElement>();
let oneShotAudio: HTMLAudioElement | null = null;
let loopAudio: HTMLAudioElement | null = null;
let stopListenerInitialized = false;
const oneShotOscillators = new Set<OscillatorNode>();
let oneShotGeneration = 0;

// Generation counter — incremented on every stop().
// Any loop whose generation doesn't match is an orphan and must self-terminate.
let generation = 0;

function clampVolume(volume = 1) {
  return Math.min(1, Math.max(0, volume));
}

function getCtx(): AudioContext {
  if (!audioCtx || audioCtx.state === "closed") {
    audioCtx = new AudioContext();
  }
  if (audioCtx.state === "suspended") {
    audioCtx.resume();
  }
  return audioCtx;
}

interface Note {
  freq: number;
  time: number;
  duration: number;
  volume: number;
}

const CHIME_NOTES: Note[] = [
  { freq: 523,  time: 0.0,  duration: 0.55, volume: 0.35 },
  { freq: 659,  time: 0.2,  duration: 0.55, volume: 0.35 },
  { freq: 784,  time: 0.45, duration: 0.55, volume: 0.40 },
  { freq: 1047, time: 0.75, duration: 0.65, volume: 0.45 },
  { freq: 880,  time: 1.1,  duration: 0.9,  volume: 0.30 },
];

const MELODY_LENGTH = 2.2;
const LOOP_INTERVAL_SECONDS = 4.0;

export const PRESET_ALERT_SOUNDS = [
  { id: "gentle-bell", labelKey: "sound.gentleBell", url: "/sounds/nalu-gentle-bell.wav" },
  { id: "warm-chime", labelKey: "sound.warmChime", url: "/sounds/nalu-warm-chime.wav" },
  { id: "soft-rise", labelKey: "sound.softRise", url: "/sounds/nalu-soft-rise.wav" },
  { id: "synth", labelKey: "sound.synth", url: "" },
] as const;

function resolveSoundUrl(choice?: SoundChoice): string | null {
  if (!choice || choice.type === "synth") return null;
  if (choice.type === "custom") {
    if (choice.path.startsWith("content://") || choice.path.startsWith("file://")) return choice.path;
    return convertFileSrc(choice.path);
  }
  const preset = PRESET_ALERT_SOUNDS.find((item) => item.id === choice.id);
  return preset?.url || null;
}

function playAudioUrl(url: string, volume = 1): Promise<void> {
  const audio = new Audio(url);
  audio.preload = "auto";
  audio.volume = clampVolume(volume);
  activeAudio.add(audio);
  audio.addEventListener("ended", () => activeAudio.delete(audio), { once: true });
  audio.addEventListener("error", () => activeAudio.delete(audio), { once: true });
  return audio.play().catch((error) => {
    activeAudio.delete(audio);
    throw error;
  });
}

function stopOneShotAlert() {
  oneShotGeneration++;
  if (oneShotAudio) {
    try { oneShotAudio.pause(); oneShotAudio.currentTime = 0; } catch { /* ignore */ }
    activeAudio.delete(oneShotAudio);
    oneShotAudio = null;
  }
  for (const oscillator of oneShotOscillators) {
    try { oscillator.stop(); } catch { /* already stopped */ }
    try { oscillator.disconnect(); } catch { /* ignore */ }
  }
  oneShotOscillators.clear();
}

function playChime(volume = 1, trackOneShot = false, expectedOneShotGeneration = oneShotGeneration) {
  try {
    if (trackOneShot && expectedOneShotGeneration !== oneShotGeneration) return;
    const ctx = getCtx();
    if (ctx.state === "closed") return;
    const normalizedVolume = clampVolume(volume);

    const now = ctx.currentTime;
    for (const note of CHIME_NOTES) {
      const osc = ctx.createOscillator();
      osc.type = "sine";
      osc.frequency.value = note.freq;

      const osc2 = ctx.createOscillator();
      osc2.type = "sine";
      osc2.frequency.value = note.freq * 2;

      const gain = ctx.createGain();
      gain.gain.setValueAtTime(0, now + note.time);
      gain.gain.linearRampToValueAtTime(note.volume * normalizedVolume, now + note.time + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.001, now + note.time + note.duration);

      const gain2 = ctx.createGain();
      gain2.gain.setValueAtTime(0, now + note.time);
      gain2.gain.linearRampToValueAtTime(note.volume * 0.12 * normalizedVolume, now + note.time + 0.02);
      gain2.gain.exponentialRampToValueAtTime(0.001, now + note.time + note.duration * 0.7);

      osc.connect(gain);
      osc2.connect(gain2);
      gain.connect(ctx.destination);
      gain2.connect(ctx.destination);

      if (trackOneShot) {
        oneShotOscillators.add(osc);
        oneShotOscillators.add(osc2);
        const cleanup = () => {
          oneShotOscillators.delete(osc);
          oneShotOscillators.delete(osc2);
        };
        osc.addEventListener("ended", cleanup, { once: true });
      }

      osc.start(now + note.time);
      osc.stop(now + note.time + note.duration);
      osc2.start(now + note.time);
      osc2.stop(now + note.time + note.duration * 0.7);
    }
  } catch (e) {
    console.error("[alertSound] playChime error:", e);
  }
}

/** Play the chime melody once (~4 seconds). Stops any previous one-shot before starting. */
export function playAlertChime(choice?: SoundChoice, volume = 1) {
  stopOneShotAlert();
  const myOneShotGeneration = oneShotGeneration;

  const url = resolveSoundUrl(choice);
  if (url) {
    const audio = new Audio(url);
    audio.preload = "auto";
    audio.volume = clampVolume(volume);
    oneShotAudio = audio;
    audio.addEventListener("ended", () => { if (oneShotAudio === audio) oneShotAudio = null; }, { once: true });
    audio.addEventListener("error", () => { if (oneShotAudio === audio) oneShotAudio = null; }, { once: true });
    void audio.play().catch((error) => {
      if (myOneShotGeneration !== oneShotGeneration) return;
      try { audio.pause(); audio.currentTime = 0; } catch { /* ignore */ }
      activeAudio.delete(audio);
      console.error("[alertSound] audio asset playback failed, falling back to synth:", error);
      if (oneShotAudio === audio) oneShotAudio = null;
      playChime(volume, true, myOneShotGeneration);
    });
    return;
  }
  playChime(volume, true, myOneShotGeneration);
}

/**
 * Start looping the chime melody (for alarms).
 * Each call gets its own generation tag; if stopLoopingAlert() bumps the
 * global generation, this loop detects the mismatch and self-terminates.
 */
export function startLoopingAlert(choice?: SoundChoice, volume = 1) {
  const myGen = ++generation;
  const url = resolveSoundUrl(choice);

  if (url) {
    const audio = new Audio(url);
    audio.preload = "auto";
    audio.loop = true;
    audio.volume = clampVolume(volume);
    loopAudio = audio;
    activeAudio.add(audio);
    audio.addEventListener("error", () => {
      activeAudio.delete(audio);
      if (loopAudio === audio) loopAudio = null;
    }, { once: true });
    void audio.play().catch((error) => {
      console.error("[alertSound] looping audio asset failed, falling back to synth:", error);
      activeAudio.delete(audio);
      if (loopAudio === audio) loopAudio = null;
      if (myGen === generation) {
        loopTimer = setTimeout(tick, 0);
      }
    });
    return;
  }

  function tick() {
    // If generation changed, we're an orphan — die silently
    if (myGen !== generation) return;
    playChime(volume);
    loopTimer = setTimeout(tick, LOOP_INTERVAL_SECONDS * 1000);
  }
  tick();
}

/**
 * Stop ALL looping alerts — including any orphans from rapid-fire calls.
 * Bumps the generation counter so every existing loop self-terminates,
 * then closes the AudioContext for instant silence.
 */
export function stopLoopingAlert() {
  generation++; // kills all running loops on next tick check
  stopOneShotAlert();
  if (loopTimer) {
    clearTimeout(loopTimer);
    loopTimer = null;
  }
  for (const audio of activeAudio) {
    try {
      audio.pause();
      audio.currentTime = 0;
    } catch { /* ignore */ }
  }
  activeAudio.clear();
  loopAudio = null;
  if (audioCtx && audioCtx.state !== "closed") {
    try { audioCtx.close(); } catch { /* ignore */ }
  }
  audioCtx = null;
}

export async function initAlertAudioStopListener() {
  if (stopListenerInitialized) return;
  stopListenerInitialized = true;
  try {
    await listen<{ reason?: string; source?: string }>(ALERT_AUDIO_STOP_EVENT, (event) => {
      if (event.payload?.source === audioWindowId) return;
      stopLoopingAlert();
    });
  } catch (error) {
    stopListenerInitialized = false;
    console.warn("[alertSound] alert audio stop listener failed:", error);
  }
}

export function stopAllAlertAudio(reason = "dismiss") {
  stopLoopingAlert();
  void emit(ALERT_AUDIO_STOP_EVENT, { reason, source: audioWindowId }).catch((error) => {
    console.warn("[alertSound] alert audio stop broadcast failed:", error);
  });
}

export function unlockAlertAudio() {
  if (unlocked) return;
  unlocked = true;
  try {
    const ctx = getCtx();
    if (ctx.state === "suspended") {
      void ctx.resume();
    }
  } catch (error) {
    console.warn("[alertSound] audio unlock failed:", error);
  }
}
