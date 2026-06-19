mod commands;
mod db;
mod sync;

#[cfg(desktop)]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::Instant;
#[cfg(desktop)]
use tauri::Emitter;
use tauri::Manager;
#[cfg(desktop)]
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[cfg(target_os = "macos")]
use objc2_foundation::NSObjectProtocol;
#[cfg(target_os = "macos")]
use tauri_nspanel::objc2::{ClassType, Message};
#[cfg(target_os = "macos")]
use tauri_nspanel::{WebviewWindowExt as PanelExt, panel};

/// Stores the bundle ID of the app that was active before the popup was shown.
/// Used by paste_to_active_app to know where to paste.
#[cfg(target_os = "macos")]
static PREVIOUS_APP_ID: Mutex<Option<String>> = Mutex::new(None);

/// Tracks the currently registered clipboard shortcut string.
#[cfg(desktop)]
static CURRENT_SHORTCUT: Mutex<Option<String>> = Mutex::new(None);

/// Marks that the next macOS app reactivation came from showing the clipboard popup.
#[cfg(target_os = "macos")]
static POPUP_ACTIVATION_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Tracks whether the main window was temporarily ordered out while showing the popup.
#[cfg(target_os = "macos")]
static MAIN_ORDERED_OUT_FOR_POPUP: Mutex<bool> = Mutex::new(false);

#[cfg(target_os = "macos")]
panel!(ClipboardPanel {
    config: {
        is_floating_panel: true,
        can_become_key_window: true,
        can_become_main_window: false,
    }
});

/// Save the currently active (non-Nalu) app's bundle ID via osascript.
/// Must be called BEFORE the popup window is shown/focused.
#[cfg(target_os = "macos")]
fn save_previous_app() -> bool {
    let started = std::time::Instant::now();

    let id = get_frontmost_bundle_id_fast();

    match id {
        Some(ref bundle_id) => {
            let is_invalid = bundle_id.is_empty()
                || bundle_id == "missing value"
                || bundle_id.contains("nalomu")
                || bundle_id.contains("nalu")
                || bundle_id.contains("tauri");

            if !is_invalid {
                tracing::info!(
                    "[save_previous_app] saved frontmost: {:?}, cost: {:?}",
                    bundle_id,
                    started.elapsed()
                );
                if let Ok(mut guard) = PREVIOUS_APP_ID.lock() {
                    *guard = Some(bundle_id.clone());
                }
                true
            } else {
                tracing::warn!(
                    "[save_previous_app] ignored self/empty app id: {:?}, clearing",
                    bundle_id
                );
                if let Ok(mut guard) = PREVIOUS_APP_ID.lock() {
                    *guard = None;
                }
                false
            }
        }
        None => {
            tracing::warn!("[save_previous_app] could not determine frontmost app");
            if let Ok(mut guard) = PREVIOUS_APP_ID.lock() {
                *guard = None;
            }
            false
        }
    }
}

#[cfg(target_os = "macos")]
fn get_frontmost_bundle_id_fast() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    let workspace = NSWorkspace::sharedWorkspace();
    let front_app = workspace.frontmostApplication()?;
    let bundle_id = front_app.bundleIdentifier()?;
    Some(bundle_id.to_string())
}

#[cfg(target_os = "macos")]
fn with_main_ns_window(app: &tauri::AppHandle, f: impl FnOnce(&objc2_app_kit::NSWindow)) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };

    match main.ns_window() {
        Ok(ns_window) => {
            let ns_window = ns_window.cast::<objc2_app_kit::NSWindow>();
            unsafe {
                f(&*ns_window);
            }
        }
        Err(e) => {
            tracing::warn!("[main_window] failed to get NSWindow: {e}");
        }
    }
}

#[cfg(target_os = "macos")]
fn order_out_main_window_for_popup(app: &tauri::AppHandle) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };

    if !main.is_visible().unwrap_or(false) {
        return;
    }

    with_main_ns_window(app, |window| {
        window.orderOut(None);
    });

    if let Ok(mut guard) = MAIN_ORDERED_OUT_FOR_POPUP.lock() {
        *guard = true;
    }
    tracing::info!("[toggle_popup] temporarily ordered out main window");
}

