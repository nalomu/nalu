# Nalu 实现细节与技术规范

**版本**：v0.3.0  
**日期**：2026-06-12  
**状态**：MVP 已完成，看板视图已上线

---

## 一、已实现功能清单

### 1. 仪表盘（Dashboard）
- 统计卡片：待办任务数、已完成数、笔记数、备忘录数、日程事件数
- 快捷导航：8 个可点击卡片跳转到各功能模块
- 最近任务列表：支持直接勾选完成（toggle_task）
- 最近笔记列表：展示标题、内容摘要、标签

### 2. 任务管理（Tasks）— 看板视图
- **看板布局**：按分组（Group）组织，每个分组内含多列（Column），列中展示任务卡片
- **分组管理**：默认分组 + 自定义分组，支持新建/重命名/复制/删除分组
- **分列管理**：默认"重要"/"一般"两列，支持新建/重命名/删除分列，列之间拖拽排序
- **任务卡片**：添加/编辑/删除/拖拽移动任务，跨列/跨分组拖拽，切换完成状态
- **拖拽排序**：任务支持 pointer 事件拖拽（触控板/鼠标），分组也支持拖拽重排
- **搜索过滤**：全局搜索任务标题，搜索时自动展开所有分组
- **撤销操作**：删除任务/分列后 5 秒内可撤销
- **进度追踪**：任务支持 0-100 进度（toggle 完成时 progress 同步为 0/100）
- SQLite 持久化，三表关联（tasks / task_columns / task_groups）

### 3. 笔记与备忘录（Notes）
- 双面板布局：左侧列表 + 右侧编辑器
- 支持笔记（note）和备忘（memo）两种类型
- 标签系统（逗号分隔）
- 全文搜索

### 4. 剪贴板管理（Clipboard）
- 监听模式：每 2 秒轮询系统剪贴板
- 文本支持：自动捕获文本内容
- 图片支持：检测剪贴板图片，通过 Canvas 转 base64 PNG data URL 存储
- 文件路径支持：content_type = "file"
- 历史列表：按时间倒序，支持复制和删除
- 快窗弹窗：`Shift+Cmd+C` 全局快捷键唤起紧凑弹窗，支持一键复制
- 清空全部功能

### 5. 番茄钟（Pomodoro）
- SVG 圆环进度条显示
- 工作/休息自动切换
- 自定义工作时长（默认 25 分钟）和休息时长（默认 5 分钟）
- 后台 Tokio 异步计时器（`tauri::async_runtime::spawn`）
- 事件通知：`pomodoro-tick`（每秒）、`pomodoro-work-end`、`pomodoro-break-end`
- 系统通知弹窗 + Web Audio API 蜂鸣音（800Hz 正弦波，0.5 秒）
- 已完成番茄计数

### 6. 日程管理（Schedule）
- 添加事件：标题 + 日期 + 时间
- 标记完成/删除
- 定时检查：每 60 秒扫描一次，在 `reminder_minutes` 窗口内触发系统通知 + 蜂鸣音
- 防重复通知：已通知的事件 ID 记录在 Set 中

### 7. 闹钟（Alarm）
- 设置时间（HH:MM）、标签、重复模式（不重复/每天/工作日/周末）
- 每 30 秒检查一次当前时间是否匹配
- 工作日/周末重复判断（基于 Date.getDay()）
- 系统通知 + Web Audio API 蜂鸣音
- 启用/禁用切换

### 8. MySQL 工具
- 连接管理：主机、端口、用户名、密码
- Root 密码记忆：成功连接后自动保存到 localStorage，下次自动填充
- 数据库列表：连接后自动拉取，过滤系统库
- 数据库导出：通过 `mysqldump` 命令生成带时间戳的 SQL 文件
- 数据库导入：选择 .sql 文件，读取内容通过 `query_drop` 执行
- SQL 查询：textarea 输入，支持 SELECT/SHOW/DESCRIBE 查询和非查询
- 用户管理：维护 用户名-密码-数据库 表，支持同步到 MySQL 服务器
- 用户导入/导出：JSON 文件格式，支持保存和加载

