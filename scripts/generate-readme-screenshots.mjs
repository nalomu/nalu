import { chromium } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";

const baseURL = process.env.README_SCREENSHOT_BASE_URL || "http://localhost:1420";
const outputDir = path.resolve("static/readme");

const now = new Date("2026-06-20T09:30:00+08:00");
const today = "2026-06-20";

function task(id, title, columnId, position, done = false, extra = {}) {
  return {
    id,
    project: extra.project || today,
    title,
    done,
    progress: done ? 100 : 0,
    column_id: columnId,
    position,
    created_at: `${today}T08:00:00`,
    updated_at: `${today}T09:00:00`,
    scheduled_start_at: extra.scheduled_start_at ?? null,
    scheduled_end_at: extra.scheduled_end_at ?? null,
    reminder_minutes: extra.reminder_minutes ?? 0,
    completed_at: done ? `${today}T10:30:00` : null,
    repeat_type: extra.repeat_type || "none",
    recurrence_series_id: null,
    recurrence_sequence: null,
    recurrence_origin_at: null,
    recurrence_detached: false,
  };
}

const columns = [
  { id: "col-plan", project: today, name: "计划中", sort_order: 0, created_at: `${today}T08:00:00`, updated_at: `${today}T08:00:00` },
  { id: "col-doing", project: today, name: "进行中", sort_order: 1, created_at: `${today}T08:00:00`, updated_at: `${today}T08:00:00` },
  { id: "col-done", project: today, name: "已完成", sort_order: 2, created_at: `${today}T08:00:00`, updated_at: `${today}T08:00:00` },
];

const boardTasks = [
  task("task-1", "整理本周重点任务", "col-plan", 0, false),
  task("task-2", "复盘产品反馈并标记后续动作", "col-plan", 1, false),
  task("task-3", "准备 10:30 方案讨论", "col-doing", 0, false, {
    scheduled_start_at: `${today}T10:30:00`,
    scheduled_end_at: `${today}T11:20:00`,
    reminder_minutes: 15,
  }),
  task("task-4", "完成番茄钟专注轮次", "col-done", 0, true),
];

const board = [
  {
    project: today,
    sort_order: 0,
    columns: columns.map((column) => ({
      column,
      tasks: boardTasks.filter((item) => item.column_id === column.id),
    })),
  },
  {
    project: "产品想法",
    sort_order: 1,
    columns: [
      {
        column: { id: "col-ideas", project: "产品想法", name: "待评估", sort_order: 0, created_at: `${today}T08:00:00`, updated_at: `${today}T08:00:00` },
        tasks: [
          task("task-5", "把常用工作流沉淀为模板", "col-ideas", 0, false, { project: "产品想法" }),
          task("task-6", "设计移动端快速记录入口", "col-ideas", 1, false, { project: "产品想法" }),
        ],
      },
    ],
  },
];

const calendarTasks = [
  task("cal-1", "每日计划", "col-doing", 0, false, {
    scheduled_start_at: `${today}T09:00:00`,
    scheduled_end_at: `${today}T09:40:00`,
    reminder_minutes: 10,
  }),
  task("cal-2", "方案讨论", "col-doing", 1, false, {
    scheduled_start_at: `${today}T10:30:00`,
    scheduled_end_at: `${today}T11:20:00`,
    reminder_minutes: 15,
  }),
  task("cal-3", "专注写作", "col-plan", 2, false, {
    scheduled_start_at: `${today}T14:00:00`,
    scheduled_end_at: `${today}T15:30:00`,
    reminder_minutes: 5,
  }),
  task("cal-4", "收尾和同步", "col-plan", 3, false, {
    scheduled_start_at: `${today}T17:00:00`,
    scheduled_end_at: `${today}T17:30:00`,
    reminder_minutes: 0,
  }),
];

const notes = [
  {
    id: "note-1",
    title: "项目备忘",
    content: "记录想法、决策和待办，不包含真实客户、账号或私密信息。",
    tags: "demo,local-first",
    note_type: "memo",
    created_at: `${today}T08:00:00`,
    updated_at: `${today}T09:00:00`,
  },
  {
    id: "note-2",
    title: "会议纪要模板",
    content: "目标、结论、行动项。",
    tags: "template",
    note_type: "memo",
    created_at: `${today}T08:00:00`,
    updated_at: `${today}T09:00:00`,
  },
];

