# Nalu Mobile Android

This is the official native Android client for Nalu.

Stack:

- Kotlin
- Jetpack Compose + Material 3
- ViewModel + Repository
- Room / SQLite
- DataStore
- Retrofit / OkHttp
- WorkManager
- kotlinx.serialization

The app is a local-first companion client. Room is the single UI data source;
local mutations write both the business table and `sync_changelog`, then
WorkManager or manual sync pushes/pulls HTTP changelog entries.

The app uses the contracts in `../../shared/contracts` and does not share Vue UI,
Tauri IPC, or Desktop Rust Core.

Build:

```bash
cd mobile/android
./gradlew :app:assembleDebug
```

Tests:

```bash
cd mobile/android
./gradlew testDebugUnitTest
```
