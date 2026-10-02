use crate::db::database::get_connection;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
#[cfg(target_os = "macos")]
use std::process::{Command, Output, Stdio};
#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: String,
    pub content_type: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardFileReference {
    pub path: String,
    pub is_image: bool,
}

#[cfg(target_os = "macos")]
static LAST_NATIVE_FILE_REFS_LOG_KEY: Mutex<Option<String>> = Mutex::new(None);

#[tauri::command]
pub fn get_clipboard_history(limit: Option<i32>) -> Result<Vec<ClipboardEntry>, String> {
    let limit = limit.unwrap_or(100);
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, content, content_type, created_at FROM clipboard_history ORDER BY created_at DESC LIMIT ?1")
        .map_err(|e| e.to_string())?;
    let entries = stmt
        .query_map(rusqlite::params![limit], |row| {
            Ok(ClipboardEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                content_type: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(entries)
}

#[tauri::command]
pub fn add_clipboard_entry(
    content: String,
    content_type: Option<String>,
) -> Result<ClipboardEntry, String> {
    let content_type = normalize_content_type(&content_type.unwrap_or_else(|| "text".to_string()));
    let created_at = chrono::Utc::now().to_rfc3339();

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let mut content = content;
    if is_file_reference_type(&content_type) {
        content = normalize_file_reference_content(&content);
    }
    let content_type = if content_type == "file" && is_single_image_file_reference(&content) {
        "image_file".to_string()
    } else {
        content_type
    };
    let content_hash = content_hash_for_entry(&content, &content_type);

    // De-duplicate:
    // - text/file/image_file/unknown: same content + same normalized type
    // - image bitmap: same file/data bytes, because new screenshot captures may have different UUID paths
    if let Some(mut existing) =
        find_duplicate_entry(conn, &content, &content_type, content_hash.as_deref())?
    {
        conn.execute(
            "UPDATE clipboard_history SET created_at = ?1, content_hash = COALESCE(content_hash, ?2) WHERE id = ?3",
            rusqlite::params![created_at, content_hash, existing.id],
        )
        .map_err(|e| e.to_string())?;

        // If the new image was just saved as a duplicate UUID file, remove it to avoid orphan files.
        if is_bitmap_image_type(&content_type) && existing.content != content {
            cleanup_stored_image_file(&content);
        }

        existing.created_at = created_at;

        tracing::info!(
            "[clipboard] duplicate entry promoted: id={}, type={}",
            existing.id,
            existing.content_type
        );

        return Ok(existing);
    }

    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO clipboard_history (id, content, content_type, content_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, content, content_type, content_hash, created_at],
    )
    .map_err(|e| e.to_string())?;

    Ok(ClipboardEntry {
        id,
        content,
        content_type,
        created_at,
    })
}

#[tauri::command]
pub fn delete_clipboard_entry(id: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let deleted_image = conn
        .query_row(
            "SELECT content FROM clipboard_history
             WHERE id = ?1 AND (lower(content_type) = 'image' OR lower(content_type) LIKE 'image/%')",
            rusqlite::params![id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM clipboard_history WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    if let Some(content) = deleted_image {
        cleanup_stored_image_file(&content);
    }
    Ok(())
}

#[tauri::command]
pub fn get_clipboard_entry(id: String) -> Result<ClipboardEntry, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.query_row(
        "SELECT id, content, content_type, created_at FROM clipboard_history WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(ClipboardEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                content_type: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

static THUMBNAIL_READ_LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Resolve by history ID instead of exposing arbitrary external paths to the asset protocol.
#[tauri::command]
pub async fn get_clipboard_image_thumbnail(id: String) -> Result<String, String> {
    let _permit = THUMBNAIL_READ_LIMIT
        .acquire()
        .await
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let entry = get_clipboard_entry(id)?;
        if !is_bitmap_image_type(&entry.content_type) && entry.content_type != "image_file" {
            return Err("clipboard entry is not an image".into());
        }
        encode_clipboard_thumbnail(&entry.content)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn encode_clipboard_thumbnail(content: &str) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose};
    let bytes = read_image_bytes_from_content(content)?;
    let image = decode_thumbnail_image(&bytes)?;
    let thumbnail = image.thumbnail(160, 160);
    let mut png = std::io::Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(png.into_inner())
    ))
}

fn decode_thumbnail_image(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    match image::load_from_memory(bytes) {
        Ok(image) => Ok(image),
        Err(error) => {
            #[cfg(target_os = "macos")]
            {
                use objc2::AnyThread;
                use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
                use objc2_foundation::{NSData, NSDictionary};
                // Let macOS decode formats not enabled in the Rust image crate (e.g. GIF).
                objc2::rc::autoreleasepool(|_| {
                    let data = NSData::with_bytes(bytes);
                    let rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &data)
                        .ok_or_else(|| format!("decode thumbnail failed: {error}"))?;
                    // SAFETY: the empty property dictionary has no values with mismatched types.
                    let png = unsafe {
                        rep.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                    }
                    .ok_or_else(|| "native thumbnail conversion failed".to_string())?;
                    image::load_from_memory(&png.to_vec()).map_err(|e| e.to_string())
                })
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err(format!("decode thumbnail failed: {error}"))
            }
        }
    }
}

#[tauri::command]
pub fn clear_clipboard_history(app: tauri::AppHandle) -> Result<usize, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let deleted = conn
        .execute("DELETE FROM clipboard_history", [])
        .map_err(|e| e.to_string())?;
    cleanup_clipboard_image_dir(&app);
    Ok(deleted)
}

