//! Desktop clipboard capture lives outside the WebView, including while windows are hidden.
use super::clipboard::{self, ClipboardEntry};
use serde::Serialize;
use tauri::{Emitter, Manager};

#[cfg(desktop)]
use std::sync::{Mutex, Once};
#[cfg(desktop)]
use std::time::Duration;

#[cfg(desktop)]
#[derive(Default)]
struct Control {
    enabled: bool,
    revision: u64,
}
#[cfg(desktop)]
static CONTROL: Mutex<Control> = Mutex::new(Control {
    enabled: false,
    revision: 0,
});
#[cfg(desktop)]
static START: Once = Once::new();

#[tauri::command]
pub async fn set_clipboard_monitoring(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(desktop)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            // Serialize toggles with persistence: after disable returns, no capture can be saved.
            let mut control = CONTROL.lock().map_err(|e| e.to_string())?;
            if control.enabled != enabled {
                control.enabled = enabled;
                control.revision = control.revision.wrapping_add(1);
            }
            drop(control);
            START.call_once(|| {
                std::thread::Builder::new()
                    .name("clipboard-monitor".into())
                    .spawn(move || monitor(app))
                    .expect("start clipboard monitor thread");
            });
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskAccess {
    #[cfg(target_os = "macos")]
    Granted,
    #[cfg(target_os = "macos")]
    Denied,
    #[cfg(target_os = "macos")]
    Unknown,
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}

/// A capability probe, not a public macOS authorization-state API.
/// TCC.db is a protected system database and is unsuitable as a general file-access probe.
#[tauri::command]
pub fn check_full_disk_access(app: tauri::AppHandle) -> DiskAccess {
    #[cfg(target_os = "macos")]
    {
        let Ok(home) = app.path().home_dir() else {
            return DiskAccess::Unknown;
        };
        let mut denied = false;
        for relative in [
            "Library/Safari/History.db",
            "Library/Safari/Bookmarks.plist",
        ] {
            // Open only; never read the contents of other applications' private data.
            match std::fs::File::open(home.join(relative)) {
                Ok(_) => return DiskAccess::Granted,
                Err(e)
                    if e.kind() == std::io::ErrorKind::PermissionDenied
                        || e.raw_os_error() == Some(1) =>
                {
                    denied = true
                }
                Err(_) => {}
            }
        }
        if denied {
            DiskAccess::Denied
        } else {
            DiskAccess::Unknown
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        DiskAccess::Unsupported
    }
}

#[tauri::command]
pub fn open_full_disk_access_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Unable to open Full Disk Access settings".into());
        }
    }
    Ok(())
}

#[cfg(desktop)]
struct Capture {
    content: String,
    content_type: &'static str,
    image_bytes: Option<Vec<u8>>,
    fingerprint: String,
}

#[cfg(desktop)]
fn capture() -> Result<Option<Capture>, String> {
    let refs = clipboard::read_clipboard_file_references_impl()?;
    if !refs.is_empty() {
        // Container URLs can be unreadable while the provider offers a usable bitmap.
        // Keep ordinary files as references; use the bitmap fallback only for one image.
        if refs.len() == 1
            && refs[0].is_image
            && std::fs::File::open(&refs[0].path).is_err()
            && let Ok(mut cb) = arboard::Clipboard::new()
            && let Ok(image) = cb.get_image()
        {
            return bitmap_capture(image).map(Some);
        }
        let kind = if refs.len() == 1 && refs[0].is_image {
            "image_file"
        } else {
            "file"
        };
        let content = refs
            .into_iter()
            .map(|r| r.path)
            .collect::<Vec<_>>()
            .join("\n");
        return Ok(Some(Capture {
            fingerprint: format!("{kind}:{content}"),
            content,
            content_type: kind,
            image_bytes: None,
        }));
    }
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match cb.get_image() {
        Ok(image) => {
            return bitmap_capture(image).map(Some);
        }
        Err(arboard::Error::ContentNotAvailable) => {}
        Err(e) => return Err(format!("read clipboard image: {e}")),
    }
    match cb.get_text() {
        Ok(content) if !content.is_empty() => {
            // Retain compatibility with applications which copy paths as plain text.
            let paths = content.lines().map(str::trim).collect::<Vec<_>>();
            let is_file = !paths.is_empty()
                && paths.iter().all(|p| {
                    (std::path::Path::new(p).is_absolute() || p.starts_with("file://"))
                        && std::path::Path::new(&clipboard::normalize_file_path(p)).exists()
                });
            let kind = if is_file { "file" } else { "text" };
            Ok(Some(Capture {
                fingerprint: format!("{kind}:{content}"),
                content,
                content_type: kind,
                image_bytes: None,
            }))
        }
        Ok(_) | Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(e) => Err(format!("read clipboard text: {e}")),
    }
}