#[cfg(target_os = "macos")]
fn restore_main_window_after_popup(app: &tauri::AppHandle) {
    let should_restore = MAIN_ORDERED_OUT_FOR_POPUP
        .lock()
        .map(|mut guard| {
            let value = *guard;
            *guard = false;
            value
        })
        .unwrap_or(false);

    if !should_restore {
        return;
    }

    with_main_ns_window(app, |window| {
        window.orderFront(None);
        window.orderBack(None);
    });
    tracing::info!("[toggle_popup] restored main window after popup");
}

#[cfg(target_os = "macos")]
fn mark_popup_activation() {
    if let Ok(mut guard) = POPUP_ACTIVATION_AT.lock() {
        *guard = Some(Instant::now());
    }
}

#[cfg(target_os = "macos")]
fn should_suppress_reopen_for_popup() -> bool {
    POPUP_ACTIVATION_AT
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
        .is_some_and(|started| started.elapsed() < std::time::Duration::from_secs(2))
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Nalu.", name)
}

/// Toggle the clipboard popup window from the frontend (debug button).
#[tauri::command]
fn toggle_clipboard_popup(app: tauri::AppHandle) -> Result<(), String> {
    toggle_popup_impl(&app);
    Ok(())
}

#[tauri::command]
fn check_path_exists(path: String) -> Result<bool, String> {
    Ok(std::path::Path::new(&path).exists())
}

#[derive(serde::Serialize)]
struct CopiedSound {
    path: String,
    name: String,
}

#[tauri::command]
fn copy_custom_sound(app: tauri::AppHandle, path: String) -> Result<CopiedSound, String> {
    let source = std::path::Path::new(&path);
    if !source.is_file() {
        return Err("selected sound file does not exist".to_string());
    }

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| "selected sound file has no extension".to_string())?;

    let allowed = ["mp3", "wav", "ogg", "m4a", "aac", "flac"];
    if !allowed.contains(&extension.as_str()) {
        return Err(format!("unsupported sound file type: {}", extension));
    }

    let display_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("custom sound")
        .to_string();

    let sounds_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("sounds");
    std::fs::create_dir_all(&sounds_dir).map_err(|e| e.to_string())?;

    let target = sounds_dir.join(source.file_name().ok_or("invalid sound filename")?);
    // Remove existing file to handle overwrite
    if target.exists() {
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    std::fs::copy(source, &target).map_err(|e| e.to_string())?;

    Ok(CopiedSound {
        path: target.to_string_lossy().to_string(),
        name: display_name,
    })
}

/// Dismiss the popup and switch back to the previously active app.
#[cfg(target_os = "macos")]
#[tauri::command]
fn activate_previous_app(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(popup) = app.get_webview_window("clipboard-popup") {
        let _ = popup.hide();
    }
    restore_main_window_after_popup(&app);
    activate_previous_app_native();
    Ok(())
}