### 9. AI 助手（AiPage）
- **多 Provider 支持**：OpenAI、DeepSeek、自定义 OpenAI 兼容 API
- **推理模式（Reasoning）**：支持 OpenAI Responses API（reasoning enabled）和 DeepSeek thinking mode，可配置 reasoning_effort
- **Chat UI**：Vue 多轮对话界面，支持 Tool Call 卡片展示
- **[ACTION] 系统**：System prompt 由 Rust 后端注入应用上下文，AI 通过 `[ACTION] {"command":"xxx","params":{...}} [/ACTION]` 格式驱动应用操作（创建任务/笔记/日程/闹钟/剪贴板）
- **模型参数配置**：API URL、API Key、Model、Temperature 可调
- **测试连接**：设置页一键测试 API 连通性

### 10. 国际化（i18n）
- 中文（默认）和英文双语
- 点号分隔键查找：`t("nav.dashboard")` → `"仪表盘"`
- localStorage 持久化语言选择
- 响应式切换：`onLocaleChange` + `localeVersion` 强制重渲染
- 设置页语言切换按钮

### 11. 命令面板（Command Palette）
- `Cmd+K` 全局快捷键唤起
- 模糊搜索
- 键盘导航（上下选择、回车执行）
- 导航命令：跳转到所有功能模块

---

## 二、数据库 Schema（SQLite）

数据库文件位于 `app_data_dir/nalu.db`。

```sql
-- 任务（v0.3.0 增加了 progress / column_id / position）
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL DEFAULT 'default',
    title TEXT NOT NULL,
    done INTEGER NOT NULL DEFAULT 0,
    progress INTEGER NOT NULL DEFAULT 0,
    column_id TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 看板分列
CREATE TABLE IF NOT EXISTS task_columns (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL DEFAULT '任务',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 看板分组（project 即分组标识）
CREATE TABLE IF NOT EXISTS task_groups (
    project TEXT PRIMARY KEY,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 笔记/备忘
CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '',
    note_type TEXT NOT NULL DEFAULT 'memo',  -- 'memo' | 'note'
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 剪贴板历史
CREATE TABLE IF NOT EXISTS clipboard_history (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,                    -- 文本内容 / base64 图片 / 文件路径
    content_type TEXT NOT NULL DEFAULT 'text', -- 'text' | 'image' | 'file'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 日程
CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    scheduled_at TEXT NOT NULL,               -- ISO 8601 格式
    reminder_minutes INTEGER NOT NULL DEFAULT 5,
    done INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 闹钟
CREATE TABLE IF NOT EXISTS alarms (
    id TEXT PRIMARY KEY,
    time TEXT NOT NULL,                       -- "HH:MM" 格式
    label TEXT NOT NULL DEFAULT '',
    repeat TEXT NOT NULL DEFAULT 'none',      -- 'none' | 'daily' | 'weekdays' | 'weekends'
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- MySQL 用户管理
CREATE TABLE IF NOT EXISTS mysql_users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL DEFAULT '',
    databases TEXT NOT NULL DEFAULT '',       -- 逗号分隔的数据库名
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

所有表的 `id` 字段使用 `uuid::Uuid::new_v4()` 生成。  
连接管理使用全局静态 `LazyLock<Mutex<Option<Connection>>>`。

---

## 三、Tauri Commands API 参考

### Database
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `db_query` | `sql: String` | `Vec<HashMap>` | 执行 SELECT 查询 |
| `db_execute` | `sql: String` | `usize` | 执行 INSERT/UPDATE/DELETE |

### Tasks — 看板核心
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_board` | — | `Vec<GroupData>` | 获取完整看板数据（分组→分列→任务） |
| `move_task` | `id, target_column_id, target_position` | `Task` | 拖拽移动任务到目标列/位置 |
| `create_column_by_drag` | `task_id, project` | `(TaskColumn, Task)` | 拖拽任务到空白区域时自动创建新列 |
| `reorder_columns` | `column_ids: Vec<String>` | `()` | 列拖拽排序 |
| `rename_column` | `id, name` | `TaskColumn` | 重命名列 |
| `delete_column` | `id` | `ColumnSnapshot` | 删除空列（支持撤销），非空或最后一列报错 |
| `restore_column` | `snapshot: ColumnSnapshot` | `TaskColumn` | 撤销删除列 |

