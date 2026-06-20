import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

function mockMatchMedia(initialMatches: boolean, legacy = false) {
  let matches = initialMatches;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const media = {
    get matches() {
      return matches;
    },
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener: legacy ? undefined : vi.fn((_event: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    }),
    removeEventListener: legacy ? undefined : vi.fn((_event: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    }),
    addListener: vi.fn((listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    }),
    removeListener: vi.fn((listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    }),
    dispatchEvent: vi.fn(),
  } as unknown as MediaQueryList;

  vi.stubGlobal("matchMedia", vi.fn(() => media));

  return {
    media,
    setMatches(next: boolean) {
      matches = next;
      listeners.forEach((listener) => listener({ matches: next, media: media.media } as MediaQueryListEvent));
    },
  };
}

describe("theme", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.className = "";
    document.documentElement.removeAttribute("style");
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("defaults system theme to dark when the OS prefers dark", async () => {
    const { effectiveTheme, initTheme } = await import("./theme");
    mockMatchMedia(true);

    const cleanup = initTheme();

    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(effectiveTheme()).toBe("dark");

    cleanup();
  });

  it("updates while in system mode when the OS preference changes", async () => {
    const { initTheme } = await import("./theme");
    const matchMedia = mockMatchMedia(false);

    const cleanup = initTheme();
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    matchMedia.setMatches(true);

    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");

    cleanup();
  });

  it("does not let OS changes override an explicit light theme", async () => {
    const { effectiveTheme, initTheme, setTheme } = await import("./theme");
    const matchMedia = mockMatchMedia(false);
    const cleanup = initTheme();

    setTheme("light");
    matchMedia.setMatches(true);

    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(effectiveTheme()).toBe("light");

    cleanup();
  });

  it("falls back to legacy media query listeners", async () => {
    const { initTheme } = await import("./theme");
    const matchMedia = mockMatchMedia(false, true);

    const cleanup = initTheme();
    matchMedia.setMatches(true);

    expect(document.documentElement.classList.contains("dark")).toBe(true);

    cleanup();
  });

  it("uses the Tauri window theme when WebView media query reports light", async () => {
    vi.doMock("@tauri-apps/api/window", () => ({
      getCurrentWindow: () => ({
        theme: async () => "dark",
        onThemeChanged: async () => () => {},
      }),
    }));
    const { effectiveTheme, initTheme } = await import("./theme");
    mockMatchMedia(false);

    const cleanup = initTheme();
    await vi.waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });

    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(effectiveTheme()).toBe("dark");

    cleanup();
  });

  it("uses the Rust system theme command when WebView media query reports light", async () => {
    vi.doMock("@tauri-apps/api/window", () => ({
      getCurrentWindow: () => ({
        theme: async () => null,
        onThemeChanged: async () => () => {},
      }),
    }));
    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: async (command: string) => {
        if (command === "get_system_theme") return "dark";
        throw new Error(`unexpected command: ${command}`);
      },
    }));
    vi.doMock("@tauri-apps/api/event", () => ({
      listen: async () => () => {},
    }));
    const { effectiveTheme, initTheme } = await import("./theme");
    mockMatchMedia(false);

    const cleanup = initTheme();
    await vi.waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });

    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(effectiveTheme()).toBe("dark");

    cleanup();
  });
});