#[cfg(target_os = "macos")]
fn activate_previous_app_native() {
    let target_id = PREVIOUS_APP_ID
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|id| !id.is_empty());
    if let Some(id) = target_id {
        let _ = activate_app_by_bundle_id(&id);
        tracing::info!("[toggle_popup] re-activated previous app: {}", id);
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn activate_previous_app(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(popup) = app.get_webview_window("clipboard-popup") {
        let _ = popup.hide();
    }
    Ok(())
}

/// Single IPC call: write to clipboard + activate previous app + Cmd+V.
#[cfg(target_os = "macos")]
#[tauri::command]
fn copy_and_paste(
    app: tauri::AppHandle,
    content: String,
    content_type: String,
) -> Result<(), String> {
    commands::clipboard::write_clipboard_entry_to_system(content, content_type)?;
    paste_to_active_app(app)
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn copy_and_paste(
    app: tauri::AppHandle,
    content: String,
    content_type: String,
) -> Result<(), String> {
    commands::clipboard::write_clipboard_entry_to_system(content, content_type)?;
    paste_to_active_app(app)
}

/// Activate the previously saved app and simulate Cmd+V to paste.
/// Uses CGEvent for keystroke simulation (uses the app's own Accessibility permission,
/// unlike osascript which needs its own separate permission).
#[cfg(target_os = "macos")]
#[tauri::command]
fn paste_to_active_app(app: tauri::AppHandle) -> Result<(), String> {
    // Without Accessibility permission, CGEvent keystrokes are silently
    // dropped by the WindowServer. Detect this up front and surface a
    // recognizable error code so the UI can prompt the user instead of
    // appearing to do nothing.
    if !check_accessibility_permission() {
        if let Some(popup) = app.get_webview_window("clipboard-popup") {
            let _ = popup.hide();
        }
        restore_main_window_after_popup(&app);
        activate_previous_app_native();
        return Err("ACCESSIBILITY_PERMISSION_DENIED".to_string());
    }

    let target_id = PREVIOUS_APP_ID
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|id| !id.is_empty());

    // Hide the popup first
    if let Some(popup) = app.get_webview_window("clipboard-popup") {
        let _ = popup.hide();
    }
    restore_main_window_after_popup(&app);

    match target_id {
        Some(id) => {
            tracing::info!("[paste] target app: {:?}", id);
            activate_app_by_bundle_id(&id)?;
            // CGEvent is posted to WindowServer which routes to frontmost app.
            // No need to wait — by the time the event is processed, the app is active.
            send_cmd_v_by_cgevent()?;
            tracing::info!("[paste] Cmd+V sent to: {} via CGEvent", id);
        }
        None => {
            // Popup was opened from Nalu itself — focus the main window and paste into it
            tracing::info!("[paste] no external app, pasting to main window");
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.show();
                let _ = main.set_focus();
            }
            // Small delay for window focus to settle before sending keystroke
            std::thread::sleep(std::time::Duration::from_millis(100));
            send_cmd_v_by_cgevent()?;
            tracing::info!("[paste] Cmd+V sent to main window via CGEvent");
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn activate_app_by_bundle_id(bundle_id: &str) -> Result<(), String> {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;

    let ns_id = NSString::from_str(bundle_id);
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&ns_id);

    if apps.count() == 0 {
        return Err(format!("no running app with bundle id: {}", bundle_id));
    }

    let app = apps.objectAtIndex(0);
    #[allow(deprecated)]
    app.activateWithOptions(
        objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    );

    Ok(())
}

#[cfg(target_os = "macos")]
#[cfg(target_os = "macos")]
fn send_cmd_v_by_cgevent() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEvent source".to_string())?;

    let cmd_flag = CGEventFlags::CGEventFlagCommand;

    // macOS virtual keycode: V = 9
    let key_down = CGEvent::new_keyboard_event(source.clone(), 9, true)
        .map_err(|_| "Failed to create key down event".to_string())?;
    key_down.set_flags(cmd_flag);
    key_down.post(CGEventTapLocation::HID);

    std::thread::sleep(std::time::Duration::from_millis(20));

    let key_up = CGEvent::new_keyboard_event(source, 9, false)
        .map_err(|_| "Failed to create key up event".to_string())?;
    key_up.set_flags(cmd_flag);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn paste_to_active_app(_app: tauri::AppHandle) -> Result<(), String> {
    Err("paste_to_active_app is only supported on macOS".to_string())
}

// FFI to the Accessibility API. AXIsProcessTrusted() reports whether this
// process is allowed to use the Accessibility API (i.e. inject CGEvent
// keystrokes). The permission is granted by the user in System Settings and
// tracked by macOS per code-signature, so it can silently lapse after a
// rebuild — leaving CGEvent.post() to no-op without any error.
#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Returns true if the app currently has Accessibility permission.
#[cfg(target_os = "macos")]
#[tauri::command]
fn check_accessibility_permission() -> bool {
    unsafe { AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn check_accessibility_permission() -> bool {
    true
}

/// Open System Settings directly at the Accessibility privacy pane.
#[cfg(target_os = "macos")]
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to open settings: {e}"))
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    Ok(())
}

/// Register the clipboard popup shortcut. Called by frontend on startup/settings change.
#[tauri::command]
fn register_clipboard_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    #[cfg(not(desktop))]
    {
        let _ = app;
        let _ = shortcut;
        return Err("global shortcut is only supported on desktop".to_string());
    }

    #[cfg(desktop)]
    {
        // Unregister previous shortcut if any
        if let Some(old) = CURRENT_SHORTCUT.lock().ok().and_then(|g| g.clone())
            && let Ok(old_parsed) = parse_shortcut(&old)
        {
            let _ = app.global_shortcut().unregister(old_parsed);
        }

        let parsed = parse_shortcut(&shortcut)?;

        // Block dangerous system shortcuts
        let lower = shortcut.to_lowercase();
        let dangerous = [
            "cmd+q",
            "cmd+w",
            "cmd+h",
            "cmd+m",
            "cmd+tab",
            "cmdorctrl+q",
            "cmdorctrl+w",
            "cmdorctrl+h",
            "cmdorctrl+m",
            "meta+q",
            "meta+w",
            "meta+h",
            "meta+m",
            "super+q",
        ];
        if dangerous
            .iter()
            .any(|d| lower.replace(' ', "") == d.replace(' ', ""))
        {
            return Err(format!(
                "shortcut '{}' conflicts with system shortcuts",
                shortcut
            ));
        }
        app.global_shortcut()
            .on_shortcut(parsed, move |app, _shortcut, event| {
                if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                    toggle_popup_window(app);
                }
            })
            .map_err(|e| format!("failed to register shortcut: {e}"))?;

        if let Ok(mut g) = CURRENT_SHORTCUT.lock() {
            *g = Some(shortcut.clone());
        }
        tracing::info!("[register_clipboard_shortcut] registered: {}", shortcut);
        Ok(())
    }
}

/// Unregister the clipboard popup shortcut.
#[tauri::command]
fn unregister_clipboard_shortcut(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(not(desktop))]
    {
        let _ = app;
        return Ok(());
    }

    #[cfg(desktop)]
    {
        if let Some(old) = CURRENT_SHORTCUT.lock().ok().and_then(|g| g.clone()) {
            if let Ok(parsed) = parse_shortcut(&old) {
                app.global_shortcut()
                    .unregister(parsed)
                    .map_err(|e| format!("failed to unregister: {e}"))?;
            }
            if let Ok(mut g) = CURRENT_SHORTCUT.lock() {
                *g = None;
            }
        }
        tracing::info!("[unregister_clipboard_shortcut] done");
        Ok(())
    }
}

