//! System information — cross-platform.
//!
//! Linux: /proc/cpuinfo, /proc/meminfo, uname, /etc/os-release
//! macOS: sysctl, sw_vers
//! Windows: systeminfo command / GetSystemInfo

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;
use serde_json::Value;

#[cfg(target_os = "linux")]
fn gather_sysinfo() -> Result<Value, String> {
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let os_name = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| {
                l.trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string()
            })
        })
        .unwrap_or_else(|| "Linux".into());

    let kernel = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let arch = std::env::consts::ARCH.to_string();

    let cpu_info = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu_model = cpu_info
        .lines()
        .find(|l| l.starts_with("model name"))
        .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let cpu_cores = cpu_info
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    let mem_info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mem_total = mem_info
        .lines()
        .find(|l| l.starts_with("MemTotal:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    Ok(serde_json::json!({
        "hostname": hostname,
        "os": os_name,
        "kernel": kernel,
        "arch": arch,
        "cpu_model": cpu_model,
        "cpu_cores": cpu_cores,
        "mem_total_kb": mem_total,
        "uptime_secs": uptime,
    }))
}

#[cfg(target_os = "macos")]
fn gather_sysinfo() -> Result<Value, String> {
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let os_name = std::process::Command::new("sw_vers")
        .arg("-productName")
        .output()
        .ok()
        .and_then(|o| {
            std::process::Command::new("sw_vers")
                .arg("-productVersion")
                .output()
                .ok()
                .map(|v| format!("macOS {}", String::from_utf8_lossy(&v.stdout).trim()))
        })
        .unwrap_or_else(|| "macOS".into());

    let kernel = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let arch = std::process::Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| std::env::consts::ARCH.into());

    let cpu_model = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let cpu_cores = std::process::Command::new("sysctl")
        .args(["-n", "hw.ncpu"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0);

    let mem_total = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map(|b| b / 1024)
        .unwrap_or(0);

    let uptime = std::process::Command::new("sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            // Format: { sec = 12345, usec = 67890 } ...
            s.split("sec = ")
                .nth(1)?
                .split(',')
                .next()?
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map(|boot| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - boot
        })
        .unwrap_or(0);

    Ok(serde_json::json!({
        "hostname": hostname,
        "os": os_name,
        "kernel": kernel,
        "arch": arch,
        "cpu_model": cpu_model,
        "cpu_cores": cpu_cores,
        "mem_total_kb": mem_total,
        "uptime_secs": uptime,
    }))
}

#[cfg(windows)]
fn gather_sysinfo() -> Result<Value, String> {
    use crate::sys::encoding::decode_output;
    use windows_sys::Win32::System::SystemInformation::{
        GetSystemInfo, GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
    };

    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let arch = std::env::consts::ARCH.to_string();

    let os_name = {
        let out = std::process::Command::new("systeminfo")
            .output()
            .map(|o| decode_output(&o.stdout))
            .unwrap_or_default();

        out.lines()
            .find(|l| l.contains("OS Name"))
            .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
            .unwrap_or_else(|| "Windows".into())
    };
    let cpu_cores = unsafe {
        let mut info: SYSTEM_INFO = std::mem::zeroed();
        GetSystemInfo(&mut info);
        u64::from(info.dwNumberOfProcessors)
    };

    let cpu_model = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default();
    let mem_total = unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..std::mem::zeroed()
        };
        if GlobalMemoryStatusEx(&mut status) != 0 {
            status.ullTotalPhys / 1024
        } else {
            0
        }
    };
    let uptime = unsafe { GetTickCount64() / 1000 };

    Ok(serde_json::json!({
        "hostname": hostname,
        "os": os_name,
        "kernel": "Windows NT",
        "arch": arch,
        "cpu_model": cpu_model,
        "cpu_cores": cpu_cores,
        "mem_total_kb": mem_total,
        "uptime_secs": uptime,
    }))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn gather_sysinfo() -> Result<Value, String> {
    Ok(serde_json::json!({
        "hostname": std::process::Command::new("hostname").output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".into()),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct Params {}
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    if let Err(e) = serde_json::from_str::<Params>(parameters) {
        return TaskResponse::failed(task.id, &format!("sysinfo parse error: {e}"));
    }
    match gather_sysinfo() {
        Ok(info) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(serde_json::to_string_pretty(&info).unwrap_or_default()),
            ..Default::default()
        },
        Err(e) => TaskResponse::failed(task.id, &e),
    }
}
