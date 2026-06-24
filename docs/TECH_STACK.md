# Nalu 技术选型定版

**项目名称**：Nalu（纳鲁）
**缩写**：NL
**项目代号**：nalux
**日期**：2026-06-06
**状态**：已定版

**选型原则**：最快开发速度优先，可一定程度牺牲性能。

---

## 环境要求（需升级）

| 工具 | 当前版本 | 要求版本 | 操作 |
|------|---------|---------|------|
| Node.js | v16.20.2 | **v20+** | 需升级（项目提供 `.nvmrc`） |
| Rust | 未安装 | **1.88+（最新 stable）** | 需安装（`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh`） |
| pnpm | 未检测 | **v10+** | 需安装（`npm install -g pnpm`） |

---

## 技术栈定版

### 核心框架

| 层级 | 技术 | 版本 | 用途 |
|------|------|------|------|
| 桌面框架 | **Tauri** | **2.11.2** | 桌面应用壳，系统 WebView 渲染，原生 API |
| 后端语言 | **Rust** | **1.88+** (Edition 2024) | Tauri 后端逻辑，插件运行时 |
| 前端框架 | **Vue 3** | **3.5** | UI 组件与响应式系统 |
| 前端路由 | **Vue Router** | **4.6** | Hash SPA 路由和 `<router-view>` |
| 全局状态 | **Pinia** | **3.0** | 跨页面应用状态 |
| 构建工具 | **Vite** | **6.4** | 前端开发服务器 + 构建 |
| 类型系统 | **TypeScript** | **5.6** | 前端类型安全，减少运行时 bug |
| CSS 方案 | **Tailwind CSS** | **4.3.0** | 原子化 CSS，极速写样式 |
| 图标库 | **lucide-vue-next** | **0.468** | Vue 图标组件 |

### Rust 后端依赖

| 库 | 版本 | 用途 |
|---|------|------|
| `tauri` | 2.11.2 | 核心框架 |
| `serde` | 1.0.228 | 序列化/反序列化 |
| `serde_json` | 1.0.150 | JSON 处理 |
| `tokio` | 1.52.3 (features: full) | 异步运行时 |
| `rusqlite` | 0.40.1 (features: bundled) | SQLite 嵌入式数据库，bundled 免装系统库 |
| `mysql_async` | 0.37.0 | MySQL 异步客户端 |
| `automerge` | 0.10.0 | CRDT 数据同步 |
| `axum` | 0.8.9 | 同步中继服务（V2 阶段） |
| `chrono` | 最新 | 日期时间处理 |
| `reqwest` | 最新 (features: json) | HTTP 客户端（AI API 调用） |
| `tracing` | 最新 | 结构化日志 |
| `uuid` | 最新 (features: v4) | UUID 生成 |

### Tauri 插件（官方）

| 插件 | npm 版本 | Rust 版本 | 用途 |
|------|---------|----------|------|
| `@tauri-apps/plugin-notification` | 2.3.3 | 2.3.3 | 系统通知（番茄钟、闹钟提醒） |
| `@tauri-apps/plugin-shell` | 2.3.5 | 2.3.5 | 执行系统命令（调用 mysqldump） |
| `@tauri-apps/plugin-clipboard-manager` | 2.3.2 | 2.3.2 | 剪贴板读写 |
| `@tauri-apps/plugin-global-shortcut` | 2.3.2 | 2.3.2 | 全局热键（命令面板唤起） |
| `@tauri-apps/plugin-store` | 2.4.3 | 2.4.3 | 轻量 KV 存储（插件配置） |
| `@tauri-apps/plugin-dialog` | 2.7.1 | 2.7.1 | 文件/目录选择对话框 |
| `@tauri-apps/plugin-fs` | 最新 | 最新 | 文件系统操作 |
| `@tauri-apps/plugin-process` | 最新 | 最新 | 进程管理（退出、重启） |

### 前端 npm 依赖