#[tauri::command]
pub async fn read_clipboard_file_references() -> Result<Vec<ClipboardFileReference>, String> {
    tauri::async_runtime::spawn_blocking(read_clipboard_file_references_impl)
        .await
        .map_err(|e| format!("read clipboard file references task failed: {e}"))?
}

#[cfg(target_os = "macos")]
pub(super) fn read_clipboard_file_references_impl() -> Result<Vec<ClipboardFileReference>, String> {
    objc2::rc::autoreleasepool(|_| {
        read_file_refs_from_pasteboard(&objc2_app_kit::NSPasteboard::generalPasteboard())
    })
}

#[cfg(target_os = "macos")]
fn read_file_refs_from_pasteboard(
    pb: &objc2_app_kit::NSPasteboard,
) -> Result<Vec<ClipboardFileReference>, String> {
    use objc2_foundation::{NSString, NSURL};
    let mut refs = Vec::new();
    if let Some(items) = pb.pasteboardItems() {
        let file_type = NSString::from_str("public.file-url");
        for item in items.iter() {
            let Some(value) = item.stringForType(&file_type) else {
                continue;
            };
            let Some(url) = NSURL::URLWithString(&value) else {
                continue;
            };
            if !url.isFileURL() {
                continue;
            }
            let Some(path) = url.path() else { continue };
            let path = path.to_string();
            // Keep the reference even when the source file is protected or transient.
            // A failed stat must not turn a real file into its preview bitmap.
            refs.push(ClipboardFileReference {
                is_image: path_has_image_extension(&path),
                path,
            });
        }
    }

    refs.sort_by(|a, b| a.path.cmp(&b.path));
    refs.dedup_by(|a, b| a.path == b.path);

    if !refs.is_empty() && should_log_native_file_refs(&refs) {
        tracing::info!(
            "[clipboard] native file references detected: count={}, first={}",
            refs.len(),
            refs[0].path
        );
    }

    Ok(refs)
}