### Tasks — 分组管理
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `create_task_group` | `project` | `GroupData` | 创建新分组（自动创建默认列） |
| `delete_task_group` | `project` | `()` | 删除分组（需无未完成任务，default 组不可删） |
| `copy_task_group` | `project` | `GroupData` | 复制分组（仅复制未完成任务） |
| `rename_task_group` | `project, name` | `GroupData` | 重命名分组（default 组不可改） |
| `reorder_task_groups` | `projects: Vec<String>` | `()` | 分组拖拽排序 |

### Tasks — 任务操作（看板内）
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `add_task_to_column` | `title, column_id` | `Task` | 在指定列末尾添加任务 |
| `add_task_to_group` | `title, project` | `Task` | 在指定分组首列添加任务 |
| `update_task_content` | `id, title` | `Task` | 内联编辑任务标题 |
| `update_task_progress` | `id, progress` | `Task` | 更新任务进度（0-100），同步 done 字段 |
| `delete_task_with_snapshot` | `id` | `TaskSnapshot` | 删除任务并返回快照（支持撤销） |
| `restore_task` | `snapshot: TaskSnapshot` | `Task` | 撤销删除任务 |

### Tasks — 旧版兼容命令（仍可用）
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_tasks` | `project?: String` | `Vec<Task>` | 获取任务列表（仪表盘/旧组件使用） |
| `add_task` | `title, project?` | `Task` | 添加任务（自动放入首列） |
| `toggle_task` | `id` | `bool` | 切换完成状态（同步 progress 0/100） |
| `update_task` | `id, title` | `Task` | 更新任务标题 |
| `delete_task` | `id` | `()` | 简单删除任务（无撤销） |

### Notes
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_notes` | `note_type?, search?` | `Vec<Note>` | 获取笔记列表 |
| `add_note` | `title, content?, tags?, note_type?` | `Note` | 添加笔记 |
| `update_note` | `id, title?, content?, tags?` | `()` | 更新笔记 |
| `delete_note` | `id` | `()` | 删除笔记 |

### Clipboard
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_clipboard_history` | `limit?: i32` | `Vec<ClipboardEntry>` | 获取历史 |
| `add_clipboard_entry` | `content, content_type?` | `ClipboardEntry` | 添加条目 |
| `get_clipboard_entry` | `id` | `ClipboardEntry` | 获取单个条目 |
| `delete_clipboard_entry` | `id` | `()` | 删除条目 |
| `clear_clipboard_history` | — | `usize` | 清空历史 |

### Pomodoro
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `pomodoro_get_state` | — | `PomodoroState` | 获取状态 |
| `pomodoro_start` | `app: AppHandle` | `()` | 开始计时（spawn 后台任务） |
| `pomodoro_pause` | — | `PomodoroState` | 暂停 |
| `pomodoro_reset` | — | `PomodoroState` | 重置 |
| `pomodoro_skip` | — | `PomodoroState` | 跳过当前阶段 |
| `pomodoro_set_duration` | `work_minutes, break_minutes` | `PomodoroState` | 设置时长 |

### Schedule
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_schedules` | — | `Vec<Schedule>` | 获取日程列表 |
| `add_schedule` | `title, scheduled_at, reminder_minutes?` | `Schedule` | 添加日程 |
| `toggle_schedule` | `id` | `bool` | 切换完成状态 |
| `delete_schedule` | `id` | `()` | 删除日程 |

### Alarm
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_alarms` | — | `Vec<Alarm>` | 获取闹钟列表 |
| `add_alarm` | `time, label, repeat` | `Alarm` | 添加闹钟 |
| `toggle_alarm` | `id` | `bool` | 切换启用状态 |
| `delete_alarm` | `id` | `()` | 删除闹钟 |

### MySQL
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `mysql_test_connection` | `config: MysqlConfig` | `bool` | 测试连接 |
| `mysql_query` | `config, sql` | `MysqlResult` | 执行查询 |
| `mysql_execute` | `config, sql` | `u64` | 执行非查询 SQL |
| `mysql_list_databases` | `config` | `Vec<String>` | 列出数据库（过滤系统库） |
| `mysql_export` | `config, export_dir, table?` | `String` | mysqldump 导出 |
| `mysql_import` | `config, file_path` | `u64` | 导入 SQL 文件 |

### MySQL Users
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_mysql_users` | — | `Vec<MysqlUser>` | 获取管理的用户列表 |
| `add_mysql_user` | `username, password, databases` | `MysqlUser` | 添加用户记录 |
| `update_mysql_user` | `id, password?, databases?` | `()` | 更新用户信息 |
| `delete_mysql_user` | `id` | `()` | 删除用户记录 |
| `mysql_create_user_on_server` | `config, new_username, new_password, grant_databases` | `bool` | 同步到 MySQL 服务器 |