| 包 | 版本 | 用途 |
|---|------|------|
| `@tauri-apps/api` | 2.11.0 | Tauri 前端 API |
| `@tauri-apps/cli` | 2.11.2 | Tauri CLI（开发/构建） |
| `vue` | 3.5 | 前端框架 |
| `vue-router` | 4.6 | 前端路由 |
| `pinia` | 3.0 | 全局状态 |
| `vite` | 6.4 | 构建工具 |
| `typescript` | 5.6 | 类型系统 |
| `tailwindcss` | 4.3.0 | CSS 方案 |
| `@milkdown/core` | 7.21.2 | Markdown 编辑器核心 |
| `@milkdown/preset-commonmark` | 最新 | Markdown 基础语法 |
| `@milkdown/plugin-listener` | 最新 | 编辑器事件监听 |
| `lucide-vue-next` | 0.468 | 图标库 |

### 开发工具链

| 工具 | 版本 | 用途 |
|------|------|------|
| `pnpm` | v10+ | 包管理（快、磁盘友好） |
| `prettier` | 最新 | 代码格式化 |
| `eslint` | 最新 | 代码检查 |
| `vitest` | 最新 | 单元测试（Vite 原生） |
| `vue-tsc` | 2.2 | Vue 类型检查 |

---

## 项目结构

```
nalomu-uni-platform/
├── docs/                      # 项目文档
│   └── TECH_STACK.md          # 本文件
├── src/                       # 前端源码（Vue）
│   ├── lib/
│   │   ├── components/        # 通用 UI 组件
│   │   ├── stores/            # Pinia stores（全局状态）
│   │   ├── composables/        # Vue 组合式逻辑
│   │   ├── i18n/              # 多语言文本
│   │   └── types.ts           # 前端共享类型
│   ├── router.ts              # Vue Router hash 路由
│   ├── App.vue                # 根组件
│   └── main.ts                # 入口
├── src-tauri/                 # Rust 后端
│   ├── src/
│   │   ├── main.rs            # 入口
│   │   ├── commands/          # Tauri commands（前端可调用）
│   │   │   ├── mod.rs
│   │   │   ├── clipboard.rs   # 剪贴板相关
│   │   │   ├── database.rs    # SQLite + MySQL
│   │   │   ├── tasks.rs       # 任务看板（分组/分列/拖拽）
│   │   │   ├── notes.rs       # 笔记/备忘
│   │   │   ├── schedule.rs    # 日程/闹钟
│   │   │   ├── pomodoro.rs    # 番茄钟
│   │   │   └── ai.rs          # AI 聊天（多 Provider/推理/[ACTION]）
│   │   ├── db/                # 数据库层
│   │   │   ├── mod.rs
│   │   │   └── database.rs    # SQLite 初始化和访问
│   │   └── sync/              # 桌面端同步客户端
│   │       ├── mod.rs
│   │       ├── client.rs
│   │       └── changelog.rs
│   ├── Cargo.toml             # Rust 依赖
│   ├── tauri.conf.json        # Tauri 配置
│   └── capabilities/          # 权限配置
├── shared/                    # 共享协议、模型、同步契约（Rust crate）
│   └── src/
├── src-server/                # 私有同步中继服务（Axum）
│   └── src/
├── plugins/                   # 插件开发目录（预留）
├── scripts/                   # 文档截图和项目脚本
├── package.json
├── pnpm-lock.yaml
├── pnpm-workspace.yaml
├── vite.config.js
├── tsconfig.json
├── vitest.config.js
└── README.md
```

移动端正式工程放在 `mobile/android/`，使用 Kotlin / Jetpack Compose / Room 原生实现。
iOS 在 `mobile/ios/` 预留 Swift / SwiftUI 方向。移动端与桌面端共享协议、数据模型、同步规则、
错误码和设计 token，不共享 Vue UI、Tauri IPC 或桌面端 Rust Core。

---

## Cargo.toml 参考

