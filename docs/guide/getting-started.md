# 快速开始

## 环境要求

- Node.js 20+
- pnpm 10+
- Rust stable
- Tauri 2 所需系统依赖

Android 构建还需要 Android SDK、NDK、cmdline-tools，见 [Android 构建](../operations/android-build.md)。

## 安装依赖

```bash
pnpm install
```

## 前端开发

```bash
pnpm dev
```

## 桌面应用开发

```bash
pnpm tauri:dev
```

`tauri:dev` 会同时启动 Vite 前端和 Tauri 后端，适合查看完整桌面能力，包括系统托盘、全局快捷键、通知和本地文件能力。

## 验证

```bash
pnpm check
pnpm test
cd src-tauri && cargo test --lib
```

完整 E2E 根据场景运行：

```bash
pnpm test:e2e
```
