# Gradle/NDK 缓存迁移

## 当前策略

Android SDK 和 NDK 已经放在外置盘：

```text
/Volumes/NALOMU_MAC/Android/sdk
```

Gradle 用户目录迁移到外置盘：

```text
/Volumes/NALOMU_MAC/Android/gradle-home
```

用户目录下保留符号链接：

```text
~/.gradle -> /Volumes/NALOMU_MAC/Android/gradle-home
```

这样 Gradle wrapper、依赖包、Kotlin 编译缓存和 Android 构建缓存都不会继续堆在系统盘。

## 迁移前检查

```bash
du -sh ~/.gradle ~/.android /Volumes/NALOMU_MAC/Android/sdk /Volumes/NALOMU_MAC/Android/sdk/ndk
```

迁移前需要停止 Gradle/Kotlin daemon：

```bash
src-tauri/gen/android/gradlew --project-dir src-tauri/gen/android --stop
```

## 验证

```bash
ls -ld ~/.gradle
du -sh ~/.gradle /Volumes/NALOMU_MAC/Android/gradle-home
src-tauri/gen/android/gradlew --project-dir src-tauri/gen/android --version
```

## 风险

- 外置盘未挂载时，Gradle 无法使用。
- 首次迁移后构建可能重新生成少量 daemon 文件。
- 如果未来换外置盘路径，需要重新创建 `~/.gradle` 符号链接。