```toml
[package]
name = "nalu"
version = "0.1.0"
edition = "2024"

[dependencies]
tauri = { version = "2.11.2", features = ["tray-icon"] }
tauri-plugin-notification = "2.3.3"
tauri-plugin-shell = "2.3.5"
tauri-plugin-clipboard-manager = "2.3.2"
tauri-plugin-global-shortcut = "2.3.2"
tauri-plugin-store = "2.4.3"
tauri-plugin-dialog = "2.7.1"
tauri-plugin-fs = "2"
tauri-plugin-process = "2"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
tokio = { version = "1.52.3", features = ["full"] }
rusqlite = { version = "0.40.1", features = ["bundled"] }
mysql_async = "0.37.0"
automerge = "0.10.0"
axum = "0.8.9"
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json"] }
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

---

## package.json 参考

```json
{
  "name": "nalu",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "check": "vue-tsc --noEmit",
    "test": "vitest run",
    "test:e2e": "playwright test",
    "docs:dev": "vuepress dev docs",
    "docs:build": "vuepress build docs",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.11.0",
    "@tauri-apps/plugin-autostart": "^2.5.1",
    "@tauri-apps/plugin-notification": "^2.3.3",
    "@tauri-apps/plugin-shell": "^2.3.5",
    "@tauri-apps/plugin-clipboard-manager": "^2.3.2",
    "@tauri-apps/plugin-global-shortcut": "^2.3.2",
    "@tauri-apps/plugin-store": "^2.4.3",
    "@tauri-apps/plugin-dialog": "^2.7.1",
    "@tauri-apps/plugin-fs": "^2",
    "@tauri-apps/plugin-process": "^2",
    "@milkdown/core": "^7.21.2",
    "@milkdown/preset-commonmark": "^7.21.2",
    "@milkdown/plugin-listener": "^7.21.2",
    "lucide-vue-next": "^0.468.0",
    "pinia": "^3.0.3",
    "tailwindcss": "^4.3.0",
    "vue": "^3.5.13",
    "vue-router": "^4.5.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.11.2",
    "@vitejs/plugin-vue": "^5.2.1",
    "@vuepress/bundler-vite": "2.0.0-rc.2",
    "@vuepress/theme-default": "2.0.0-rc.2",
    "@playwright/test": "^1.60.0",
    "vite": "^6.0.3",
    "typescript": "~5.6.2",
    "prettier": "^3",
    "eslint": "^9",
    "vitest": "^4",
    "vue-tsc": "^2.2.0",
    "vuepress": "2.0.0-rc.2"
  }
}
```

---

## 开发阶段规划（快速优先）

### Phase 1：桌面端主应用

目标：继续以 Tauri 2 + Vue 3 + Rust 作为主线，优先把桌面端本地优先能力做扎实。

- [x] Vue 3 + Vue Router + Pinia + Tailwind CSS 4 主前端
- [x] Tauri 2 桌面应用壳和 Rust command 层
- [x] SQLite 本地数据基础
- [x] 系统托盘、全局热键、剪贴板、通知等桌面能力
- [ ] 继续完善任务、日程、笔记、番茄钟、剪贴板和 AI 助手入口

### Phase 2：共享契约中心

目标：把跨端真正需要共享的内容收敛到 `shared/`。

- [x] 任务、笔记、日程等数据模型契约
- [x] 同步消息格式和冲突处理规则
- [x] 错误码基础契约
- [ ] 导入导出格式、加密/压缩规则
- [ ] 设计 token，供 Vue、Compose、SwiftUI 各端映射

### Phase 3：私有同步服务

目标：让桌面端和后续移动端通过同一套同步协议交换变更。

- [x] `src-server/` HTTP changelog 同步服务基础
- [ ] WebSocket 同步通道（future）
- [ ] 设备身份、认证和重放策略
- [ ] 离线变更补偿和日志清理

### Phase 4：Android 原生端

目标：新开 Kotlin / Jetpack Compose 原生端，只接同步协议和移动端核心功能。

- [x] `mobile/android/` 工程
- [x] Room / SQLite 本地副本
- [x] 同步协议客户端
- [x] 通知、分享入口等 Android 原生能力基础
- [ ] 快捷入口和精确日程提醒

### Phase 5：iOS 轻端

目标：在桌面端和 Android 同步跑通后，再启动 SwiftUI 轻端。

- [ ] 任务、笔记、日程查看和手动添加
- [ ] Share Extension
- [ ] Shortcuts
- [ ] 同步查看收藏内容

---

## 本仓库常用命令

```bash
pnpm dev
pnpm tauri:dev
pnpm check
pnpm test
pnpm tauri:build
```
