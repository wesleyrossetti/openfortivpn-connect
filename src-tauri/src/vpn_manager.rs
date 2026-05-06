use tauri::{AppHandle, Emitter};

use crate::keychain;
use crate::models::*;
use crate::process_manager::ProcessManager;
use crate::profile_store::ProfileStore;

pub struct VpnManager {
    state: ConnectionState,
    profile_store: ProfileStore,
    process_manager: ProcessManager,
    selected_profile_id: Option<String>,
}

impl VpnManager {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            state: ConnectionState::Disconnected,
            profile_store: ProfileStore::new()?,
            process_manager: ProcessManager::new(),
            selected_profile_id: None,
        })
    }

    pub fn get_state(&self) -> &ConnectionState {
        &self.state
    }

    pub fn selected_profile_id(&self) -> Option<&str> {
        self.selected_profile_id.as_deref()
    }

    pub fn set_selected_profile(&mut self, id: &str) {
        self.selected_profile_id = Some(id.to_string());
    }

    /// Update internal state from an event payload (used by the tray listener
    /// to sync state changes emitted by the log monitor).
    pub fn sync_state_from_payload(&mut self, payload: &ConnectionStatusPayload) {
        let new_state = match payload.state.as_str() {
            "Disconnected" => ConnectionState::Disconnected,
            "Connecting" => ConnectionState::Connecting {
                profile_id: payload.profile_id.clone().unwrap_or_default(),
            },
            "WaitingSaml" => ConnectionState::WaitingSaml {
                profile_id: payload.profile_id.clone().unwrap_or_default(),
                url: payload.message.clone().unwrap_or_default(),
            },
            "Connected" => ConnectionState::Connected {
                profile_id: payload.profile_id.clone().unwrap_or_default(),
                ip: payload.ip.clone().unwrap_or_else(|| "unknown".into()),
                since: payload
                    .since
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now),
            },
            "Disconnecting" => ConnectionState::Disconnecting,
            "Error" => ConnectionState::Error {
                message: payload.message.clone().unwrap_or_default(),
            },
            _ => return,
        };
        self.state = new_state;
    }

    fn set_state(&mut self, state: ConnectionState, app_handle: &AppHandle) {
        self.state = state;
        let payload = ConnectionStatusPayload::from(&self.state);
        let _ = app_handle.emit("connection-status-changed", &payload);
        // NOTE: Do NOT call refresh_tray_menu here — we may be inside a Mutex lock.
        // Tray refresh is driven by listening to the "connection-status-changed" event.
        // update_tray_icon is safe here — it doesn't access the Mutex.
        crate::tray::update_tray_icon(app_handle, &self.state);
    }

    pub fn connect(
        &mut self,
        profile_id: &str,
        app_handle: AppHandle,
        debug_mode: bool,
        dns_fallback: bool,
        token_pin: Option<String>,
    ) -> Result<(), String> {
        // Validate state
        match &self.state {
            ConnectionState::Disconnected | ConnectionState::Error { .. } => {}
            _ => return Err("Already connecting or connected".to_string()),
        }

        // Look up profile
        let profile = self
            .profile_store
            .get_by_id(profile_id)?
            .ok_or_else(|| format!("Profile '{}' not found", profile_id))?;

        // Set connecting state
        self.set_state(
            ConnectionState::Connecting {
                profile_id: profile_id.to_string(),
            },
            &app_handle,
        );

        // Build args
        let mut args = vec![format!("{}:{}", profile.host, profile.port)];
        let mut pkcs11_provider = None;

        match profile.auth_type {
            AuthType::Password => {
                if let Some(ref username) = profile.username {
                    args.push("-u".to_string());
                    args.push(username.clone());
                }
                if let Ok(Some(password)) = keychain::get_password(&profile.id) {
                    args.push("-p".to_string());
                    args.push(password);
                }
            }
            AuthType::Saml => {
                args.push("--saml-login".to_string());
            }
            AuthType::CertificateToken => {
                if let Some(ref username) = profile.username {
                    if !username.trim().is_empty() {
                        args.push("-u".to_string());
                        args.push(username.clone());
                    }
                }
                if let Some(ref user_cert) = profile.user_cert {
                    if !user_cert.trim().is_empty() {
                        let mut cert_uri = user_cert.trim().to_string();
                        if cert_uri.starts_with("pkcs11:") {
                            if !cert_uri.contains("pin-value=") {
                                let runtime_pin = token_pin
                                    .clone()
                                    .filter(|pin| !pin.trim().is_empty());
                                let stored_pin = if runtime_pin.is_none() {
                                    keychain::get_token_pin(&profile.id)?
                                } else {
                                    None
                                };
                                let token_pin = runtime_pin
                                    .or(stored_pin)
                                    .ok_or_else(|| "Token PIN is required for certificate token authentication".to_string())?;
                                cert_uri = append_pkcs11_pin_value(&cert_uri, &token_pin);
                            }
                            pkcs11_provider = profile.pkcs11_provider.clone().or_else(default_pkcs11_provider);
                        }
                        args.push(format!("--user-cert={}", cert_uri));
                    } else {
                        return Err("Certificate/token URI is required for certificate token authentication".to_string());
                    }
                } else {
                    return Err("Certificate/token URI is required for certificate token authentication".to_string());
                }
            }
        }

        for cert in &profile.trusted_certs {
            args.push(format!("--trusted-cert={}", cert));
        }

        if let Some(ref realm) = profile.realm {
            args.push(format!("--realm={}", realm));
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // DNS is managed by the app: scutil on macOS and systemd-resolved on Linux.
            args.push("--set-dns=0".to_string());
            args.push("--pppd-use-peerdns=0".to_string());
        }

        // Enable verbose logging so we can capture DNS server info from debug output
        args.push("-v".to_string());

        args.extend(profile.extra_args.clone());

        // Spawn the process
        match self
            .process_manager
            .spawn_vpn(args, profile_id.to_string(), app_handle.clone(), debug_mode, dns_fallback, pkcs11_provider)
        {
            Ok(()) => Ok(()),
            Err(e) => {
                self.set_state(ConnectionState::Error { message: e.clone() }, &app_handle);
                Err(e)
            }
        }
    }

    /// Begin disconnecting: sets state to Disconnecting and returns immediately.
    pub fn begin_disconnect(&mut self, app_handle: &AppHandle) {
        self.set_state(ConnectionState::Disconnecting, app_handle);
    }

    /// Perform the heavy disconnect work (kill process, restore routes, DNS).
    pub fn finish_disconnect(&mut self, app_handle: &AppHandle) {
        match self.process_manager.kill_vpn() {
            Ok(()) => {
                self.set_state(ConnectionState::Disconnected, app_handle);
            }
            Err(e) => {
                self.set_state(ConnectionState::Error { message: e.clone() }, app_handle);
            }
        }
    }

    /// Full synchronous disconnect (used by tray menu).
    pub fn disconnect(&mut self, app_handle: AppHandle) -> Result<(), String> {
        self.begin_disconnect(&app_handle);
        self.finish_disconnect(&app_handle);
        Ok(())
    }

    pub fn get_profiles(&self) -> Result<Vec<VpnProfile>, String> {
        self.profile_store.get_all()
    }

    pub fn save_profile(
        &self,
        profile: VpnProfile,
        password: Option<String>,
    ) -> Result<VpnProfile, String> {
        let saved = self.profile_store.upsert(profile)?;
        if let Some(pwd) = password {
            if !pwd.is_empty() {
                keychain::set_password(&saved.id, &pwd)?;
            }
        }
        Ok(saved)
    }

    pub fn delete_profile(&self, profile_id: &str) -> Result<(), String> {
        self.profile_store.delete(profile_id)?;
        let _ = keychain::delete_password(profile_id);
        Ok(())
    }
}

fn append_pkcs11_pin_value(uri: &str, token_pin: &str) -> String {
    let separator = if uri.contains('?') { '&' } else { ';' };
    format!("{uri}{separator}pin-value={}", encode_pkcs11_component(token_pin))
}

fn encode_pkcs11_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn default_pkcs11_provider() -> Option<String> {
    [
        "/usr/lib64/libeToken.so",
        "/usr/lib64/libeTPkcs11.so",
        "/usr/lib64/libIDPrimePKCS11.so",
        "/usr/lib64/opensc-pkcs11.so",
        "/usr/lib/pkcs11/libeToken.so",
        "/usr/lib/pkcs11/libIDPrimePKCS11.so",
    ]
    .iter()
    .find(|path| std::path::Path::new(path).exists())
    .map(|path| path.to_string())
}
