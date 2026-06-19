# Android 构建

## SDK 路径

当前 Android SDK 放在外置盘：

```text
/Volumes/NALOMU_MAC/Android/sdk
```

Shell 环境变量写在 `~/.zshrc`：

```bash
export ANDROID_HOME="/Volumes/NALOMU_MAC/Android/sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
```

## 初始化

```bash
pnpm tauri android init
```

该命令生成 Android 工程到：

```text
src-tauri/gen/android
```

## 构建 APK

```bash
pnpm tauri android build --debug --apk
```

产物路径：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## 安装到手机

```bash
adb devices
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## 注意事项

- Debug universal APK 会包含多个 ABI 和调试符号，体积会明显偏大。
- 裸 `cargo check --target aarch64-linux-android` 需要显式设置 NDK clang 环境；`pnpm tauri android build` 会自动注入。
- Android 不支持桌面端托盘、全局快捷键、NSPanel 和 macOS 辅助功能粘贴。
