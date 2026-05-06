use std::sync::Mutex;
use std::{path::Path, process::Command};

use tauri::{Manager, State};

use crate::helper_installer;
use crate::models::*;
use crate::settings_store::{AppSettings, SettingsStore};
use crate::vpn_manager::VpnManager;

#[tauri::command]
pub fn get_profiles(manager: State<'_, Mutex<VpnManager>>) -> Result<Vec<VpnProfile>, String> {
    let mgr = manager.lock().map_err(|e| e.to_string())?;
    mgr.get_profiles()
}

#[tauri::command]
pub fn save_profile(
    manager: State<'_, Mutex<VpnManager>>,
    app_handle: tauri::AppHandle,
    profile: VpnProfile,
    password: Option<String>,
    token_pin: Option<String>,
) -> Result<VpnProfile, String> {
    let result = {
        let mgr = manager.lock().map_err(|e| e.to_string())?;
        mgr.save_profile(profile, password)?
    };

    if let Some(pin) = token_pin {
        if pin.is_empty() {
            let _ = crate::keychain::delete_token_pin(&result.id);
        } else {
            crate::keychain::set_token_pin(&result.id, &pin)?;
        }
    }

    crate::tray::refresh_tray_menu(&app_handle);
    Ok(result)
}

#[tauri::command]
pub fn delete_profile(
    manager: State<'_, Mutex<VpnManager>>,
    app_handle: tauri::AppHandle,
    profile_id: String,
) -> Result<(), String> {
    {
        let mgr = manager.lock().map_err(|e| e.to_string())?;
        mgr.delete_profile(&profile_id)?;
    }
    crate::tray::refresh_tray_menu(&app_handle);
    Ok(())
}

#[tauri::command]
pub fn connect(
    manager: State<'_, Mutex<VpnManager>>,
    app_handle: tauri::AppHandle,
    profile_id: String,
    token_pin: Option<String>,
) -> Result<(), String> {
    let settings = SettingsStore::new()
        .and_then(|s| s.get())
        .unwrap_or_default();

    let mut mgr = manager.lock().map_err(|e| e.to_string())?;
    mgr.set_selected_profile(&profile_id);
    mgr.connect(
        &profile_id,
        app_handle,
        settings.debug_mode,
        settings.dns_fallback,
        token_pin,
    )
}

#[tauri::command]
pub fn disconnect(
    manager: State<'_, Mutex<VpnManager>>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    // Set Disconnecting state immediately so the UI updates right away.
    {
        let mut mgr = manager.lock().map_err(|e| e.to_string())?;
        mgr.begin_disconnect(&app_handle);
    }

    // Spawn the heavy kill work in background so this command returns instantly.
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<Mutex<VpnManager>>();
        let mut mgr = state.lock().unwrap();
        mgr.finish_disconnect(&app_handle);
    });

    Ok(())
}

#[tauri::command]
pub fn get_status(
    manager: State<'_, Mutex<VpnManager>>,
) -> Result<ConnectionStatusPayload, String> {
    let mgr = manager.lock().map_err(|e| e.to_string())?;
    Ok(ConnectionStatusPayload::from(mgr.get_state()))
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    let store = SettingsStore::new()?;
    store.get()
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    let store = SettingsStore::new()?;
    store.save(&settings)
}

#[tauri::command]
pub fn check_helper_status() -> Result<helper_installer::HelperStatus, String> {
    Ok(helper_installer::check_status())
}

#[tauri::command]
pub fn install_helper(app_handle: tauri::AppHandle) -> Result<(), String> {
    helper_installer::install(&app_handle)
}

#[tauri::command]
pub fn uninstall_helper() -> Result<(), String> {
    helper_installer::uninstall()
}

#[tauri::command]
pub fn suggest_certificate_tokens() -> Result<Vec<CertificateTokenSuggestion>, String> {
    let mut suggestions = Vec::new();
    let mut scan_notes = Vec::new();

    for provider in pkcs11_provider_candidates() {
        if !Path::new(provider).exists() {
            scan_notes.push(format!("{provider}: provider not found"));
            continue;
        }

        let parsed = match list_provider_tokens(provider) {
            Ok(parsed) => parsed,
            Err(error) => {
                scan_notes.push(format!("{provider}: {error}"));
                continue;
            }
        };

        if parsed.is_empty() {
            scan_notes.push(format!("{provider}: no certificate token found"));
        }

        for suggestion in parsed {
            if !suggestions.iter().any(|existing: &CertificateTokenSuggestion| existing.uri == suggestion.uri) {
                suggestions.push(suggestion);
            }
        }
    }

    if suggestions.is_empty() && !scan_notes.is_empty() {
        return Err(format!(
            "No PKCS#11 certificate token detected. Scan details: {}",
            scan_notes.join(" | ")
        ));
    }

    Ok(suggestions)
}

fn pkcs11_provider_candidates() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &[
            "/usr/lib64/libeToken.so",
            "/usr/lib64/libeTPkcs11.so",
            "/usr/lib64/libIDPrimePKCS11.so",
            "/usr/lib64/pkcs11/libeTPkcs11.so",
            "/usr/lib64/pkcs11/libeToken.so",
            "/usr/lib64/pkcs11/libIDPrimePKCS11.so",
            "/usr/lib64/opensc-pkcs11.so",
            "/usr/lib/pkcs11/libeToken.so",
            "/usr/lib/pkcs11/libeTPkcs11.so",
            "/usr/lib/pkcs11/libIDPrimePKCS11.so",
        ]
    }

    #[cfg(target_os = "macos")]
    {
        &[
            "/usr/local/lib/libetpkcs11.dylib",
            "/usr/local/lib/libeToken.dylib",
            "/Library/OpenSC/lib/opensc-pkcs11.so",
        ]
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        &[]
    }
}

