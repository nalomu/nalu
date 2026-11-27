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

interface MidiSynthNote {
  midi: number;
  time: number;
  duration: number;
  velocity: number;
  voice: "bell" | "pad";
}

export function midiToFrequency(midi: number): number {
  return 440 * Math.pow(2, (midi - 69) / 12);
}

// MIDI-driven Dmaj9 chime melody for Classic Synth & Web Audio fallback
const SYNTH_CHIME_NOTES: MidiSynthNote[] = [
  // Warm chord bed (Dmaj9 -> Gmaj7 -> Dmaj9)
  { midi: 62, time: 0.00, duration: 1.05, velocity: 54, voice: "pad" }, // D4
  { midi: 69, time: 0.02, duration: 1.03, velocity: 50, voice: "pad" }, // A4
  { midi: 67, time: 0.80, duration: 0.90, velocity: 52, voice: "pad" }, // G4
  { midi: 71, time: 0.82, duration: 0.88, velocity: 48, voice: "pad" }, // B4
  { midi: 62, time: 1.48, duration: 1.45, velocity: 54, voice: "pad" }, // D4
  { midi: 69, time: 1.50, duration: 1.43, velocity: 56, voice: "pad" }, // A4
  { midi: 76, time: 1.52, duration: 1.40, velocity: 46, voice: "pad" }, // E5

  // Bell-like upper chime phrase
  { midi: 78, time: 0.00, duration: 0.95, velocity: 84, voice: "bell" }, // F#5
  { midi: 81, time: 0.24, duration: 0.95, velocity: 82, voice: "bell" }, // A5
  { midi: 86, time: 0.50, duration: 1.15, velocity: 92, voice: "bell" }, // D6
  { midi: 85, time: 0.84, duration: 0.85, velocity: 76, voice: "bell" }, // C#6
  { midi: 81, time: 1.12, duration: 0.90, velocity: 78, voice: "bell" }, // A5
  { midi: 78, time: 1.48, duration: 1.45, velocity: 88, voice: "bell" }, // F#5
  { midi: 86, time: 1.52, duration: 1.38, velocity: 60, voice: "bell" }, // D6 shimmer
];

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

function scheduleSynthPartial(
  ctx: AudioContext,
  destination: AudioNode,
  options: {
    type: OscillatorType;
    freq: number;
    startTime: number;
    attack: number;
    duration: number;
    peakGain: number;
    trackOneShot: boolean;
  }
) {
  const osc = ctx.createOscillator();
  osc.type = options.type;
  osc.frequency.value = options.freq;

  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0.0001, options.startTime);
  gain.gain.linearRampToValueAtTime(Math.max(0.0002, options.peakGain), options.startTime + options.attack);
  gain.gain.exponentialRampToValueAtTime(0.0001, options.startTime + options.duration);

  osc.connect(gain);
  gain.connect(destination);

  if (options.trackOneShot) {
    oneShotOscillators.add(osc);
    const cleanup = () => {
      oneShotOscillators.delete(osc);
      try { osc.disconnect(); } catch { /* ignore */ }
      try { gain.disconnect(); } catch { /* ignore */ }
    };
    osc.addEventListener?.("ended", cleanup, { once: true });
  }

  osc.start(options.startTime);
  osc.stop(options.startTime + options.duration + 0.02);
}

function playChime(volume = 1, trackOneShot = false, expectedOneShotGeneration = oneShotGeneration) {
  try {
    if (trackOneShot && expectedOneShotGeneration !== oneShotGeneration) return;
    const ctx = getCtx();
    if (ctx.state === "closed") return;
    const normalizedVolume = clampVolume(volume);
    if (normalizedVolume <= 0) return;

    const masterGain = ctx.createGain();
    masterGain.gain.value = normalizedVolume * 0.42;

    let synthBus: AudioNode = masterGain;
    if (typeof ctx.createBiquadFilter === "function") {
      const toneFilter = ctx.createBiquadFilter();
      toneFilter.type = "lowpass";
      toneFilter.frequency.value = 4200;
      toneFilter.Q.value = 0.7;
      toneFilter.connect(masterGain);
      synthBus = toneFilter;
    }
    masterGain.connect(ctx.destination);

    const now = ctx.currentTime + 0.01;
    for (const note of SYNTH_CHIME_NOTES) {
      const freq = midiToFrequency(note.midi);
      const velocityGain = (note.velocity / 127) * (note.voice === "bell" ? 0.55 : 0.24);
      const startTime = now + note.time;

      if (note.voice === "bell") {
        // Fundamental tone
        scheduleSynthPartial(ctx, synthBus, {
          type: "sine",
          freq,
          startTime,
          attack: 0.012,
          duration: note.duration,
          peakGain: velocityGain,
          trackOneShot,
        });
        // Warm octave harmonic
        scheduleSynthPartial(ctx, synthBus, {
          type: "sine",
          freq: freq * 2,
          startTime,
          attack: 0.01,
          duration: note.duration * 0.62,
          peakGain: velocityGain * 0.22,
          trackOneShot,
        });
        // Crisp struck bell shimmer overtone
        scheduleSynthPartial(ctx, synthBus, {
          type: "sine",
          freq: freq * 3,
          startTime,
          attack: 0.006,
          duration: Math.min(0.28, note.duration * 0.28),
          peakGain: velocityGain * 0.07,
          trackOneShot,
        });
      } else {
        // Warm pad/EP harmonic bed
        scheduleSynthPartial(ctx, synthBus, {
          type: "triangle",
          freq,
          startTime,
          attack: 0.035,
          duration: note.duration,
          peakGain: velocityGain,
          trackOneShot,
        });
        scheduleSynthPartial(ctx, synthBus, {
          type: "sine",
          freq: freq * 2,
          startTime,
          attack: 0.03,
          duration: note.duration * 0.55,
          peakGain: velocityGain * 0.09,
          trackOneShot,
        });
      }
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