#[cfg(desktop)]
fn parse_shortcut(s: &str) -> Result<tauri_plugin_global_shortcut::Shortcut, String> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut mods = Modifiers::empty();
    let mut key_code: Option<Code> = None;

    for part in &parts {
        match part.to_lowercase().as_str() {
            "cmd" | "command" | "super" | "meta" | "cmdorctrl" => mods |= Modifiers::META,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" | "option" => mods |= Modifiers::ALT,
            key => {
                key_code = Some(match key {
                    "a" => Code::KeyA,
                    "b" => Code::KeyB,
                    "c" => Code::KeyC,
                    "d" => Code::KeyD,
                    "e" => Code::KeyE,
                    "f" => Code::KeyF,
                    "g" => Code::KeyG,
                    "h" => Code::KeyH,
                    "i" => Code::KeyI,
                    "j" => Code::KeyJ,
                    "k" => Code::KeyK,
                    "l" => Code::KeyL,
                    "m" => Code::KeyM,
                    "n" => Code::KeyN,
                    "o" => Code::KeyO,
                    "p" => Code::KeyP,
                    "q" => Code::KeyQ,
                    "r" => Code::KeyR,
                    "s" => Code::KeyS,
                    "t" => Code::KeyT,
                    "u" => Code::KeyU,
                    "v" => Code::KeyV,
                    "w" => Code::KeyW,
                    "x" => Code::KeyX,
                    "y" => Code::KeyY,
                    "z" => Code::KeyZ,
                    "0" => Code::Digit0,
                    "1" => Code::Digit1,
                    "2" => Code::Digit2,
                    "3" => Code::Digit3,
                    "4" => Code::Digit4,
                    "5" => Code::Digit5,
                    "6" => Code::Digit6,
                    "7" => Code::Digit7,
                    "8" => Code::Digit8,
                    "9" => Code::Digit9,
                    "space" => Code::Space,
                    "tab" => Code::Tab,
                    "enter" | "return" => Code::Enter,
                    "backspace" => Code::Backspace,
                    "delete" => Code::Delete,
                    "escape" | "esc" => Code::Escape,
                    "up" => Code::ArrowUp,
                    "down" => Code::ArrowDown,
                    "left" => Code::ArrowLeft,
                    "right" => Code::ArrowRight,
                    "f1" => Code::F1,
                    "f2" => Code::F2,
                    "f3" => Code::F3,
                    "f4" => Code::F4,
                    "f5" => Code::F5,
                    "f6" => Code::F6,
                    "f7" => Code::F7,
                    "f8" => Code::F8,
                    "f9" => Code::F9,
                    "f10" => Code::F10,
                    "f11" => Code::F11,
                    "f12" => Code::F12,
                    _ => return Err(format!("unknown key: {}", part)),
                });
            }
        }
    }

    let code = key_code.ok_or_else(|| "no key specified in shortcut".to_string())?;
    Ok(Shortcut::new(
        if mods.is_empty() { None } else { Some(mods) },
        code,
    ))
}

