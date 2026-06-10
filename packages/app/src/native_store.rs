//! Filesystem-backed key/value store for native (desktop/mobile) builds.
//!
//! On Android the WebView's `localStorage` does not reliably survive a cold app
//! restart, so the GitHub token (and bookmarks, board, …) were being lost and
//! the user had to re-authorise on every launch. This stores the same keys in a
//! JSON file in the app's private directory instead, bypassing the WebView
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
    dirs::data_dir()
        .map(|d| d.join("oxidian"))
        .unwrap_or_else(std::env::temp_dir)
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
    if let Ok(s) = serde_json::to_string(map) {
        if let Err(e) = std::fs::write(store_path(), s) {
            console_log(&format!("[oxidian] native_store write failed: {e}"));
        }
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
