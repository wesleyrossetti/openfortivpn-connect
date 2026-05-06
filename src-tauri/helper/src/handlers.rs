use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::protocol::{Request, Response};
use crate::validation;

pub fn handle(request: Request) -> Response {
    match request {
        Request::Ping => handle_ping(),
        Request::SpawnVpn {
            args,
            log_path,
            binary_path,
            env_vars,
        } => handle_spawn_vpn(args, log_path, binary_path, env_vars),
        Request::KillVpn { pid, gateway } => handle_kill_vpn(pid, gateway),
        Request::SetupDns { servers, suffixes } => handle_setup_dns(servers, suffixes),
        Request::TeardownDns => handle_teardown_dns(),
    }
}

fn handle_ping() -> Response {
    Response::with_version(env!("CARGO_PKG_VERSION").to_string())
}

fn handle_spawn_vpn(
    args: Vec<String>,
    log_path: String,
    binary_path: Option<String>,
    env_vars: Vec<(String, String)>,
) -> Response {
    // Validate args
    if let Err(e) = validation::validate_vpn_args(&args) {
        return Response::error(e);
    }

    // Validate log path
    if !validation::is_valid_log_path(&log_path) {
        return Response::error(format!("Invalid log path: {}", log_path));
    }

    if let Err(e) = validation::validate_env_vars(&env_vars) {
        return Response::error(e);
    }

    let binary_path = match binary_path {
        Some(path) => {
            if !validation::is_allowed_openfortivpn_path(&path) {
                return Response::error(format!("Invalid openfortivpn path: {}", path));
            }
            path
        }
        None => validation::openfortivpn_path().to_string(),
    };

    // Open log file for appending.
    // If the file already exists with unexpected ownership/permissions, recreate it.
    let log_file = match open_log_file(&log_path) {
        Ok(f) => f,
        Err(e) => return Response::error(format!("Failed to open log file: {}", e)),
    };

    let log_file_stderr = match log_file.try_clone() {
        Ok(f) => f,
        Err(e) => return Response::error(format!("Failed to clone log file handle: {}", e)),
    };

    // Spawn openfortivpn directly (no shell, no quoting needed)
    match Command::new(&binary_path)
        .envs(env_vars)
        .args(&args)
        .stdout(log_file)
        .stderr(log_file_stderr)
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            log::info!("Spawned openfortivpn with PID {}", pid);
            Response::with_pid(pid)
        }
        Err(e) => Response::error(format!("Failed to spawn openfortivpn: {}", e)),
    }
}

fn open_log_file(log_path: &str) -> std::io::Result<std::fs::File> {
    match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(file) => {
            let _ = fs::set_permissions(log_path, fs::Permissions::from_mode(0o644));
            Ok(file)
        }
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            let _ = fs::remove_file(log_path);
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(log_path)?;
            let _ = fs::set_permissions(log_path, fs::Permissions::from_mode(0o644));
            Ok(file)
        }
        Err(err) => Err(err),
    }
}

fn handle_kill_vpn(pid: u32, gateway: Option<String>) -> Response {
    // Validate gateway if provided
    if let Some(ref gw) = gateway {
        if !validation::is_valid_gateway(gw) {
            return Response::error(format!("Invalid gateway: {}", gw));
        }
    }

    // Validate that the PID is actually openfortivpn
    if !validation::is_openfortivpn_pid(pid) {
        return Response::error(format!(
            "PID {} is not an openfortivpn process",
            pid
        ));
    }

    log::info!("Killing openfortivpn PID {}", pid);

    // 1. SIGINT for clean shutdown
    let _ = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .output();

    // 2. Wait, then SIGKILL if still alive
    std::thread::sleep(std::time::Duration::from_secs(2));
    let still_alive = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if still_alive {
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }

    // 3. Kill orphaned pppd
    let _ = Command::new("killall")
        .args(["pppd"])
        .output();

    std::thread::sleep(std::time::Duration::from_secs(1));

    // 4. Bring down ppp interfaces
    let _ = Command::new("ifconfig")
        .args(["ppp0", "down"])
        .output();
    let _ = Command::new("ifconfig")
        .args(["ppp1", "down"])
        .output();

    // 5. Restore original default route
    if let Some(ref gw) = gateway {
        let _ = Command::new("/sbin/route")
            .args(["delete", "default"])
            .output();
        let _ = Command::new("/sbin/route")
            .args(["add", "default", gw])
            .output();
    }

    // 6. Remove VPN DNS config
    #[cfg(target_os = "linux")]
    {
        for iface in ppp_interfaces() {
            let _ = Command::new("resolvectl").args(["revert", &iface]).output();
        }
        let _ = Command::new("resolvectl").arg("flush-caches").output();
    }

    #[cfg(target_os = "macos")]
    let _ = Command::new("/usr/sbin/scutil")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(b"remove State:/Network/Service/OpenFortiVPN/DNS\nquit\n")?;
            }
            child.wait()
        });

    // 7. Flush DNS cache
    #[cfg(target_os = "macos")]
    let _ = Command::new("/usr/bin/dscacheutil")
        .args(["-flushcache"])
        .output();
    #[cfg(target_os = "macos")]
    let _ = Command::new("/usr/bin/killall")
        .args(["-HUP", "mDNSResponder"])
        .output();

    log::info!("VPN cleanup complete for PID {}", pid);
    Response::success()
}

