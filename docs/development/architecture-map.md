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

## 剪贴板监听

`commands/clipboard_monitor.rs` 在独立 Rust 线程中每 200ms 检查剪贴板。macOS 用
`NSPasteboard.changeCount` 判断是否有新复制，按文件 URL、位图、文本的顺序读取；读取前后
校验 changeCount，避免把不同复制操作的数据混合保存。关闭开关与入库互斥，重新开启时
重新读取当前内容。后台监听不依赖 WebView 定时器或窗口可见性。

图片比较使用完整像素内容的 BLAKE3 指纹，落盘后的图片以完整文件字节的 BLAKE3 哈希去重，
并校验字节一致。旧 FNV-1a 哈希在命中旧历史时按需升级，保留原记录。前端通过
`clipboard-history-changed` 事件刷新历史。

开启监听默认引导 macOS 完全磁盘访问。已经开启监听的用户在升级后补一次引导，
通过 `nalu-clipboard-full-disk-guide-v1` 记录引导完成，后续启动不重复打开系统设置。
用户主动关闭后重新开启时，若尚未确认权限，则再次引导。

完全磁盘访问必须由用户在系统设置中手动添加并开启 Nalu，没有自动授权接口。
后端只尝试打开当前用户的受保护 TCC 文件来探测访问能力，不读取其中数据；探测结果为
`granted` / `denied` / `unknown`，非 macOS 为 `unsupported`。这是文件访问探测，
并非系统授权状态 API。返回应用或点击重新检查会刷新结果；未授权仍记录可读取内容。
文件历史保存源路径，源文件移动、删除或访问受限后可能无法再次粘贴。

本地打包通过 `bundle.macOS.signingIdentity = "-"`，发布工作流显式设置
`APPLE_SIGNING_IDENTITY = "-"`，统一使用 ad-hoc 签名。重建应用后若系统权限失效，
需要在系统设置里重新授权；应用不能自动替用户授权。
