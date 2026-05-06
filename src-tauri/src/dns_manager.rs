use std::process::Command;

/// Configure macOS DNS via scutil for the VPN interface.
/// This is necessary because macOS ignores /etc/resolv.conf and uses
/// the SystemConfiguration framework instead. openfortivpn only writes
/// to resolv.conf, which doesn't affect macOS DNS resolution.
#[cfg(target_os = "macos")]
pub fn setup_dns(dns_servers: &[String], dns_suffixes: &[String]) -> Result<(), String> {
    if dns_servers.is_empty() {
        return Ok(());
    }

    log::info!(
        "Setting up macOS DNS with servers: {} suffixes: {:?}",
        dns_servers.join(", "),
        dns_suffixes
    );

    match crate::helper_client::setup_dns(dns_servers, dns_suffixes) {
        Ok(()) => {
            log::info!("DNS configured via helper daemon");
            return Ok(());
        }
        Err(e) if crate::helper_client::is_connection_error(&e) => {
            log::info!("Helper unavailable ({}), configuring DNS via osascript", e);
        }
        Err(e) => return Err(e),
    }
    setup_dns_osascript(dns_servers, dns_suffixes)
}

#[cfg(not(target_os = "macos"))]
pub fn setup_dns(dns_servers: &[String], dns_suffixes: &[String]) -> Result<(), String> {
    if dns_servers.is_empty() {
        return Ok(());
    }

    match crate::helper_client::setup_dns(dns_servers, dns_suffixes) {
        Ok(()) => {
            log::info!("Linux DNS configured via privileged helper");
            return Ok(());
        }
        Err(e) if crate::helper_client::is_connection_error(&e) => {
            log::info!("Helper unavailable ({}), configuring DNS directly", e);
        }
        Err(e) => return Err(e),
    }

    let Some(interface) = vpn_dns_interface() else {
        return Err("No PPP interface found for VPN DNS setup".to_string());
    };

    log::info!(
        "Setting up Linux DNS on {} with servers: {} suffixes: {:?}",
        interface,
        dns_servers.join(", "),
        dns_suffixes
    );

    run_resolvectl(&["dns", &interface], dns_servers)?;

    let route_domains: Vec<String> = dns_suffixes
        .iter()
        .map(|suffix| {
            if suffix.starts_with('~') {
                suffix.clone()
            } else {
                format!("~{suffix}")
            }
        })
        .collect();

    if !route_domains.is_empty() {
        run_resolvectl(&["domain", &interface], &route_domains)?;
    }

    run_resolvectl(&["default-route", &interface, "false"], &[])?;
    run_resolvectl(&["flush-caches"], &[])?;

    log::info!("Linux DNS configured successfully on {}", interface);
    Ok(())
}