fn list_provider_tokens(provider: &str) -> Result<Vec<CertificateTokenSuggestion>, String> {
    let mut suggestions = list_provider_tokens_direct(provider)?;

    if !suggestions.is_empty() {
        return Ok(suggestions);
    }

    #[cfg(target_os = "linux")]
    {
        suggestions = list_provider_tokens_via_systemd(provider)?;
        if !suggestions.is_empty() {
            return Ok(suggestions);
        }
    }

    Ok(Vec::new())
}

fn list_provider_tokens_direct(provider: &str) -> Result<Vec<CertificateTokenSuggestion>, String> {
    let token_output = Command::new(p11tool_path())
        .args(["--provider", provider, "--list-token-urls"])
        .output()
        .map_err(|e| format!("Failed to execute p11tool: {}", e))?;

    if !token_output.status.success() {
        return Err(format!(
            "p11tool --list-token-urls failed: {}",
            String::from_utf8_lossy(&token_output.stderr).trim()
        ));
    }

    let token_uris = parse_p11tool_token_urls(&String::from_utf8_lossy(&token_output.stdout));
    list_provider_certs(provider, token_uris, false)
}

#[cfg(target_os = "linux")]
fn list_provider_tokens_via_systemd(provider: &str) -> Result<Vec<CertificateTokenSuggestion>, String> {
    let token_output = Command::new("systemd-run")
        .args([
            "--user",
            "--wait",
            "--pipe",
            p11tool_path(),
            "--provider",
            provider,
            "--list-token-urls",
        ])
        .output()
        .map_err(|e| format!("Failed to execute systemd-run p11tool: {}", e))?;

    let token_uris = parse_p11tool_token_urls(&String::from_utf8_lossy(&token_output.stdout));
    list_provider_certs(provider, token_uris, true)
}

#[cfg(not(target_os = "linux"))]
fn list_provider_tokens_via_systemd(_provider: &str) -> Result<Vec<CertificateTokenSuggestion>, String> {
    Ok(Vec::new())
}

fn list_provider_certs(
    provider: &str,
    token_uris: Vec<String>,
    via_systemd: bool,
) -> Result<Vec<CertificateTokenSuggestion>, String> {
    let mut suggestions = Vec::new();

    for token_uri in token_uris {
        let output = if via_systemd {
            Command::new("systemd-run")
                .args([
                    "--user",
                    "--wait",
                    "--pipe",
                    p11tool_path(),
                    "--provider",
                    provider,
                    "--list-all-certs",
                    &token_uri,
                ])
                .output()
                .map_err(|e| format!("Failed to execute systemd-run p11tool: {}", e))?
        } else {
            Command::new(p11tool_path())
                .args(["--provider", provider, "--list-all-certs", &token_uri])
                .output()
                .map_err(|e| format!("Failed to execute p11tool: {}", e))?
        };

        if !output.status.success() {
            return Err(format!(
                "p11tool --list-all-certs failed for {token_uri}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        suggestions.extend(parse_p11tool_certificates(&stdout, provider));
    }

    Ok(suggestions)
}

fn parse_p11tool_token_urls(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pkcs11:"))
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_p11tool_certificates(stdout: &str, provider: &str) -> Vec<CertificateTokenSuggestion> {
    let mut suggestions = Vec::new();
    let mut current_url: Option<String> = None;
    let mut current_label: Option<String> = None;
    let mut current_id: Option<String> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Object ") {
            push_certificate_suggestion(&mut suggestions, provider, current_url.take(), current_label.take(), current_id.take());
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("URL:") {
            current_url = Some(value.trim().to_string());
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("Label:") {
            current_label = Some(value.trim().to_string());
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("ID:") {
            current_id = Some(value.trim().to_string());
        }
    }

    push_certificate_suggestion(&mut suggestions, provider, current_url, current_label, current_id);
    suggestions
}

fn push_certificate_suggestion(
    suggestions: &mut Vec<CertificateTokenSuggestion>,
    provider: &str,
    cert_url: Option<String>,
    label: Option<String>,
    id: Option<String>,
) {
    let Some(cert_url) = cert_url else {
        return;
    };

    let connect_uri = normalize_pkcs11_cert_url(&cert_url);
    let display_name = match (label.as_deref(), id.as_deref()) {
        (Some(label), Some(id)) if !label.is_empty() && !id.is_empty() => {
            format!("{} [{}]", label, id)
        }
        (Some(label), _) if !label.is_empty() => label.to_string(),
        _ => connect_uri.clone(),
    };

    suggestions.push(CertificateTokenSuggestion {
        uri: connect_uri,
        display_name,
        provider: provider.to_string(),
    });
}

fn normalize_pkcs11_cert_url(cert_url: &str) -> String {
    cert_url
        .split(';')
        .filter(|part| !part.starts_with("object=") && !part.starts_with("type="))
        .collect::<Vec<_>>()
        .join(";")
}

fn p11tool_path() -> &'static str {
    if Path::new("/usr/bin/p11tool").exists() {
        "/usr/bin/p11tool"
    } else {
        "p11tool"
    }
}
