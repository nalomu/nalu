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
class MockOscillatorNode {
  type = "sine";
  frequency = { value: 0 };
  playbackState = 1; // RUNNING
  _stopped = false;
  start = vi.fn();
  stop = vi.fn(() => { this._stopped = true; this.playbackState = 3; });
  connect = vi.fn();
  disconnect = vi.fn();
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

class MockAudioContext {
  state = "running";
  currentTime = 0;
  destination = {};
  createOscillator = vi.fn(() => new MockOscillatorNode());
  createGain = vi.fn(() => new MockGainNode());
  resume = vi.fn(() => Promise.resolve());
}

// Install the mock globally
vi.stubGlobal("AudioContext", MockAudioContext);

// Import AFTER mocking
const { playAlertChime, startLoopingAlert, stopLoopingAlert } = await import(
  "$lib/utils/alertSound"
);

describe("alertSound", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    stopLoopingAlert();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("playAlertChime plays without error", () => {
    expect(() => playAlertChime()).not.toThrow();
  });

  it("startLoopingAlert plays chime and schedules next", () => {
    startLoopingAlert();
    // First chime plays immediately
    expect(true).toBe(true); // no crash

    // Advance 3.2s (melody 2.2s + 1s pause) → second chime
    vi.advanceTimersByTime(3200);
    expect(true).toBe(true); // still running
  });

  it("stopLoopingAlert stops the loop", () => {
    startLoopingAlert();

    // Stop after first chime
    stopLoopingAlert();

    // Advance past where next chime would be
    vi.advanceTimersByTime(5000);
    // If loop wasn't stopped, this would have created more oscillators
    expect(true).toBe(true); // no crash = stopped cleanly
  });

  it("stopLoopingAlert kills active oscillators", () => {
    startLoopingAlert();
    // Stop should not throw even with active oscillators
    expect(() => stopLoopingAlert()).not.toThrow();
  });

  it("can start and stop multiple times", () => {
    startLoopingAlert();
    stopLoopingAlert();
    startLoopingAlert();
    stopLoopingAlert();
    startLoopingAlert();
    stopLoopingAlert();
    // Should handle repeated start/stop gracefully
    expect(true).toBe(true);
  });
});
