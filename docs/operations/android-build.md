# Android 构建

## 正式 Android 工程

正式 Android 端使用 Kotlin / Jetpack Compose 原生工程：

```text
mobile/android
```

`src-tauri/gen/android` 是 Tauri Mobile 生成目录，只保留作 legacy/实验调试参考，不作为
Nalu Mobile 的正式开发主线。日常移动端开发、构建和测试都应从 `mobile/android` 进入。

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

## 原生工程构建

```bash
cd mobile/android
./gradlew :app:assembleDebug
```

## 原生工程测试

```bash
cd mobile/android
./gradlew testDebugUnitTest
```

## 安装原生 Debug APK

```bash
adb devices
adb install -r mobile/android/app/build/outputs/apk/debug/app-debug.apk
```

## Tauri Android legacy

以下命令只用于排查旧 Tauri Mobile 生成工程，不用于正式 Android 端开发。

## 初始化 legacy 工程

```bash
pnpm tauri android init
```

该命令生成 Android 工程到：

```text
src-tauri/gen/android
```

## 构建 legacy APK

日常真机调试不需要 release 签名包，优先使用：

```bash
pnpm tauri:dev:android:legacy
```

需要安装包形态时使用 debug APK：

```bash
pnpm tauri android build --debug --apk
```

产物路径：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## 安装 legacy APK 到手机

```bash
adb devices
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## 注意事项

- Debug universal APK 会包含多个 ABI 和调试符号，体积会明显偏大。
- 裸 `cargo check --target aarch64-linux-android` 需要显式设置 NDK clang 环境；`pnpm tauri android build` 会自动注入。
- Android 不支持桌面端托盘、全局快捷键、NSPanel 和 macOS 辅助功能粘贴。
