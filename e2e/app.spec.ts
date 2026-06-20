import { test, expect } from "@playwright/test";

const tauriMock = `
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      if (cmd === "runtime_platform") return window.__NALU_RUNTIME_PLATFORM__ || "desktop";
      if (cmd === "get_tasks") return [];
      if (cmd === "get_notes") return [{ id: "note-1", title: "移动笔记", content: "手机端编辑内容", tags: "mobile", note_type: "memo" }];
      if (cmd === "get_clipboard_history") return [];
      if (cmd === "get_alarms") return [];
      if (cmd === "pomodoro_get_state") return {
        is_running: false, is_break: false,
        remaining_seconds: 1500, work_duration: 1500,
        break_duration: 300, completed_count: 0,
      };
      if (cmd === "get_schedules") return [];
      if (cmd === "pomodoro_set_duration") return {
        is_running: false, is_break: false,
        remaining_seconds: (args?.workMinutes || 25) * 60,
        work_duration: (args?.workMinutes || 25) * 60,
        break_duration: (args?.breakMinutes || 5) * 60,
        completed_count: 0,
      };
      return null;
    },
    transformCallback: (cb) => cb,
  };
  window.__TAURI__ = {
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ hide: async () => {}, show: async () => {}, isVisible: async () => true }) },
    core: { invoke: window.__TAURI_INTERNALS__.invoke },
  };
`;

test.describe("Nalu App E2E", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(tauriMock);
  });

  // Helper: sidebar nav button
  const navBtn = (page: any, name: string) =>
    page.getByRole("navigation").getByRole("button", { name });

  test("app loads and shows sidebar navigation", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    const nav = page.getByRole("navigation");
    await expect(nav).toBeVisible();
    await expect(navBtn(page, "番茄钟")).toBeVisible();
    await expect(navBtn(page, "闹钟")).toBeVisible();
    await expect(navBtn(page, "设置")).toBeVisible();
  });

  test("navigate to pomodoro page", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await navBtn(page, "番茄钟").click();
    await page.waitForTimeout(500);

    // Timer display
    await expect(page.getByText("25:00")).toBeVisible();
    await expect(page.getByRole("heading", { name: "专注" })).toBeVisible();
  });

  test("navigate to alarm page", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await navBtn(page, "闹钟").click();
    await page.waitForTimeout(500);

    await expect(page.getByText("设置闹钟")).toBeVisible();
    await expect(page.locator('input[type="time"]')).toBeVisible();
  });

  test("pomodoro shows timer circle and duration inputs", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await navBtn(page, "番茄钟").click();
    await page.waitForTimeout(500);

    // SVG timer circle
    await expect(page.locator("svg circle").first()).toBeAttached();

    // Duration number inputs (work + break)
    const numberInputs = page.locator('input[type="number"]');
    await expect(numberInputs).toHaveCount(2);
  });

  test("alarm form has all required fields", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await navBtn(page, "闹钟").click();
    await page.waitForTimeout(500);

    // Time input
    await expect(page.locator('input[type="time"]')).toBeVisible();

    // Label text input
    await expect(page.locator('input[type="text"]')).toBeVisible();

    // Repeat select with correct options
    const select = page.locator("select").first();
    await expect(select).toBeVisible();
    await expect(select.locator('option[value="none"]')).toHaveCount(1);
    await expect(select.locator('option[value="daily"]')).toHaveCount(1);
    await expect(select.locator('option[value="weekdays"]')).toHaveCount(1);
    await expect(select.locator('option[value="weekends"]')).toHaveCount(1);
  });

  test("multiple page navigation works", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Dashboard → Pomodoro
    await navBtn(page, "番茄钟").click();
    await page.waitForTimeout(300);
    await expect(page.getByText("25:00")).toBeVisible();

    // Pomodoro → Alarm
    await navBtn(page, "闹钟").click();
    await page.waitForTimeout(300);
    await expect(page.getByText("设置闹钟")).toBeVisible();

    // Alarm → Settings
    await navBtn(page, "设置").click();
    await page.waitForTimeout(300);
    await expect(page.locator("main h1").first()).toBeAttached();
  });

  test("narrow desktop window keeps desktop platform features", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await expect(navBtn(page, "剪贴板")).toBeVisible();
    await expect(navBtn(page, "MySQL")).toBeVisible();
    await expect(navBtn(page, "设置")).toBeVisible();
  });

  test("android platform shows mobile tabs and redirects desktop-only routes", async ({ page }) => {
    await page.addInitScript(() => {
      window.__NALU_RUNTIME_PLATFORM__ = "android";
    });
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/#/mysql");
    await page.waitForLoadState("networkidle");

    await expect(page).toHaveURL(/#\/?$/);
    const nav = page.getByRole("navigation");
    for (const name of ["仪表盘", "任务", "笔记", "番茄钟", "我的"]) {
      await expect(nav.getByRole("button", { name })).toBeVisible();
    }
    for (const name of ["日程", "闹钟", "AI 助手"]) {
      await expect(nav.getByRole("button", { name })).toHaveCount(0);
    }
    await expect(nav.getByRole("button", { name: "剪贴板" })).toHaveCount(0);
    await expect(nav.getByRole("button", { name: "MySQL" })).toHaveCount(0);
  });

  test("android redirects schedule and alarm routes to dashboard", async ({ page }) => {
    await page.addInitScript(() => {
      window.__NALU_RUNTIME_PLATFORM__ = "android";
    });
    await page.setViewportSize({ width: 390, height: 800 });

    await page.goto("/#/schedule");
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveURL(/#\/?$/);

    await page.goto("/#/alarm");
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveURL(/#\/?$/);
  });

  test("android notes use list then detail flow", async ({ page }) => {
    await page.addInitScript(() => {
      window.__NALU_RUNTIME_PLATFORM__ = "android";
    });
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/#/notes");
    await page.waitForLoadState("networkidle");

    await page.getByRole("button", { name: /移动笔记/ }).click();
    await expect(page.locator('textarea[placeholder="开始写作..."]')).toBeVisible();
    await page.locator("button").first().click();
    await expect(page.getByRole("button", { name: /移动笔记/ })).toBeVisible();
  });

  test("android settings hide desktop-only options", async ({ page }) => {
    await page.addInitScript(() => {
      window.__NALU_RUNTIME_PLATFORM__ = "android";
    });
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/#/settings");
    await page.waitForLoadState("networkidle");

    await expect(page.getByRole("heading", { name: "语言" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "手机入口" })).toBeVisible();
    await expect(page.getByRole("button", { name: /AI 助手/ })).toBeVisible();
    await expect(page.getByRole("heading", { name: "主题" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "铃声" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "AI 配置" })).toBeVisible();
    await expect(page.getByText("闹钟铃声")).toHaveCount(0);
    await expect(page.getByText("开机自启")).toHaveCount(0);
    await expect(page.getByText("剪贴板")).toHaveCount(0);
  });
});
