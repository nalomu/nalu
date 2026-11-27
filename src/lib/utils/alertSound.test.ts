/**
 * Tests for alertSound module.
 * Mocks the Web Audio API since jsdom doesn't provide it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// ── Web Audio API Mock ────────────────────────────────────
const createdOscillators: MockOscillatorNode[] = [];
const createdContexts: MockAudioContext[] = [];

class MockOscillatorNode {
  type = "sine";
  frequency = { value: 0 };
  playbackState = 1; // RUNNING
  _stopped = false;
  start = vi.fn();
  stop = vi.fn(() => { this._stopped = true; this.playbackState = 3; });
  connect = vi.fn();
  disconnect = vi.fn();
  addEventListener = vi.fn();
}

class MockGainNode {
  gain = {
    value: 0,
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  };
  connect = vi.fn();
  disconnect = vi.fn();
}

class MockBiquadFilterNode {
  type = "lowpass";
  frequency = { value: 0 };
  Q = { value: 0 };
  connect = vi.fn();
  disconnect = vi.fn();
}

class MockAudioContext {
  state = "running";
  currentTime = 0;
  destination = {};
  constructor() {
    createdContexts.push(this);
  }
  createOscillator = vi.fn(() => {
    const osc = new MockOscillatorNode();
    createdOscillators.push(osc);
    return osc;
  });
  createGain = vi.fn(() => new MockGainNode());
  createBiquadFilter = vi.fn(() => new MockBiquadFilterNode());
  resume = vi.fn(() => Promise.resolve());
  close = vi.fn(() => {
    this.state = "closed";
    return Promise.resolve();
  });
}

// Install the mock globally
vi.stubGlobal("AudioContext", MockAudioContext);

// Import AFTER mocking
const { midiToFrequency, playAlertChime, startLoopingAlert, stopLoopingAlert } = await import(
  "$lib/utils/alertSound"
);

describe("alertSound", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    createdOscillators.length = 0;
    createdContexts.length = 0;
  });

  afterEach(() => {
    stopLoopingAlert();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("converts MIDI note numbers to standard frequencies", () => {
    expect(midiToFrequency(69)).toBeCloseTo(440, 5);
    expect(midiToFrequency(60)).toBeCloseTo(261.6256, 3);
    expect(midiToFrequency(81)).toBeCloseTo(880, 5);
  });

  it("playAlertChime schedules MIDI synth voices and stops previous one-shot oscillators", () => {
    playAlertChime({ type: "synth" }, 0.8);
    const firstBatch = [...createdOscillators];
    expect(firstBatch.length).toBeGreaterThan(0);

    // Calling again stops the previous one-shot oscillators cleanly
    playAlertChime({ type: "synth" }, 0.8);
    expect(firstBatch.every((osc) => osc._stopped)).toBe(true);
    expect(createdOscillators.length).toBeGreaterThan(firstBatch.length);
  });

  it("startLoopingAlert plays chime and schedules next", () => {
    startLoopingAlert({ type: "synth" }, 0.8);
    const firstCount = createdOscillators.length;
    expect(firstCount).toBeGreaterThan(0);

    // Advance 4.0s loop interval → second chime
    vi.advanceTimersByTime(4000);
    expect(createdOscillators.length).toBeGreaterThan(firstCount);
  });

  it("stopLoopingAlert stops the loop", () => {
    startLoopingAlert({ type: "synth" }, 0.8);
    const countBeforeStop = createdOscillators.length;

    // Stop after first chime
    stopLoopingAlert();

    // Advance past where next chime would be
    vi.advanceTimersByTime(5000);
    expect(createdOscillators.length).toBe(countBeforeStop);
  });

  it("stopLoopingAlert kills active oscillators", () => {
    startLoopingAlert({ type: "synth" }, 0.8);
    expect(() => stopLoopingAlert()).not.toThrow();
    expect(createdContexts[createdContexts.length - 1]?.close).toHaveBeenCalled();
  });

  it("can start and stop multiple times", () => {
    startLoopingAlert({ type: "synth" }, 0.8);
    stopLoopingAlert();
    startLoopingAlert({ type: "synth" }, 0.8);
    stopLoopingAlert();
    startLoopingAlert({ type: "synth" }, 0.8);
    stopLoopingAlert();
    expect(createdContexts.length).toBe(3);
  });
});
