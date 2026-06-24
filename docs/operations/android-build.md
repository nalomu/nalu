# Android 构建

## 正式 Android 工程

正式 Android 端使用 Kotlin / Jetpack Compose 原生工程：

```text
mobile/android
```

当前原生工程暂用 `compileSdk = 35` / `targetSdk = 35`。本机 Android 36 platform 在 AGP
JDK image transform 阶段不稳定，等 SDK/JDK 组合稳定后再统一升到 36。

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

## 注意事项

- Debug universal APK 会包含多个 ABI 和调试符号，体积会明显偏大。
- Android 不支持桌面端托盘、全局快捷键、NSPanel 和 macOS 辅助功能粘贴。
