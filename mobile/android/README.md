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
one-time WorkManager or manual sync pushes/pulls HTTP changelog entries. The app
also registers a 15-minute connected-network periodic sync fallback on startup.

Schedule items are task rows with `scheduled_start_at`, `scheduled_end_at`, and
`reminder_minutes`. The legacy `schedules` table remains only for compatibility
with older data and is not used for new Android writes. Legacy rows are shown in
a read-only section on the schedule screen.

The app uses the contracts in `../../shared/contracts` and does not share Vue UI,
Tauri IPC, or Desktop Rust Core.

The native app currently uses `compileSdk = 35` and `targetSdk = 35` because the
local Android 36 platform fails AGP's JDK image transform on this machine. Raise
both values after the Android 36 SDK/JDK combination is stable locally.

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
