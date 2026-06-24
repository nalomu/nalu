# 手机端查看和测试

## 正式 Android 原生端

Nalu Mobile 的正式 Android 端位于：

```text
mobile/android
```

它是 Kotlin / Jetpack Compose / Room 原生工程，不依赖 Vue UI、Tauri IPC 或 Desktop Rust
Core。移动端只共享协议、数据模型、同步规则、错误码和 design token。

Android 原生日程新建数据写入 `tasks.scheduled_start_at` / `scheduled_end_at` /
`reminder_minutes`，旧 `schedules` 表只做兼容读取，不作为新建日程主路径。
旧 `schedules` 远端数据会显示在日程页只读分区；Android 新增、完成、删除日程只应产生
`tasks` changelog。

本地任务、笔记、日程任务写入后会 enqueue 一次性 WorkManager 同步；App 启动时还会注册
15 分钟联网周期同步作为兜底。手动测试离线写入时，应验证恢复网络后一次性同步和周期同步
都不会把新日程写回 legacy `schedules` 表。

构建和运行单元测试：

```bash
cd mobile/android
./gradlew :app:assembleDebug
./gradlew testDebugUnitTest
```

真机安装：

```bash
adb devices
adb install -r mobile/android/app/build/outputs/apk/debug/app-debug.apk
```

查看 Android 运行日志：

```bash
adb logcat | rg "nalu|tauri|chromium|AndroidRuntime"
```

## 浏览器响应式预览

只看布局时，先用 Vite：

```bash
pnpm dev
```

在浏览器 DevTools 中切换手机尺寸，适合快速检查首页、设置页、闹钟页和番茄钟页的响应式布局。

## 手机端重点检查

- 首页快捷操作是否适合窄屏。
- 声音设置是否能找到并调整音量。
- 闹钟和番茄钟页面是否无横向溢出。
- Android 上桌面专用能力是否被隐藏或降级，例如全局快捷键、系统托盘和 NSPanel。