#[cfg(target_os = "macos")]
fn should_log_native_file_refs(refs: &[ClipboardFileReference]) -> bool {
    let key = refs
        .iter()
        .map(|item| item.path.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    match LAST_NATIVE_FILE_REFS_LOG_KEY.lock() {
        Ok(mut guard) => {
            if guard.as_deref() == Some(key.as_str()) {
                false
            } else {
                *guard = Some(key);
                true
            }
        }
        Err(_) => true,
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn read_clipboard_file_references_impl() -> Result<Vec<ClipboardFileReference>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn save_clipboard_image_data_url(
    app: tauri::AppHandle,
    data_url: String,
) -> Result<String, String> {
    let bytes = decode_data_url(&data_url)?
        .ok_or_else(|| "invalid image data URL: expected data:image/...;base64,...".to_string())?;

    save_clipboard_image_bytes(&app, &bytes)
}

pub(super) fn save_clipboard_image_bytes(
    app: &tauri::AppHandle,
    bytes: &[u8],
) -> Result<String, String> {
    // Validate the image before saving, so broken bytes do not enter history.
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("decode image before save failed: {e}"))?;
    let width = img.width();
    let height = img.height();

    // If the same image is already in history, return its existing path.
    // add_clipboard_entry will then update created_at and move it to the top.
    if let Ok(db) = get_connection()
        && let Some(conn) = db.as_ref()
        && let Some(existing) = find_duplicate_image_entry_by_bytes(conn, bytes)?
    {
        tracing::info!(
            "[clipboard] duplicate image save skipped, reuse existing path: id={}, path={}",
            existing.id,
            existing.content
        );
        return Ok(existing.content);
    }

    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("get app data dir failed: {e}"))?;

    let image_dir = app_dir.join("clipboard-images");
    std::fs::create_dir_all(&image_dir).map_err(|e| {
        format!(
            "create image dir failed: path={}, err={e}",
            image_dir.display()
        )
    })?;

    let file_name = format!("{}.png", uuid::Uuid::new_v4());
    let file_path = image_dir.join(file_name);

    std::fs::write(&file_path, bytes).map_err(|e| {
        format!(
            "write image file failed: path={}, err={e}",
            file_path.display()
        )
    })?;

    let path = file_path.to_string_lossy().to_string();

    tracing::info!(
        "[clipboard] image saved to file: path={}, size={}x{}",
        path,
        width,
        height
    );

    Ok(path)
}

#[tauri::command]
pub fn write_clipboard_entry_to_system(
    content: String,
    content_type: String,
) -> Result<(), String> {
    let normalized_type = normalize_content_type(&content_type);

    if is_file_reference_type(&normalized_type) {
        write_file_reference_to_clipboard(&content, &normalized_type)
    } else if is_bitmap_image_type(&normalized_type) {
        write_image_content_to_clipboard(&content)
    } else {
        write_text_to_clipboard(&content)
    }
}

fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;

    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("set text clipboard failed: {e}"))?;

    tracing::info!(
        "[clipboard] text written to system clipboard, len={}",
        text.len()
    );
    Ok(())
}

fn write_image_content_to_clipboard(content: &str) -> Result<(), String> {
    let bytes = read_image_bytes_from_content(content)?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("decode image failed: {e}"))?
        .to_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    let expected_len = width as usize * height as usize * 4;
    if rgba.len() != expected_len {
        return Err(format!(
            "invalid RGBA image data length: got {}, expected {}",
            rgba.len(),
            expected_len
        ));
    }

    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;

    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(rgba),
        })
        .map_err(|e| format!("set image clipboard failed: {e}"))?;

    tracing::info!(
        "[clipboard] image written to system clipboard, size={}x{}",
        width,
        height
    );

    Ok(())
}

