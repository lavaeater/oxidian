//! Filesystem-backed key/value store for native (desktop/mobile) builds.
//!
//! On Android the `WebView`'s `localStorage` does not reliably survive a cold app
//! restart, so the GitHub token (and bookmarks, board, …) were being lost and
//! the user had to re-authorise on every launch. This stores the same keys in a
//! JSON file in the app's private directory instead, bypassing the `WebView`
//! entirely. `js::ls_get`/`ls_set`/`ls_remove` route here on native and stay on
//! real `localStorage` for web.
//!
//! Location:
//! - Android: the app's internal `getFilesDir()` (persists until the app is
//!   uninstalled or its data is cleared), obtained via JNI; falls back to the
//!   process temp dir if that lookup fails.
//! - Desktop: the OS data dir (`dirs::data_dir()/oxidian`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::console_log;

// In-memory mirror of the on-disk store, lazily loaded on first access.
static CACHE: Mutex<Option<BTreeMap<String, String>>> = Mutex::new(None);

#[cfg(target_os = "android")]
fn base_dir() -> PathBuf {
    android_files_dir().unwrap_or_else(std::env::temp_dir)
}

#[cfg(not(target_os = "android"))]
fn base_dir() -> PathBuf {
    dirs::data_dir().map_or_else(std::env::temp_dir, |d| d.join("oxidian"))
}

/// The app's internal files directory via `Context.getFilesDir()`. Best-effort:
/// any JNI hiccup returns `None` so the caller falls back to the temp dir.
#[cfg(target_os = "android")]
fn android_files_dir() -> Option<PathBuf> {
    std::panic::catch_unwind(|| {
        use jni::objects::{JObject, JString};
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let file = env
            .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])
            .ok()?
            .l()
            .ok()?;
        let path = env
            .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(path)).ok()?.into();
        Some(PathBuf::from(s))
    })
    .ok()
    .flatten()
}

fn store_path() -> PathBuf {
    let dir = base_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("oxidian_store.json")
}

fn load_map() -> BTreeMap<String, String> {
    let path = store_path();
    let existed = path.exists();
    console_log(&format!(
        "[oxidian] native_store at {} (existing={existed})",
        path.display()
    ));
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn with_cache<T>(f: impl FnOnce(&mut BTreeMap<String, String>) -> T) -> T {
    let mut guard = CACHE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(load_map());
    }
    f(guard.as_mut().expect("cache just initialized"))
}

fn persist(map: &BTreeMap<String, String>) {
    if let Ok(s) = serde_json::to_string(map)
        && let Err(e) = std::fs::write(store_path(), s)
    {
        console_log(&format!("[oxidian] native_store write failed: {e}"));
    }
}

pub fn get(key: &str) -> String {
    with_cache(|m| m.get(key).cloned().unwrap_or_default())
}

pub fn set(key: &str, value: &str) {
    with_cache(|m| {
        m.insert(key.to_string(), value.to_string());
        persist(m);
    });
}

pub fn remove(key: &str) {
    with_cache(|m| {
        m.remove(key);
        persist(m);
    });
}

// ── Large blobs ───────────────────────────────────────────────────────────────
// The vault index scales with the vault and is rewritten on every save. Keeping
// it in the settings map would mean re-encoding and rewriting every setting (and
// the token) each time, and JSON-inside-JSON doubles it through escaping. So a
// blob gets its own file, written verbatim. Mirrors IndexedDB on web.

fn blob_path(key: &str) -> PathBuf {
    let dir = base_dir();
    let _ = std::fs::create_dir_all(&dir);
    // Keys are internal constants, but a path separator would still escape the
    // directory, so make the name safe regardless.
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    dir.join(format!("{safe}.json"))
}

pub fn blob_get(key: &str) -> String {
    std::fs::read_to_string(blob_path(key)).unwrap_or_default()
}

pub fn blob_set(key: &str, value: &str) {
    if let Err(e) = std::fs::write(blob_path(key), value) {
        console_log(&format!("[oxidian] blob write failed: {e}"));
    }
}

pub fn blob_remove(key: &str) {
    let _ = std::fs::remove_file(blob_path(key));
}

/// `(usage, quota)` in bytes. Native has no quota, so only the store's own
/// footprint is reported and the quota is `-1` ("not applicable").
pub fn usage() -> (i64, i64) {
    fn dir_size(dir: &std::path::Path) -> u64 {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some(if meta.is_dir() { dir_size(&e.path()) } else { meta.len() })
            })
            .sum()
    }
    let total = dir_size(&base_dir());
    (i64::try_from(total).unwrap_or(i64::MAX), -1)
}

// ── Per-note index records ────────────────────────────────────────────────────
// One file per note, mirroring IndexedDB's `pages` store on web. Saving a note
// rewrites one small file instead of the whole index — which now carries every
// note's text for search, so it is roughly vault-sized.

fn records_dir() -> PathBuf {
    let dir = base_dir().join("pages");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Note path → filename, reversibly.
///
/// Percent-encoding rather than "replace awkward characters with `_`": that
/// would map `a/b.md` and `a_b.md` onto the same file, and one note would
/// silently overwrite the other's record.
fn encode_key(key: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(key.len());
    for b in key.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn decode_key(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = name.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn records_all() -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(records_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let key = decode_key(&name)?;
            let value = std::fs::read_to_string(e.path()).ok()?;
            Some((key, value))
        })
        .collect()
}

pub fn records_put(entries: &[(String, String)]) {
    let dir = records_dir();
    for (key, value) in entries {
        if let Err(e) = std::fs::write(dir.join(encode_key(key)), value) {
            console_log(&format!("[oxidian] record write failed: {e}"));
        }
    }
}

pub fn records_delete(keys: &[String]) {
    let dir = records_dir();
    for key in keys {
        let _ = std::fs::remove_file(dir.join(encode_key(key)));
    }
}

pub fn records_clear() {
    let _ = std::fs::remove_dir_all(records_dir());
}

#[cfg(test)]
mod tests {
    use super::{decode_key, encode_key};

    #[test]
    fn record_keys_round_trip_and_never_collide() {
        for key in ["a/b.md", "a_b.md", "Åsa's note.md", "x.md", "deep/a/b/c.md"] {
            assert_eq!(decode_key(&encode_key(key)).as_deref(), Some(key), "{key}");
        }
        // The collision a naive sanitiser would produce.
        assert_ne!(encode_key("a/b.md"), encode_key("a_b.md"));
        // Encoded names stay filename-safe.
        assert!(!encode_key("a/b.md").contains('/'));
    }

    #[test]
    fn a_malformed_filename_is_skipped_rather_than_panicking() {
        assert_eq!(decode_key("%ZZ"), None);
        assert_eq!(decode_key("%"), None);
    }
}