### AI
| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `ai_chat` | `config: AiConfig, messages: Vec<AiMessage>, context: String` | `AiResponse` | 注入 system prompt + 应用上下文，发送聊天请求 |

**AiConfig 结构**：
```
provider: "openai" | "deepseek" | "custom"
api_key, api_url, model: String
reasoning_enabled?: bool          // 启用推理模式（OpenAI → Responses API, DeepSeek → thinking）
reasoning_effort?: "low"|"medium"|"high"|"max"  // 推理深度
temperature?: f32                 // 可选温度参数
```

**AiResponse 结构**：
```
content: String                   // 模型输出文本
tokens_used?: u32                 // token 消耗
reasoning_content?: String        // 推理摘要/思考过程（DeepSeek thinking / OpenAI Responses reasoning summary）
```

**System prompt 注入**：Rust 后端根据 `context`（当前应用数据）全量拼装 system prompt，定义 [ACTION] 格式和允许的命令列表（tasks/notes/schedules/alarms/clipboard）。前端仅传消息列表，不构建 system prompt。

---

## 四、前端组件结构

```
src/
├── App.vue                     # Vue 根组件
├── main.ts                     # Vue、Router、Pinia 入口
├── router.ts                   # Hash 路由配置
├── lib/
│   ├── i18n/
│   │   ├── index.ts            # t() 函数 + setLocale() + onLocaleChange()
│   │   ├── zh.ts               # 中文翻译
│   │   └── en.ts               # 英文翻译
│   ├── types.ts                # TypeScript 接口定义
│   ├── components/
│   │   ├── Sidebar.vue         # 左侧导航栏（9 项）
│   │   ├── CommandPalette.vue  # 命令面板（Cmd+K）
│   │   ├── ClipboardPopup.vue  # 剪贴板快窗（Shift+Cmd+C）
│   │   └── pages/
│   │       ├── DashboardPage.vue     # 仪表盘
│   │       ├── TasksPage.vue         # 任务管理
│   │       ├── NotesPage.vue         # 笔记/备忘
│   │       ├── ClipboardPage.vue     # 剪贴板历史
│   │       ├── PomodoroPage.vue      # 番茄钟
│   │       ├── SchedulePage.vue      # 日程管理
│   │       ├── MysqlPage.vue         # MySQL 工具
│   │       ├── AlarmPage.vue         # 闹钟
│   │       ├── AiPage.vue            # AI 助手对话
│   │       └── SettingsPage.vue      # 设置（语言）
```

### 路由策略
采用 **Vue Router + hash history** 的 SPA 模式。主布局通过 `<router-view>` 渲染页面，独立剪贴板窗口使用 `/#/clipboard-popup`。Hash 路由避免 Tauri 静态资源环境直接访问子路径时出现 404。

### 全局快捷键
| 快捷键 | 功能 |
|--------|------|
| `Cmd+K` / `Ctrl+K` | 打开/关闭命令面板 |
| `Shift+Cmd+C` / `Shift+Ctrl+C` | 打开/关闭剪贴板快窗 |

---

## 五、Rust 后端结构