#[cfg(desktop)]
fn bitmap_capture(image: arboard::ImageData<'_>) -> Result<Capture, String> {
    let fingerprint = pixel_fingerprint(image.width, image.height, &image.bytes);
    let width = u32::try_from(image.width).map_err(|e| e.to_string())?;
    let height = u32::try_from(image.height).map_err(|e| e.to_string())?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    let rgba = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or("Invalid clipboard image dimensions")?;
    rgba.write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(Capture {
        content: String::new(),
        content_type: "image",
        image_bytes: Some(bytes.into_inner()),
        fingerprint,
    })
}

#[cfg(desktop)]
fn pixel_fingerprint(width: usize, height: usize, rgba: &[u8]) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(&(width as u64).to_le_bytes());
    hash.update(&(height as u64).to_le_bytes());
    hash.update(rgba);
    hash.finalize().to_hex().to_string()
}

#[cfg(target_os = "macos")]
fn change_count() -> Option<isize> {
    Some(objc2_app_kit::NSPasteboard::generalPasteboard().changeCount())
}
#[cfg(all(desktop, not(target_os = "macos")))]
fn change_count() -> Option<isize> {
    None
}

#[cfg(desktop)]
fn monitor(app: tauri::AppHandle) {
    let mut revision = 0;
    let mut last_count = None;
    let mut last_fingerprint = String::new();
    let mut failures = 0;
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let (enabled, current_revision) = match CONTROL.lock() {
            Ok(c) => (c.enabled, c.revision),
            Err(_) => return,
        };
        if !enabled {
            continue;
        }
        if revision != current_revision {
            revision = current_revision;
            last_count = None;
            last_fingerprint.clear();
            failures = 0;
        }
        // Release Objective-C autoreleased objects on every iteration of the long-lived thread.
        objc_capture_iteration(
            &app,
            revision,
            &mut last_count,
            &mut last_fingerprint,
            &mut failures,
        );
    }
}

#[cfg(desktop)]
fn objc_capture_iteration(
    app: &tauri::AppHandle,
    revision: u64,
    last_count: &mut Option<isize>,
    last_fingerprint: &mut String,
    failures: &mut u8,
) {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {
        capture_iteration(app, revision, last_count, last_fingerprint, failures)
    });
    #[cfg(not(target_os = "macos"))]
    capture_iteration(app, revision, last_count, last_fingerprint, failures);
}

#[cfg(desktop)]
fn capture_iteration(
    app: &tauri::AppHandle,
    revision: u64,
    last_count: &mut Option<isize>,
    last_fingerprint: &mut String,
    failures: &mut u8,
) {
    let count = change_count();
    if count.is_some() && count == *last_count {
        return;
    }
    let result = capture();
    #[cfg(target_os = "macos")]
    let result = match result {
        Ok(None)
            if objc2_app_kit::NSPasteboard::generalPasteboard()
                .types()
                .is_some_and(|types| !types.is_empty()) =>
        {
            Err("Clipboard provider has not supplied readable file, image or text data".to_string())
        }
        other => other,
    };
    // Do not persist mixed data if the clipboard changed between its representations.
    if count != change_count() {
        *failures = 0;
        return;
    }
    let Ok(control) = CONTROL.lock() else { return };
    if !control.enabled || control.revision != revision {
        return;
    }
    let saved = result.and_then(|snapshot| {
        let Some(snapshot) = snapshot else {
            last_fingerprint.clear();
            return Ok(());
        };
        // macOS changeCount records repeated copies too; other OSes use full-content fingerprints.
        if count.is_none() && snapshot.fingerprint == *last_fingerprint {
            return Ok(());
        }
        let content = match snapshot.image_bytes {
            Some(bytes) => clipboard::save_clipboard_image_bytes(app, &bytes)?,
            None => snapshot.content,
        };
        let entry: ClipboardEntry =
            clipboard::add_clipboard_entry(content, Some(snapshot.content_type.into()))?;
        *last_fingerprint = snapshot.fingerprint;
        app.emit("clipboard-history-changed", entry)
            .map_err(|e| e.to_string())?;
        Ok(())
    });
    match saved {
        Ok(()) => {
            *last_count = count;
            *failures = 0;
        }
        Err(e) => {
            *failures = failures.saturating_add(1);
            // Retry delayed providers/write failures, then wait for the next macOS change.
            if *failures <= 3 {
                tracing::warn!("[clipboard] capture failed (attempt {}): {}", failures, e);
            }
            if *failures >= 3 {
                *last_count = count;
                if count.is_some() {
                    *failures = 0;
                }
            }
        }
    }
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;
    #[test]
    fn same_dimensions_different_pixels_are_distinct() {
        assert_ne!(
            pixel_fingerprint(1, 1, &[0, 0, 0, 255]),
            pixel_fingerprint(1, 1, &[255, 0, 0, 255])
        );
        assert_eq!(
            pixel_fingerprint(1, 1, &[0, 0, 0, 255]),
            pixel_fingerprint(1, 1, &[0, 0, 0, 255])
        );
        assert_ne!(
            pixel_fingerprint(1, 2, &[0; 8]),
            pixel_fingerprint(2, 1, &[0; 8])
        );
    }
}
