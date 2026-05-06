/// Validate that a string is a valid IPv4 address.
pub fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

/// Validate that a string is a plausible hostname (letters, digits, dots, hyphens).
pub fn is_valid_hostname(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !s.starts_with('-')
        && !s.starts_with('.')
}

/// Validate a log file path — must be under /tmp/ and not contain path traversal.
pub fn is_valid_log_path(s: &str) -> bool {
    s.starts_with("/tmp/openvpngui-") && !s.contains("..")
}

/// Check that a process with the given PID is actually openfortivpn.
/// Returns true if the PID exists and its command name contains "openfortivpn".
pub fn is_openfortivpn_pid(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    match output {
        Ok(o) => {
            let comm = String::from_utf8_lossy(&o.stdout);
            comm.trim().contains("openfortivpn")
        }
        Err(_) => false,
    }
}

/// Validate that a gateway is a valid IPv4 address.
pub fn is_valid_gateway(s: &str) -> bool {
    is_valid_ipv4(s)
}

/// Reject args that could cause openfortivpn/pppd to execute arbitrary code.
/// We block args that reference plugin paths or arbitrary script execution.
const BLOCKED_ARG_PREFIXES: &[&str] = &[
    "--plugin",
    "--pppd-plugin",
    "--pppd-ifname",
    "--pppd-call",
];

pub fn validate_vpn_args(args: &[String]) -> Result<(), String> {
    for arg in args {
        let lower = arg.to_lowercase();
        for prefix in BLOCKED_ARG_PREFIXES {
            if lower.starts_with(prefix) {
                return Err(format!("Blocked argument: {}", arg));
            }
        }
    }
    Ok(())
}

pub fn validate_env_vars(env_vars: &[(String, String)]) -> Result<(), String> {
    for (key, value) in env_vars {
        match key.as_str() {
            "OPENSSL_CONF" => {
                if !value.starts_with("/tmp/openvpngui-openssl-") || value.contains("..") {
                    return Err(format!("Invalid OPENSSL_CONF path: {}", value));
                }
            }
            "PKCS11_PROVIDER_MODULE" => {
                if !value.starts_with("/usr/lib") && !value.starts_with("/opt/") {
                    return Err(format!("Invalid PKCS11 provider path: {}", value));
                }
            }
            _ => return Err(format!("Blocked environment variable: {}", key)),
        }
    }
    Ok(())
}

pub fn openfortivpn_path() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "/opt/homebrew/bin/openfortivpn"
    }

    #[cfg(not(target_os = "macos"))]
    {
        for path in allowed_openfortivpn_paths() {
            if std::path::Path::new(path).exists() {
                return path;
            }
        }
        "/usr/bin/openfortivpn"
    }
}

pub fn is_allowed_openfortivpn_path(path: &str) -> bool {
    // Canonicalize input path to resolve any `..` components
    let canonical_input = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string());

    // Static list check (canonicalize each allowed path too)
    for allowed in allowed_openfortivpn_paths() {
        let canonical_allowed = std::fs::canonicalize(allowed)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| allowed.to_string());
        if canonical_input == canonical_allowed {
            return true;
        }
    }

    // Policy-based check: filename must be exactly "openfortivpn"
    // and parent directory must be under a trusted prefix.
    #[cfg(not(target_os = "macos"))]
    {
        let p = std::path::Path::new(&canonical_input);
        if p.file_name() == Some(std::ffi::OsStr::new("openfortivpn")) {
            let trusted_prefixes = [
                "/usr/",
                "/opt/",
                "/usr/lib/openfortivpn-connect/",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../target/"),
            ];
            if let Some(parent) = p.parent() {
                let parent_str = parent.to_string_lossy();
                for prefix in &trusted_prefixes {
                    // Canonicalize the prefix to resolve any `..` segments
                    let canon_prefix = std::fs::canonicalize(prefix.trim_end_matches('/'))
                        .map(|p| p.to_string_lossy().to_string() + "/")
                        .unwrap_or_else(|_| prefix.to_string());
                    if parent_str.starts_with(prefix) || parent_str.starts_with(&canon_prefix) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn allowed_openfortivpn_paths() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &["/opt/homebrew/bin/openfortivpn"]
    }

    #[cfg(not(target_os = "macos"))]
    {
        &[
            // Path compiled into source tree (dev build from source)
            concat!(env!("CARGO_MANIFEST_DIR"), "/../openfortivpn/openfortivpn"),
            // Tauri dev mode can resolve resources inside src-tauri/target/debug/
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/openfortivpn"),
            // Tauri dev mode: resources copied to target/debug/
            concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/openfortivpn"),
            // Packaged deb/rpm (Tauri bundles resources under /usr/lib/{product-name}/)
            "/usr/lib/openfortivpn-connect/openfortivpn",
            "/usr/local/bin/openfortivpn",
            "/usr/bin/openfortivpn",
        ]
    }
}

/// The only binary we allow the helper to execute.
#[cfg(target_os = "macos")]
pub const OPENFORTIVPN_PATH: &str = "/opt/homebrew/bin/openfortivpn";

/// The only binary we allow the helper to execute.
#[cfg(not(target_os = "macos"))]
pub const OPENFORTIVPN_PATH: &str = "/usr/bin/openfortivpn";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_ipv4() {
        assert!(is_valid_ipv4("10.0.0.1"));
        assert!(is_valid_ipv4("192.168.1.1"));
        assert!(is_valid_ipv4("0.0.0.0"));
        assert!(is_valid_ipv4("255.255.255.255"));
    }

    #[test]
    fn test_invalid_ipv4() {
        assert!(!is_valid_ipv4("256.0.0.1"));
        assert!(!is_valid_ipv4("10.0.0"));
        assert!(!is_valid_ipv4("10.0.0.1.2"));
        assert!(!is_valid_ipv4("abc.def.ghi.jkl"));
        assert!(!is_valid_ipv4(""));
    }

    #[test]
    fn test_valid_hostname() {
        assert!(is_valid_hostname("corp.example.com"));
        assert!(is_valid_hostname("my-domain.co"));
        assert!(is_valid_hostname("a"));
    }

    #[test]
    fn test_invalid_hostname() {
        assert!(!is_valid_hostname(""));
        assert!(!is_valid_hostname("-start.com"));
        assert!(!is_valid_hostname(".start.com"));
        assert!(!is_valid_hostname("bad domain.com"));
        assert!(!is_valid_hostname("bad;domain.com"));
    }

    #[test]
    fn test_valid_log_path() {
        assert!(is_valid_log_path("/tmp/openvpngui-abc123.log"));
        assert!(is_valid_log_path(
            "/tmp/openvpngui-550e8400-e29b-41d4-a716-446655440000.log"
        ));
    }

    #[test]
    fn test_invalid_log_path() {
        assert!(!is_valid_log_path("/etc/passwd"));
        assert!(!is_valid_log_path("/tmp/other-file.log"));
        assert!(!is_valid_log_path("/tmp/openvpngui-../../etc/passwd"));
    }

    #[test]
    fn test_validate_vpn_args() {
        assert!(validate_vpn_args(&vec![
            "-u".to_string(), "user".to_string(),
            "-p".to_string(), "pass".to_string(),
            "--trusted-cert=abc".to_string(),
        ]).is_ok());
    }

    #[test]
    fn test_validate_vpn_args_rejects_dangerous() {
        assert!(validate_vpn_args(&vec![
            "--pppd-plugin=/evil".to_string(),
        ]).is_err());
        assert!(validate_vpn_args(&vec![
            "--plugin=/evil".to_string(),
        ]).is_err());
    }
}