```
src-tauri/src/
├── main.rs                     # 入口，调用 nalu_lib::run()
├── lib.rs                      # 注册所有插件和命令，初始化数据库
├── db/
│   ├── mod.rs
│   └── database.rs             # SQLite 连接管理 + 表创建
└── commands/
    ├── mod.rs                  # 模块声明
    ├── database.rs             # SQLite 通用查询
    ├── tasks.rs                # 任务看板 CRUD（含分组/分列/拖拽排序 + 旧版兼容命令）
    ├── notes.rs                # 笔记 CRUD
    ├── clipboard.rs            # 剪贴板历史 CRUD
    ├── pomodoro.rs             # 番茄钟（后台 Tokio 计时器）
    ├── schedule.rs             # 日程 CRUD
    ├── alarm.rs                # 闹钟 CRUD
    ├── mysql.rs                # MySQL 连接/查询/导入导出
    ├── mysql_users.rs          # MySQL 用户管理
    └── ai.rs                   # AI 聊天（多 Provider、推理模式、system prompt 注入、[ACTION] 命令解析）
```

### 关键技术决策

1. **异步命令**：MySQL 和 AI 命令标记为 `async fn`，Tauri 自动在 Tokio 运行时中调度。
2. **番茄钟计时器**：使用 `tauri::async_runtime::spawn`（而非 `tokio::spawn`）确保在 Tauri 运行时上下文中执行。
3. **数据库连接**：全局静态 `LazyLock<Mutex<Option<Connection>>>`，通过 `get_connection()` 获取 MutexGuard。
4. **rusqlite 生命周期**：在有参数过滤的查询中，使用独立的 `prepare` + `query_map` 调用避免 `params!` 宏临时值生命周期问题。
5. **MySQL 导出**：使用 `std::process::Command` 调用 `mysqldump`，不依赖 Rust 库做 dump。
6. **音频通知**：前端使用 Web Audio API 生成 800Hz 正弦波蜂鸣音，不依赖音频文件。
7. **看板数据加载**：`get_board` 一次性返回完整看板数据（分组→列→任务），前端通过 computed 属性过滤搜索，避免多次 IPC。
8. **看板 schema 迁移**：数据库初始化时通过 `migrate_kanban_schema` 幂等迁移：检测旧 tasks 表缺少 progress/column_id/position 列时自动添加，兼容已有数据。
9. **任务拖拽**：前端使用 pointer 事件（非 HTML5 DragEvent），同时兼容触控板和鼠标拖拽，移动端 WebView 也能触发。移动操作在 SQLite 事务中完成（position 重排 + 去空隙）。
10. **撤销删除**：`delete_task_with_snapshot` / `restore_task` 和 `delete_column` / `restore_column` 通过快照模式实现 undo，前端用 5 秒 toast 提供撤销入口。

---

## 六、已知问题与限制

### 已修复的问题
1. ~~rusqlite `params!` 宏生命周期错误~~ → 拆分 match arms
2. ~~旧前端响应性和路由状态问题~~ → 迁移为 Vue Router + Pinia SPA
3. ~~`tokio::spawn` 在同步命令中无运行时~~ → 改用 `tauri::async_runtime::spawn`
4. ~~lucide `Loader` 图标不存在~~ → 改用 `LoaderCircle`

### 当前限制
1. 剪贴板文件支持仅限路径存储，不支持文件内容深拷贝
2. 图片复制回剪贴板时，仅复制 data URL 文本（非原生图片）
3. MySQL 导入依赖整个 SQL 文件加载到内存，大文件可能受限
4. 闹钟和日程的通知检查仍需持续关注多窗口和隐藏窗口生命周期，后台定时逻辑应优先放在 Rust 端

### 构建警告（非阻塞）
- Vue 模板和 TypeScript 类型检查以 `pnpm check` / `vue-tsc --noEmit` 为准
- UI 可访问性问题以后续 Playwright 和组件测试补充覆盖

---

## 七、开发工作流

### 环境
- **Node.js**: 22.22.0（通过 nvm）
- **Rust**: 1.96.0（Edition 2024）
- **pnpm**: 10.33.0
- **平台**: macOS (aarch64-apple-darwin)

### 启动开发
```bash
source ~/.nvm/nvm.sh && nvm use 22
cd /Volumes/NALOMU_MAC/web/nalomu-uni-platform
pnpm tauri dev
```

### 项目路径
`/Volumes/NALOMU_MAC/web/nalomu-uni-platform`（外置磁盘 NALOMU_MAC）

### 数据路径
`~/Library/Application Support/com.nalomu.nalu/nalu.db`

### Git 仓库
已初始化，首次提交：`990597e`