fn handle_setup_dns(servers: Vec<String>, suffixes: Vec<String>) -> Response {
    // Validate servers
    if servers.is_empty() {
        return Response::error("No DNS servers provided".to_string());
    }
    for server in &servers {
        if !validation::is_valid_ipv4(server) {
            return Response::error(format!("Invalid DNS server IP: {}", server));
        }
    }

    // Validate each suffix individually (rejecting e.g. strings containing ';')
    for s in &suffixes {
        if !validation::is_valid_hostname(s) {
            return Response::error(format!("Invalid DNS suffix: {}", s));
        }
    }

    #[cfg(target_os = "linux")]
    {
        return handle_setup_dns_linux(servers, suffixes);
    }

    #[cfg(target_os = "macos")]
    let servers_str = servers.join(" ");

    #[cfg(target_os = "macos")]
    let scutil_input = if suffixes.is_empty() {
        format!(
            "d.init\n\
             d.add ServerAddresses * {servers}\n\
             d.add SupplementalMatchDomains * \"\"\n\
             set State:/Network/Service/OpenFortiVPN/DNS\n\
             quit\n",
            servers = servers_str,
        )
    } else {
        let domains = suffixes.join(" ");
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

    #[cfg(target_os = "macos")]
    log::info!(
        "Setting up DNS with servers: {} suffixes: {:?}",
        servers_str,
        suffixes
    );

    #[cfg(target_os = "macos")]
    let result = Command::new("/usr/sbin/scutil")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(scutil_input.as_bytes())?;
                // stdin is dropped here, closing the pipe so scutil processes input
            }
            child.wait()
        });

    #[cfg(target_os = "macos")]
    match result {
        Ok(status) if status.success() => {
            log::info!("DNS configured successfully");
            Response::success()
        }
        Ok(status) => Response::error(format!("scutil exited with status: {}", status)),
        Err(e) => Response::error(format!("Failed to run scutil: {}", e)),
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Response::error("DNS setup is not supported on this platform".to_string())
}

fn handle_teardown_dns() -> Response {
    log::info!("Tearing down DNS configuration");

    #[cfg(target_os = "linux")]
    {
        for iface in ppp_interfaces() {
            let _ = Command::new("resolvectl").args(["revert", &iface]).output();
        }
        let _ = Command::new("resolvectl").arg("flush-caches").output();
        return Response::success();
    }

    #[cfg(target_os = "macos")]
    let result = Command::new("/usr/sbin/scutil")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(b"remove State:/Network/Service/OpenFortiVPN/DNS\nquit\n")?;
            }
            child.wait()
        });

    #[cfg(target_os = "macos")]
    match result {
        Ok(_) => {
            log::info!("DNS configuration removed");
            Response::success()
        }
        Err(e) => Response::error(format!("Failed to teardown DNS: {}", e)),
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Response::error("DNS teardown is not supported on this platform".to_string())
}

#[cfg(target_os = "linux")]
fn handle_setup_dns_linux(servers: Vec<String>, suffixes: Vec<String>) -> Response {
    let Some(interface) = vpn_dns_interface() else {
        return Response::error("No PPP interface found for VPN DNS setup".to_string());
    };

    log::info!(
        "Setting up Linux DNS on {} with servers: {} suffixes: {:?}",
        interface,
        servers.join(", "),
        suffixes
    );

    if let Err(e) = run_resolvectl(&["dns", &interface], &servers) {
        return Response::error(e);
    }

    let route_domains: Vec<String> = suffixes
        .into_iter()
        .map(|suffix| {
            if suffix.starts_with('~') {
                suffix
            } else {
                format!("~{suffix}")
            }
        })
        .collect();

    if !route_domains.is_empty() {
        if let Err(e) = run_resolvectl(&["domain", &interface], &route_domains) {
            return Response::error(e);
        }
    }

    if let Err(e) = run_resolvectl(&["default-route", &interface, "false"], &[]) {
        return Response::error(e);
    }

    if let Err(e) = run_resolvectl(&["flush-caches"], &[]) {
        return Response::error(e);
    }

    Response::success()
}

#[cfg(target_os = "linux")]
fn vpn_dns_interface() -> Option<String> {
    if std::path::Path::new("/sys/class/net/ppp0").exists() {
        return Some("ppp0".to_string());
    }

    ppp_interfaces().into_iter().next()
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn run_resolvectl(prefix: &[&str], values: &[String]) -> Result<(), String> {
    let output = Command::new("resolvectl")
        .args(prefix)
        .args(values)
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