/// Toggle popup from the global shortcut handler.
#[cfg(desktop)]
fn toggle_popup_window(app: &tauri::AppHandle) {
    toggle_popup_impl(app);
}

/// Shared implementation for showing/hiding the clipboard popup panel.
/// Uses NSPanel's order_front + make_key which makes the panel key window
/// WITHOUT activating the app — focus stays in the original app.
fn toggle_popup_impl(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        #[allow(unused_imports)]
        use tauri_nspanel::ManagerExt;
        #[allow(unused_imports)]
        use tauri_nspanel::Panel as _;
        if let Ok(panel) = app.get_webview_panel("clipboard-popup") {
            if panel.is_visible() {
                panel.hide();
                restore_main_window_after_popup(app);
                activate_previous_app_native();
                tracing::info!("[toggle_popup] hidden via panel API");
            } else {
                let opened_from_external_app = save_previous_app();
                if let Some(window) = app.get_webview_window("clipboard-popup") {
                    if opened_from_external_app {
                        order_out_main_window_for_popup(app);
                    }
                    let _ = window.center();
                    panel.order_front_regardless();
                    panel.make_key_window();
                    // Activate our app so WKWebView receives keyboard events.
                    // If main is already open in the background, keep it out
                    // of AppKit's activation ordering while the popup is shown.
                    {
                        use objc2::MainThreadMarker;
                        use objc2_app_kit::NSApplication;
                        let mtm = MainThreadMarker::new().unwrap();
                        let ns_app = NSApplication::sharedApplication(mtm);
                        mark_popup_activation();
                        ns_app.preventWindowOrdering();
                        #[allow(deprecated)]
                        ns_app.activateIgnoringOtherApps(true);
                    }
                    panel.order_front_regardless();
                    panel.make_key_window();
                }
                let _ = app.emit_to("clipboard-popup", "panel-shown", ());
                tracing::info!("[toggle_popup] shown + activated for keyboard");
            }
        } else {
            // Fallback: panel not registered, use window API
            if let Some(window) = app.get_webview_window("clipboard-popup") {
                let visible = window.is_visible().unwrap_or(false);
                if visible {
                    let _ = window.hide();
                } else {
                    save_previous_app();
                    let _ = window.center();
                    let _ = window.show();
                }
            }
        }
    }
    #[cfg(all(desktop, not(target_os = "macos")))]
    {
        if let Some(window) = app.get_webview_window("clipboard-popup") {
            let visible = window.is_visible().unwrap_or(false);
            if visible {
                let _ = window.hide();
            } else {
                let _ = window.center();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
    }
}

/// Show and activate the main window (called from tray icon).
#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
        let _ = main.unminimize();
        tracing::info!("[show_main_window] main window shown and focused");
    } else {
        tracing::error!("[show_main_window] main window not found!");
    }
}