fn find_duplicate_entry(
    conn: &Connection,
    content: &str,
    content_type: &str,
    content_hash: Option<&str>,
) -> Result<Option<ClipboardEntry>, String> {
    let normalized_type = normalize_content_type(content_type);

    if is_bitmap_image_type(&normalized_type) {
        return match content_hash {
            Some(_) => {
                let bytes = read_image_bytes_from_content(content)?;
                find_duplicate_image_entry_by_bytes(conn, &bytes)
            }
            None => find_duplicate_image_entry_by_exact_content(conn, content),
        };
    }

    conn.query_row(
        "SELECT id, content, content_type, created_at
         FROM clipboard_history
         WHERE content = ?1 AND lower(content_type) = ?2
         ORDER BY created_at DESC
         LIMIT 1",
        rusqlite::params![content, normalized_type],
        row_to_clipboard_entry,
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn find_duplicate_image_entry_by_exact_content(
    conn: &Connection,
    content: &str,
) -> Result<Option<ClipboardEntry>, String> {
    conn.query_row(
        "SELECT id, content, content_type, created_at
         FROM clipboard_history
         WHERE content = ?1 AND (lower(content_type) = 'image' OR lower(content_type) LIKE 'image/%')
         ORDER BY created_at DESC
         LIMIT 1",
        rusqlite::params![content],
        row_to_clipboard_entry,
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn find_duplicate_image_entry_by_hash(
    conn: &Connection,
    target_hash: &str,
) -> Result<Option<ClipboardEntry>, String> {
    conn.query_row(
        "SELECT id, content, content_type, created_at
         FROM clipboard_history
         WHERE content_hash = ?1
            AND (lower(content_type) = 'image' OR lower(content_type) LIKE 'image/%')
         ORDER BY created_at DESC
         LIMIT 1",
        rusqlite::params![target_hash],
        row_to_clipboard_entry,
    )
    .optional()
    .map_err(|e| e.to_string())
}

// Upgrade old FNV-1a hashes lazily, without discarding existing history.
fn find_duplicate_image_entry_by_bytes(
    conn: &Connection,
    bytes: &[u8],
) -> Result<Option<ClipboardEntry>, String> {
    let hash = image_bytes_hash(bytes);
    for legacy in [false, true] {
        let key = if legacy {
            legacy_image_bytes_hash(bytes)
        } else {
            hash.clone()
        };
        if let Some(entry) = find_duplicate_image_entry_by_hash(conn, &key)? {
            // Verify bytes as well: a hash collision must not drop clipboard content.
            if read_image_bytes_from_content(&entry.content).is_ok_and(|old| old == bytes) {
                if legacy {
                    conn.execute(
                        "UPDATE clipboard_history SET content_hash = ?1 WHERE id = ?2",
                        rusqlite::params![hash, entry.id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                return Ok(Some(entry));
            }
        }
    }
    Ok(None)
}

fn row_to_clipboard_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardEntry> {
    Ok(ClipboardEntry {
        id: row.get(0)?,
        content: row.get(1)?,
        content_type: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn content_hash_for_entry(content: &str, content_type: &str) -> Option<String> {
    if !is_bitmap_image_type(content_type) {
        return None;
    }

    match read_image_bytes_from_content(content) {
        Ok(bytes) => Some(image_bytes_hash(&bytes)),
        Err(err) => {
            tracing::warn!("[clipboard] image content hash failed: {err}");
            None
        }
    }
}

fn image_bytes_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn legacy_image_bytes_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn normalize_content_type(content_type: &str) -> String {
    let value = content_type.trim().to_lowercase();
    if value.is_empty() {
        "text".to_string()
    } else {
        value
    }
}

fn is_bitmap_image_type(content_type: &str) -> bool {
    content_type == "image" || content_type.starts_with("image/")
}

fn is_file_reference_type(content_type: &str) -> bool {
    content_type == "file" || content_type == "image_file"
}

#[cfg(target_os = "macos")]
fn write_file_reference_to_clipboard(content: &str, content_type: &str) -> Result<(), String> {
    let paths = extract_file_reference_paths(content);
    if paths.is_empty() {
        return Err("file reference is empty or path does not exist".to_string());
    }

    let items = paths
        .iter()
        .map(|path| format!("(POSIX file \"{}\")", escape_applescript_string(path)))
        .collect::<Vec<_>>()
        .join(", ");

    let script = if paths.len() == 1 {
        format!("set the clipboard to {}", items)
    } else {
        format!("set the clipboard to {{{}}}", items)
    };

    let out = run_osascript_with_timeout(&script, Duration::from_millis(1500))
        .map_err(|e| format!("osascript write file reference failed: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if content_type == "image_file" {
            tracing::warn!(
                "[clipboard] file reference pasteboard failed, fallback to bitmap image: {}",
                err
            );
            return write_image_content_to_clipboard(content);
        }
        return Err(format!("write file reference failed: {err}"));
    }

    tracing::info!(
        "[clipboard] file reference written to system clipboard: type={}, count={}",
        content_type,
        paths.len()
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn write_file_reference_to_clipboard(content: &str, _content_type: &str) -> Result<(), String> {
    // Cross-platform file-reference pasteboard support is OS-specific.
    // Keep a safe fallback so the path is still copyable.
    write_text_to_clipboard(content)
}

fn extract_file_reference_paths(content: &str) -> Vec<String> {
    content
        .lines()
        .map(normalize_file_path)
        .filter(|path| !path.is_empty())
        .filter(|path| std::path::Path::new(path).exists())
        .collect()
}

fn normalize_file_reference_content(content: &str) -> String {
    let paths = content
        .lines()
        .map(normalize_file_path)
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();

    if paths.is_empty() {
        content.trim().to_string()
    } else {
        paths.join("\n")
    }
}

pub(super) fn normalize_file_path(value: &str) -> String {
    let mut path = value.trim().to_string();
    if path.starts_with("file://localhost") {
        path = path.trim_start_matches("file://localhost").to_string();
    } else if path.starts_with("file://") {
        path = path.trim_start_matches("file://").to_string();
    }

    percent_decode_path(&path)
}

#[cfg(target_os = "macos")]
fn run_osascript_with_timeout(script: &str, timeout: Duration) -> Result<Output, String> {
    let mut child = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().map_err(|e| e.to_string()),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {}ms", timeout.as_millis()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}

fn percent_decode_path(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }

    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_single_image_file_reference(content: &str) -> bool {
    let paths = extract_file_reference_paths(content);
    paths.len() == 1 && path_has_image_extension(&paths[0])
}

fn path_has_image_extension(path: &str) -> bool {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default();

    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "gif"
            | "bmp"
            | "tif"
            | "tiff"
            | "heic"
            | "heif"
            | "avif"
            | "ico"
    )
}

fn cleanup_stored_image_file(content: &str) {
    let path = content.trim().trim_start_matches("file://");

    // Safety guard: only remove files from our clipboard image cache.
    if path.is_empty() || !path.contains("clipboard-images") {
        return;
    }

    match std::fs::remove_file(path) {
        Ok(_) => tracing::info!("[clipboard] stored image file removed: {}", path),
        Err(e) => tracing::warn!(
            "[clipboard] stored image file remove failed: path={}, err={}",
            path,
            e
        ),
    }
}

fn cleanup_clipboard_image_dir(app: &tauri::AppHandle) {
    let Ok(app_dir) = app.path().app_data_dir() else {
        return;
    };
    let image_dir = app_dir.join("clipboard-images");
    if !image_dir.exists() {
        return;
    }
    if let Err(err) = std::fs::remove_dir_all(&image_dir) {
        tracing::warn!(
            "[clipboard] clipboard image dir cleanup failed: path={}, err={}",
            image_dir.display(),
            err
        );
        return;
    }
    if let Err(err) = std::fs::create_dir_all(&image_dir) {
        tracing::warn!(
            "[clipboard] clipboard image dir recreate failed: path={}, err={}",
            image_dir.display(),
            err
        );
    }
}

fn read_image_bytes_from_content(content: &str) -> Result<Vec<u8>, String> {
    let trimmed = content.trim();

    if trimmed.is_empty() {
        return Err("image content is empty".to_string());
    }

    if let Some(data_url_bytes) = decode_data_url(trimmed)? {
        return Ok(data_url_bytes);
    }

    if trimmed.starts_with("file://") || trimmed.starts_with('/') {
        let path = normalize_file_path(trimmed);
        return std::fs::read(&path).map_err(|e| {
            let marker = if e.kind() == std::io::ErrorKind::PermissionDenied
                || e.raw_os_error() == Some(1)
            {
                "CLIPBOARD_IMAGE_ACCESS_DENIED: "
            } else {
                ""
            };
            format!("{marker}read image file failed: path={path}, err={e}")
        });
    }

    // Some old records may only store raw base64 without the data:image/... prefix.
    if let Ok(bytes) = decode_base64(trimmed) {
        return Ok(bytes);
    }

    Err(
        "unsupported image content; expected data URL, file:// URL, absolute path, or base64"
            .to_string(),
    )
}

fn decode_data_url(value: &str) -> Result<Option<Vec<u8>>, String> {
    if !value.starts_with("data:") {
        return Ok(None);
    }

    let comma_index = value
        .find(',')
        .ok_or_else(|| "invalid data URL: missing comma".to_string())?;

    let (meta, payload_with_comma) = value.split_at(comma_index);
    let payload = &payload_with_comma[1..];

    if !meta.to_lowercase().contains(";base64") {
        return Err("unsupported data URL: only base64 image data is supported".to_string());
    }

    decode_base64(payload).map(Some)
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    use base64::{Engine as _, engine::general_purpose};

    general_purpose::STANDARD
        .decode(value.as_bytes())
        .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(value.as_bytes()))
        .map_err(|e| format!("base64 decode failed: {e}"))
}

#[tauri::command]
pub fn cleanup_clipboard(
    app: tauri::AppHandle,
    mode: String,
    days: Option<i64>,
    count: Option<i64>,
) -> Result<usize, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let deleted_images = match mode.as_str() {
        "time" => {
            let days = days.unwrap_or(7).max(1);
            collect_image_contents_for_where(
                conn,
                "created_at < datetime('now', ?1)",
                rusqlite::params![format!("-{} days", days)],
            )?
        }
        "count" => {
            let max_count = count.unwrap_or(200).max(10);
            collect_image_contents_for_where(
                conn,
                "id NOT IN (SELECT id FROM clipboard_history ORDER BY created_at DESC LIMIT ?1)",
                rusqlite::params![max_count],
            )?
        }
        _ => Vec::new(),
    };

    let deleted = match mode.as_str() {
        "time" => {
            let days = days.unwrap_or(7).max(1);
            conn.execute(
                "DELETE FROM clipboard_history WHERE created_at < datetime('now', ?1)",
                rusqlite::params![format!("-{} days", days)],
            )
            .map_err(|e| e.to_string())?
        }
        "count" => {
            let max_count = count.unwrap_or(200).max(10);
            conn.execute(
                "DELETE FROM clipboard_history WHERE id NOT IN (SELECT id FROM clipboard_history ORDER BY created_at DESC LIMIT ?1)",
                rusqlite::params![max_count],
            ).map_err(|e| e.to_string())?
        }
        _ => 0,
    };

    if deleted > 0 {
        tracing::info!(
            "[clipboard] cleanup: deleted {} entries (mode={})",
            deleted,
            mode
        );
    }

    for content in deleted_images {
        cleanup_stored_image_file(&content);
    }
    cleanup_unreferenced_clipboard_images(&app, conn);

    Ok(deleted)
}

fn collect_image_contents_for_where<P>(
    conn: &Connection,
    where_clause: &str,
    params: P,
) -> Result<Vec<String>, String>
where
    P: rusqlite::Params,
{
    let sql = format!(
        "SELECT content FROM clipboard_history
         WHERE ({where_clause})
           AND (lower(content_type) = 'image' OR lower(content_type) LIKE 'image/%')"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let contents = stmt
        .query_map(params, |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(contents)
}

fn cleanup_unreferenced_clipboard_images(app: &tauri::AppHandle, conn: &Connection) {
    let Ok(app_dir) = app.path().app_data_dir() else {
        return;
    };
    let image_dir = app_dir.join("clipboard-images");
    let Ok(files) = std::fs::read_dir(&image_dir) else {
        return;
    };

    for file in files.filter_map(|entry| entry.ok()) {
        let path = file.path();
        if !path.is_file() {
            continue;
        }
        let path_text = path.to_string_lossy().to_string();
        let referenced = conn
            .query_row(
                "SELECT 1 FROM clipboard_history WHERE content = ?1 OR content = ?2 LIMIT 1",
                rusqlite::params![path_text, format!("file://{path_text}")],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some();
        if !referenced {
            cleanup_stored_image_file(&path_text);
        }
    }
}

#[cfg(test)]
mod hash_tests {
    use super::*;

    fn history_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE clipboard_history (id TEXT PRIMARY KEY, content TEXT, content_type TEXT, content_hash TEXT, created_at TEXT);").unwrap();
        conn
    }

    #[test]
    fn legacy_image_hash_is_upgraded_without_replacing_history() {
        let conn = history_db();
        let bytes = b"old image bytes";
        let content = format!(
            "data:image/png;base64,{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
        );
        conn.execute(
            "INSERT INTO clipboard_history VALUES ('old', ?1, 'image', ?2, '2026-01-01')",
            rusqlite::params![content, legacy_image_bytes_hash(bytes)],
        )
        .unwrap();
        let entry = find_duplicate_image_entry_by_bytes(&conn, bytes)
            .unwrap()
            .unwrap();
        assert_eq!(entry.id, "old");
        assert_eq!(
            conn.query_row("SELECT content_hash FROM clipboard_history", [], |r| r
                .get::<_, String>(
                0
            ))
            .unwrap(),
            image_bytes_hash(bytes)
        );
        assert!(
            find_duplicate_image_entry_by_bytes(&conn, b"new image bytes")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn matching_hash_with_different_bytes_does_not_drop_content() {
        let conn = history_db();
        conn.execute("INSERT INTO clipboard_history VALUES ('other', 'data:image/png;base64,YQ==', 'image', ?1, '2026-01-01')", [image_bytes_hash(b"b")]).unwrap();
        assert!(
            find_duplicate_image_entry_by_bytes(&conn, b"b")
                .unwrap()
                .is_none()
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod native_file_tests {
    use super::*;
    #[test]
    fn reads_native_file_url_without_stat_or_script_output() {
        objc2::rc::autoreleasepool(|_| {
            use objc2_app_kit::NSPasteboard;
            use objc2_foundation::NSString;
            // A separate pasteboard keeps the user's real clipboard untouched.
            let pb = NSPasteboard::pasteboardWithUniqueName();
            pb.clearContents();
            assert!(pb.setString_forType(
                &NSString::from_str("file:///nonexistent/clipboard%20image.png"),
                &NSString::from_str("public.file-url")
            ));
            let refs = read_file_refs_from_pasteboard(&pb).unwrap();
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].path, "/nonexistent/clipboard image.png");
            assert!(refs[0].is_image);
            pb.clearContents();
        });
    }
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;
    #[test]
    fn reads_external_image_and_returns_bounded_png_data() {
        use base64::{Engine as _, engine::general_purpose};
        let path =
            std::env::temp_dir().join(format!("nalu-container-image-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(320, 240, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let result = encode_clipboard_thumbnail(path.to_str().unwrap());
        std::fs::remove_file(&path).unwrap();
        let data_url = result.unwrap();
        let bytes = general_purpose::STANDARD
            .decode(data_url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (160, 120));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_decodes_gif_without_rust_gif_feature() {
        let image = encode_clipboard_thumbnail(
            "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7",
        )
        .unwrap();
        assert!(image.starts_with("data:image/png;base64,"));
    }
    #[test]
    fn inaccessible_or_missing_source_is_an_error_not_a_broken_asset_url() {
        let path =
            std::env::temp_dir().join(format!("missing-nalu-image-{}.png", uuid::Uuid::new_v4()));
        assert!(encode_clipboard_thumbnail(path.to_str().unwrap()).is_err());
    }
}