#[cfg(target_os = "macos")]
fn setup_dns_osascript(dns_servers: &[String], dns_suffixes: &[String]) -> Result<(), String> {
    let servers_str = dns_servers.join(" ");

    let scutil_input = if dns_suffixes.is_empty() {
        format!(
            "d.init\n\
             d.add ServerAddresses * {servers}\n\
             d.add SupplementalMatchDomains * \"\"\n\
             set State:/Network/Service/OpenFortiVPN/DNS\n\
             quit\n",
            servers = servers_str,
        )
    } else {
        let domains = dns_suffixes.join(" ");
        format!(
            "d.init\n\
             d.add ServerAddresses * {servers}\n\
             d.add SupplementalMatchDomains * {domains}\n\
             d.add SearchDomains * {domains}\n\
             set State:/Network/Service/OpenFortiVPN/DNS\n\
             quit\n",
            servers = servers_str,
            domains = domains,
        )
    };

    let output = Command::new("osascript")
        .args([
            "-e",
            &format!(
                "do shell script \"echo '{}' | /usr/sbin/scutil\" with administrator privileges",
                applescript_escape_inner(&scutil_input)
            ),
        ])
        .output()
        .map_err(|e| format!("Failed to configure DNS: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("scutil DNS setup failed: {}", stderr));
    }

    log::info!("macOS DNS configured successfully");
    Ok(())
}

/// Remove the VPN DNS configuration from macOS
#[cfg(target_os = "macos")]
pub fn teardown_dns() -> Result<(), String> {
    log::info!("Tearing down macOS DNS configuration");

    match crate::helper_client::teardown_dns() {
        Ok(()) => {
            log::info!("DNS torn down via helper daemon");
            return Ok(());
        }
        Err(e) if crate::helper_client::is_connection_error(&e) => {
            log::info!("Helper unavailable ({}), tearing down DNS via osascript", e);
        }
        Err(e) => return Err(e),
    }
    teardown_dns_osascript()
}

#[cfg(not(target_os = "macos"))]
pub fn teardown_dns() -> Result<(), String> {
    match crate::helper_client::teardown_dns() {
        Ok(()) => {
            log::info!("Linux DNS torn down via privileged helper");
            return Ok(());
        }
        Err(e) if crate::helper_client::is_connection_error(&e) => {}
        Err(e) => return Err(e),
    }

    for interface in ppp_interfaces() {
        let _ = run_resolvectl(&["revert", &interface], &[]);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn vpn_dns_interface() -> Option<String> {
    if std::path::Path::new("/sys/class/net/ppp0").exists() {
        return Some("ppp0".to_string());
    }

    ppp_interfaces().into_iter().next()
}

#[cfg(not(target_os = "macos"))]
fn ppp_interfaces() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("ppp"))
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn run_resolvectl(prefix: &[&str], values: &[String]) -> Result<(), String> {
    let mut command = Command::new("resolvectl");
    command.args(prefix).args(values);

    let output = command
        .output()
        .map_err(|e| format!("Failed to execute resolvectl: {}", e))?;

    if output.status.success() {
        return Ok(());
    }

    Err(format!(
        "resolvectl {} failed: {}",
        prefix.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(target_os = "macos")]
fn teardown_dns_osascript() -> Result<(), String> {
    let scutil_input = "remove State:/Network/Service/OpenFortiVPN/DNS\nquit\n";

    let output = Command::new("osascript")
        .args([
            "-e",
            &format!(
                "do shell script \"echo '{}' | /usr/sbin/scutil\" with administrator privileges",
                applescript_escape_inner(scutil_input)
            ),
        ])
        .output()
        .map_err(|e| format!("Failed to teardown DNS: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("User canceled") {
            log::warn!("scutil DNS teardown returned error: {}", stderr);
        }
    }

    log::info!("macOS DNS configuration removed");
    Ok(())
}

/// Capture the current DNS servers configured on the system (from DHCP/manual).
/// Parses `scutil --dns` output to find the primary resolver's nameservers.
#[cfg(target_os = "macos")]
pub fn get_current_dns_servers() -> Vec<String> {
    let output = match Command::new("scutil").args(["--dns"]).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut servers = Vec::new();
    let mut in_first_resolver = false;

    for line in stdout.lines() {
        let trimmed = line.trim();
        // Only parse the first resolver block (primary DHCP resolver)
        if trimmed.starts_with("resolver #1") {
            in_first_resolver = true;
            continue;
        }
        if in_first_resolver && trimmed.starts_with("resolver #") {
            break;
        }
        if in_first_resolver && trimmed.starts_with("nameserver[") {
            // Format: "nameserver[0] : 1.1.1.1"
            if let Some(ip) = trimmed.split(':').nth(1) {
                let ip = ip.trim();
                let parts: Vec<&str> = ip.split('.').collect();
                if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
                    if !servers.contains(&ip.to_string()) {
                        servers.push(ip.to_string());
                    }
                }
            }
        }
    }

    log::info!("Current system DNS servers: {:?}", servers);
    servers
}

/// Capture the current DNS servers configured on the system from resolv.conf.
#[cfg(not(target_os = "macos"))]
pub fn get_current_dns_servers() -> Vec<String> {
    let contents = match std::fs::read_to_string("/etc/resolv.conf") {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    let mut servers = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(ip) = trimmed.strip_prefix("nameserver ") {
            let ip = ip.trim();
            let parts: Vec<&str> = ip.split('.').collect();
            if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
                let candidate = ip.to_string();
                if !servers.contains(&candidate) {
                    servers.push(candidate);
                }
            }
        }
    }
    servers
}

/// Parse DNS servers from openfortivpn log output.
/// With -v flag, openfortivpn logs:
///   "Found dns server 10.0.0.1 in xml config"
///   "Found dns suffix corp.example.com in xml config"
///   "Found dns suffix a.example.com;b.example.com;c.example.com in xml config"
///
/// The FortiGate may concatenate multiple DNS suffixes in a single line, separated
/// by ';' (or sometimes ','). We split them so each suffix becomes a search domain.
pub fn parse_dns_from_log(line: &str) -> Option<DnsInfo> {
    let trimmed = line.trim();

    // Match "Found dns server X.X.X.X in xml config"
    if trimmed.contains("Found dns server") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        for (i, part) in parts.iter().enumerate() {
            if *part == "server" && i + 1 < parts.len() {
                let ip = parts[i + 1];
                let octets: Vec<&str> = ip.split('.').collect();
                if octets.len() == 4 && octets.iter().all(|o| o.parse::<u8>().is_ok()) {
                    return Some(DnsInfo::Server(ip.to_string()));
                }
            }
        }
    }

    // Match "Found dns suffix example.com in xml config" (possibly with multiple
    // suffixes separated by ';' or ',')
    if trimmed.contains("Found dns suffix") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        for (i, part) in parts.iter().enumerate() {
            if *part == "suffix" && i + 1 < parts.len() {
                let compound = parts[i + 1];
                let domains: Vec<String> = compound
                    .split(|c: char| c == ';' || c == ',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| s.contains('.'))
                    .collect();
                if !domains.is_empty() {
                    return Some(DnsInfo::SearchDomains(domains));
                }
            }
        }
    }

    None
}

#[derive(Debug, Clone)]
pub enum DnsInfo {
    Server(String),
    SearchDomains(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_dns_suffix() {
        let line = "[2026-04-11T12:41:23.189550+00:00] DEBUG:  Found dns suffix corp.example.com in xml config";
        match parse_dns_from_log(line) {
            Some(DnsInfo::SearchDomains(domains)) => {
                assert_eq!(domains, vec!["corp.example.com".to_string()]);
            }
            other => panic!("expected SearchDomains, got {:?}", other),
        }
    }

    #[test]
    fn parse_multiple_dns_suffixes_semicolon() {
        let line = "[2026-04-11T12:41:23.189550+00:00] DEBUG:  Found dns suffix redecamara.camara.gov.br;camara.leg.br;camara.gov.br in xml config";
        match parse_dns_from_log(line) {
            Some(DnsInfo::SearchDomains(domains)) => {
                assert_eq!(
                    domains,
                    vec![
                        "redecamara.camara.gov.br".to_string(),
                        "camara.leg.br".to_string(),
                        "camara.gov.br".to_string(),
                    ]
                );
            }
            other => panic!("expected SearchDomains, got {:?}", other),
        }
    }

    #[test]
    fn parse_multiple_dns_suffixes_comma() {
        let line = "DEBUG: Found dns suffix a.com,b.com in xml config";
        match parse_dns_from_log(line) {
            Some(DnsInfo::SearchDomains(domains)) => {
                assert_eq!(domains, vec!["a.com".to_string(), "b.com".to_string()]);
            }
            other => panic!("expected SearchDomains, got {:?}", other),
        }
    }

    #[test]
    fn parse_dns_server() {
        let line = "[2026-04-11T12:41:23.189573+00:00] DEBUG:  Found dns server 10.1.3.6 in xml config";
        match parse_dns_from_log(line) {
            Some(DnsInfo::Server(ip)) => assert_eq!(ip, "10.1.3.6"),
            other => panic!("expected Server, got {:?}", other),
        }
    }

    #[test]
    fn parse_irrelevant_line_returns_none() {
        let line = "[2026-04-11T12:41:23.190413+00:00] Sat Apr 11 09:41:23 2026 : Using interface ppp16";
        assert!(parse_dns_from_log(line).is_none());
    }
}

/// Escape for use inside a single-quoted AppleScript string that's inside a double-quoted shell string.
/// We need to handle single quotes in the scutil input and also escape for the AppleScript layer.
#[cfg(target_os = "macos")]
fn applescript_escape_inner(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "'\\''")
        .replace('\n', "\\n")
}