/// Rebuild the tray menu with translated labels from the frontend.
#[tauri::command]
fn update_tray_menu(
    app: tauri::AppHandle,
    labels: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    #[cfg(not(desktop))]
    {
        let _ = app;
        let _ = labels;
        return Ok(());
    }

    #[cfg(desktop)]
    {
        let label = |key: &str, fallback: &str| -> String {
            labels
                .get(key)
                .cloned()
                .unwrap_or_else(|| fallback.to_string())
        };

        let menu = tauri::menu::MenuBuilder::new(&app)
            .text("show_main", label("open", "Open Nalu"))
            .separator()
            .text("nav_dashboard", label("dashboard", "Dashboard"))
            .text("nav_tasks", label("tasks", "Tasks"))
            .text("nav_notes", label("notes", "Notes"))
            .text("nav_clipboard", label("clipboard", "Clipboard"))
            .text("nav_pomodoro", label("pomodoro", "Pomodoro"))
            .text("nav_schedule", label("schedule", "Schedule"))
            .text("nav_alarm", label("alarm", "Alarm"))
            .text("nav_ai", label("ai", "AI Assistant"))
            .text("nav_mysql", label("mysql", "MySQL"))
            .separator()
            .text("toggle_popup", label("clipboardPopup", "Clipboard Popup"))
            .text("nav_settings", label("settings", "Settings"))
            .separator()
            .text("quit_app", label("quit", "Quit"))
            .build()
            .map_err(|e| format!("build tray menu failed: {e}"))?;

        if let Some(tray) = app.tray_by_id("main-tray") {
            tray.set_menu(Some(menu))
                .map_err(|e| format!("set tray menu failed: {e}"))?;
            tracing::info!(
                "[update_tray_menu] tray menu updated with {} labels",
                labels.len()
            );
        } else {
            return Err("tray icon 'main-tray' not found".to_string());
        }

        Ok(())
    }
}

