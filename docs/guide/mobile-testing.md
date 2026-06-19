# 手机端查看和测试

## 浏览器响应式预览

只看布局时，先用 Vite：

```bash
pnpm dev
```

在浏览器 DevTools 中切换手机尺寸，适合快速检查首页、设置页、闹钟页和番茄钟页的响应式布局。

## Android 真机 APK

完整能力测试使用 Tauri Android APK：

```bash
pnpm tauri android build --debug --apk
```

构建产物：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

安装到手机：

```bash
adb devices
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## 手机端重点检查

- 首页快捷操作是否适合窄屏。
- 声音设置是否能找到并调整音量。
- 闹钟和番茄钟页面是否无横向溢出。
- Android 上桌面专用能力是否被隐藏或降级，例如全局快捷键、系统托盘和 NSPanel。
