pub fn get_internal_ips() -> Vec<String> {
    return vec!["127.0.0.1".to_string(), "192.168.1.10".to_string()];
}

pub fn get_external_ip() -> Option<String> {
    return Some("43.1.2.3".to_string());
}

pub fn get_os() -> Option<String> {
    return Some("Windows11".to_string());
}

pub fn get_arch() -> Option<String> {
    return Some("x86_64".to_string());
}

pub fn get_user() -> Option<String> {
    return Some("user".to_string());
}

pub fn get_hostname() -> Option<String> {
    return Some("DESKTOP-DS2T5".to_string());
}

pub fn get_pid() -> Option<u32> {
    return Some(4455);
}

pub fn get_domain() -> Option<String> {
    return Some("localhost".to_string());
}

pub fn get_integrity_level() -> Option<u32> {
    return Some(0);
}

pub fn get_process_name() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    return Some(args[0].to_string());
}