/// Show main window and emit a navigation event the frontend listens for.
#[cfg(desktop)]
fn show_and_navigate(app: &tauri::AppHandle, route: &str) {
    show_main_window(app);
    if let Err(e) = app.emit("tray-navigate", route.to_string()) {
        tracing::warn!("[show_and_navigate] emit tray-navigate failed: {:?}", e);
    } else {
        tracing::info!("[show_and_navigate] navigated to: {}", route);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        // Core plugins
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init());

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec![]),
            ))
            .plugin(tauri_nspanel::init());
    }

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    builder
        // All commands
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::database::db_query,
            commands::database::db_execute,
            commands::tasks::get_tasks,
            commands::tasks::add_task,
            commands::tasks::toggle_task,
            commands::tasks::update_task,
            commands::tasks::delete_task,
            commands::tasks::get_board,
            commands::tasks::create_task_group,
            commands::tasks::delete_task_group,
            commands::tasks::copy_task_group,
            commands::tasks::rename_task_group,
            commands::tasks::reorder_task_groups,
            commands::tasks::add_task_to_group,
            commands::tasks::add_task_to_column,
            commands::tasks::update_task_content,
            commands::tasks::update_task_progress,
            commands::tasks::delete_task_with_snapshot,
            commands::tasks::restore_task,
            commands::tasks::move_task,
            commands::tasks::create_column_by_drag,
            commands::tasks::rename_column,
            commands::tasks::reorder_columns,
            commands::tasks::delete_column,
            commands::tasks::restore_column,
            commands::notes::get_notes,
            commands::notes::add_note,
            commands::notes::update_note,
            commands::notes::delete_note,
            commands::pomodoro::pomodoro_get_state,
            commands::pomodoro::pomodoro_start,
            commands::pomodoro::pomodoro_pause,
            commands::pomodoro::pomodoro_reset,
            commands::pomodoro::pomodoro_skip,
            commands::pomodoro::pomodoro_set_duration,
            commands::pomodoro::pomodoro_reset_rounds,
            commands::schedule::get_schedules,
            commands::schedule::add_schedule,
            commands::schedule::toggle_schedule,
            commands::schedule::delete_schedule,
            commands::clipboard::get_clipboard_history,
            commands::clipboard::add_clipboard_entry,
            commands::clipboard::delete_clipboard_entry,
            commands::clipboard::clear_clipboard_history,
            commands::clipboard::get_clipboard_entry,
            commands::clipboard::write_clipboard_entry_to_system,
            commands::clipboard::save_clipboard_image_data_url,
            commands::clipboard::read_clipboard_file_references,
            commands::clipboard::cleanup_clipboard,
            toggle_clipboard_popup,
            register_clipboard_shortcut,
            unregister_clipboard_shortcut,
            check_path_exists,
            copy_custom_sound,
            activate_previous_app,
            paste_to_active_app,
            copy_and_paste,
            check_accessibility_permission,
            open_accessibility_settings,
            commands::mysql::mysql_test_connection,
            commands::mysql::mysql_query,
            commands::mysql::mysql_execute,
            commands::mysql::mysql_list_databases,
            commands::mysql::mysql_export,
            commands::mysql::mysql_import,
            commands::mysql_users::get_mysql_users,
            commands::mysql_users::add_mysql_user,
            commands::mysql_users::upsert_mysql_user,
            commands::mysql_users::update_mysql_user,
            commands::mysql_users::delete_mysql_user,
            commands::mysql_users::mysql_create_user_on_server,
            commands::mysql_users::mysql_create_database_with_user,
            commands::mysql_users::mysql_delete_database_with_user,
            commands::mysql_users::mysql_update_managed_user_password,
            commands::mysql_users::mysql_list_server_users,
            commands::mysql_users::mysql_drop_server_user,
            commands::ai::ai_chat,
            commands::alarm::get_alarms,
            commands::alarm::add_alarm,
            commands::alarm::skip_next_alarm,
            commands::alarm::update_alarm_sound,
            commands::alarm::toggle_alarm,
            commands::alarm::delete_alarm,
            commands::sync::sync_pair,
            commands::sync::sync_now,
            commands::sync::sync_get_config,
            commands::sync::sync_disconnect,
            update_tray_menu,
        ])
        // Setup
        .setup(|app| {
            tracing::info!("[Setup] Nalu application starting...");
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");
            let db_path = app_dir.join("nalu.db");
            db::init(&db_path).expect("failed to initialize database");
            tracing::info!("[Setup] Database initialized at: {:?}", db_path);

            // --- Clipboard popup window → NSPanel (non-activating) ---
            #[cfg(target_os = "macos")]
            {
                if let Some(popup) = app.get_webview_window("clipboard-popup") {
                    let _ = popup.hide();
                    match popup.to_panel::<ClipboardPanel<tauri::Wry>>() {
                        Ok(_panel_handle) => {
                            tracing::info!(
                                "[Setup] clipboard-popup converted to NSPanel (non-activating)"
                            );
                        }
                        Err(e) => {
                            tracing::error!("[Setup] failed to convert popup to NSPanel: {:?}", e);
                            let _ = popup.set_always_on_top(true);
                            let _ = popup.set_skip_taskbar(true);
                        }
                    }
                }
            }
            #[cfg(all(desktop, not(target_os = "macos")))]
            {
                if let Some(popup) = app.get_webview_window("clipboard-popup") {
                    let _ = popup.set_always_on_top(true);
                    let _ = popup.set_skip_taskbar(true);
                    let _ = popup.hide();
                }
            }

            // --- Main window: close → hide (don't quit) ---
            if let Some(main) = app.get_webview_window("main") {
                let main_clone = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_clone.hide();
                        tracing::info!("[MainWindow] close intercepted → hidden");
                    }
                });
            }

            // --- Tray icon: click to show main window ---
            #[cfg(desktop)]
            {
                let tray_menu = tauri::menu::MenuBuilder::new(app)
                    .text("show_main", "Open Nalu")
                    .separator()
                    .text("nav_dashboard", "Dashboard")
                    .text("nav_tasks", "Tasks")
                    .text("nav_notes", "Notes")
                    .text("nav_clipboard", "Clipboard")
                    .text("nav_pomodoro", "Pomodoro")
                    .text("nav_schedule", "Schedule")
                    .text("nav_alarm", "Alarm")
                    .text("nav_ai", "AI Assistant")
                    .text("nav_mysql", "MySQL")
                    .separator()
                    .text("toggle_popup", "Clipboard Popup")
                    .text("nav_settings", "Settings")
                    .separator()
                    .text("quit_app", "Quit")
                    .build()?;

                let tray_icon = tauri::include_image!("icons/tray-icon.png");
                let _tray = tauri::tray::TrayIconBuilder::with_id("main-tray")
                    .icon(tray_icon)
                    .icon_as_template(true)
                    .menu(&tray_menu)
                    .on_menu_event(move |app, event| match event.id().as_ref() {
                        "show_main" => show_main_window(app),
                        "nav_dashboard" => show_and_navigate(app, "/"),
                        "nav_tasks" => show_and_navigate(app, "/tasks"),
                        "nav_notes" => show_and_navigate(app, "/notes"),
                        "nav_clipboard" => show_and_navigate(app, "/clipboard"),
                        "nav_pomodoro" => show_and_navigate(app, "/pomodoro"),
                        "nav_schedule" => show_and_navigate(app, "/schedule"),
                        "nav_alarm" => show_and_navigate(app, "/alarm"),
                        "nav_ai" => show_and_navigate(app, "/ai"),
                        "nav_mysql" => show_and_navigate(app, "/mysql"),
                        "nav_settings" => show_and_navigate(app, "/settings"),
                        "toggle_popup" => toggle_popup_window(app),
                        "quit_app" => {
                            tracing::info!("[Tray] Quit requested");
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        // Single click or left-click on tray icon → show main window
                        if let tauri::tray::TrayIconEvent::Click { .. } = event {
                            let app = tray.app_handle();
                            show_main_window(app);
                        }
                    })
                    .build(app)?;

                tracing::info!("[Setup] Tray icon with menu configured");

                // --- Global shortcuts ---
                // Shortcut registration is now driven by the frontend via commands.
                // On startup the frontend will call register_clipboard_shortcut if enabled.
                tracing::info!("[Setup] Global shortcut registration deferred to frontend");
            }

            // --- Background alarm checker (runs in Rust, immune to WebView throttling) ---
            commands::alarm::start_alarm_checker(app.handle().clone());
            tracing::info!("[Setup] Alarm checker started (10s interval)");

            tracing::info!("[Setup] ✅ Nalu started successfully");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|app, event| {
            #[cfg(not(desktop))]
            {
                let _ = app;
                let _ = event;
            }

            // Handle macOS dock icon click / reopen event
            #[cfg(desktop)]
            if let tauri::RunEvent::Reopen { .. } = event {
                #[cfg(target_os = "macos")]
                if should_suppress_reopen_for_popup() {
                    tracing::info!("[Reopen] suppressed main window activation from popup");
                    return;
                }

                tracing::info!("[Reopen] Dock icon clicked or app reactivated");
                show_main_window(app);
            }
        });
}