const schedules = calendarTasks.map((item) => ({
  id: item.id,
  title: item.title,
  scheduled_at: item.scheduled_start_at,
  reminder_minutes: item.reminder_minutes,
  done: item.done,
  created_at: item.created_at,
}));

const tauriMock = `
  const board = ${JSON.stringify(board)};
  const tasks = ${JSON.stringify(boardTasks.concat(calendarTasks))};
  const notes = ${JSON.stringify(notes)};
  const schedules = ${JSON.stringify(schedules)};
  const calendarTasks = ${JSON.stringify(calendarTasks)};
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      if (cmd === "runtime_platform") return window.__NALU_RUNTIME_PLATFORM__ || "desktop";
      if (cmd === "get_board") return board;
      if (cmd === "get_tasks") return tasks;
      if (cmd === "get_notes") return notes;
      if (cmd === "get_schedules") return schedules;
      if (cmd === "get_calendar_tasks") return calendarTasks;
      if (cmd === "get_clipboard_history") return [
        { id: "clip-1", content: "示例剪贴文本", content_type: "text", created_at: "${today}T09:10:00" },
        { id: "clip-2", content: "https://example.local", content_type: "text", created_at: "${today}T09:05:00" }
      ];
      if (cmd === "get_alarms") return [
        { id: "alarm-1", time: "08:30", label: "晨间计划", repeat: "weekdays", active: true, skip_next: false, created_at: "${today}T08:00:00" },
        { id: "alarm-2", time: "18:00", label: "收尾提醒", repeat: "daily", active: true, skip_next: false, created_at: "${today}T08:00:00" }
      ];
      if (cmd === "pomodoro_get_state") return {
        is_running: true, is_break: false,
        remaining_seconds: 1180, work_duration: 1500,
        break_duration: 300, completed_count: 3,
      };
      if (cmd === "pomodoro_set_duration") return {
        is_running: false, is_break: false,
        remaining_seconds: (args?.workMinutes || 25) * 60,
        work_duration: (args?.workMinutes || 25) * 60,
        break_duration: (args?.breakMinutes || 5) * 60,
        completed_count: 3,
      };
      if (cmd === "sync_get_config") return null;
      return null;
    },
    transformCallback: (cb) => cb,
  };
  window.__TAURI__ = {
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: "main", hide: async () => {}, show: async () => {}, isVisible: async () => true }) },
    core: { invoke: window.__TAURI_INTERNALS__.invoke },
  };
  window.localStorage.setItem("nalu-locale", "zh");
`;

async function sanitizePage(page) {
  await page.evaluate(() => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (const node of nodes) {
      node.nodeValue = node.nodeValue.replaceAll("Nalomu", "本地用户");
    }
  });
}

async function capture(browser, route, filename, viewport, platform = "desktop") {
  const page = await browser.newPage();
  await page.setViewportSize(viewport);
  await page.addInitScript(tauriMock);
  await page.addInitScript(`window.__NALU_RUNTIME_PLATFORM__ = ${JSON.stringify(platform)};`);
  await page.goto(`${baseURL}${route}`);
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(900);
  await sanitizePage(page);
  await page.screenshot({
    path: path.join(outputDir, filename),
    fullPage: true,
  });
  await page.close();
}

await fs.mkdir(outputDir, { recursive: true });
const browser = await chromium.launch({ headless: true });

try {
  await capture(browser, "/", "dashboard.png", { width: 1440, height: 960 });
  await capture(browser, "/#/tasks", "tasks.png", { width: 1440, height: 960 });
  await capture(browser, "/#/schedule", "schedule.png", { width: 1440, height: 960 });
  await capture(browser, "/#/tasks", "mobile-tasks.png", { width: 390, height: 844 }, "android");
} finally {
  await browser.close();
}

console.log(`README screenshots written to ${outputDir}`);
