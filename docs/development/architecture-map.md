# 架构地图

## 前端

核心目录：

```text
src/
  App.vue
  lib/
    components/pages/
    stores/
    utils/
    i18n/
```

前端负责页面、交互状态、声音预览和 Tauri IPC 调用。跨页面设置放在 Pinia store 中。

## Rust 后端

核心目录：

```text
src-tauri/src/
  commands/
  db/
  sync/
  lib.rs
```

Rust 后端负责本地数据库、后台计时器、文件复制、系统能力和移动端平台差异。

## 平台差异

桌面端能力：

- 系统托盘
- 全局快捷键
- NSPanel 剪贴板弹窗
- 开机自启
- macOS 辅助功能粘贴

移动端需要降级或隐藏这些能力。后端使用 `#[cfg(desktop)]`、`#[cfg(target_os = "macos")]` 做条件编译。
