const SERVICE: &str = "com.openvpngui.app";

fn account_for(kind: &str, profile_id: &str) -> String {
    format!("{}-{}", kind, profile_id)
}

#[cfg(target_os = "macos")]
fn set_secret(profile_id: &str, kind: &str, secret: &str) -> Result<(), String> {
    use security_framework::passwords::{
        delete_generic_password, set_generic_password,
    };

    let account = account_for(kind, profile_id);
    // Delete first to avoid "duplicate item" errors
    let _ = delete_generic_password(SERVICE, &account);
    set_generic_password(SERVICE, &account, secret.as_bytes())
        .map_err(|e| format!("Failed to save password to Keychain: {}", e))
}

#[cfg(target_os = "macos")]
fn get_secret(profile_id: &str, kind: &str) -> Result<Option<String>, String> {
    use security_framework::passwords::get_generic_password;

    let account = account_for(kind, profile_id);
    match get_generic_password(SERVICE, &account) {
        Ok(bytes) => {
            let password = String::from_utf8(bytes.to_vec())
                .map_err(|e| format!("Password is not valid UTF-8: {}", e))?;
            Ok(Some(password))
        }
        Err(e) => {
            // errSecItemNotFound = -25300
            if e.code() == -25300 {
                Ok(None)
            } else {
                Err(format!("Failed to retrieve password from Keychain: {}", e))
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn delete_secret(profile_id: &str, kind: &str) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password;

    let account = account_for(kind, profile_id);
    match delete_generic_password(SERVICE, &account) {
        Ok(()) => Ok(()),
        Err(e) => {
            if e.code() == -25300 {
                Ok(()) // Not found is ok
            } else {
                Err(format!("Failed to delete password from Keychain: {}", e))
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn set_password(profile_id: &str, password: &str) -> Result<(), String> {
    set_secret(profile_id, "vpn", password)
}

#[cfg(target_os = "macos")]
pub fn get_password(profile_id: &str) -> Result<Option<String>, String> {
    get_secret(profile_id, "vpn")
}

#[cfg(target_os = "macos")]
pub fn delete_password(profile_id: &str) -> Result<(), String> {
    delete_secret(profile_id, "vpn")
}

#[cfg(target_os = "macos")]
pub fn set_token_pin(profile_id: &str, pin: &str) -> Result<(), String> {
    set_secret(profile_id, "token-pin", pin)
}

#[cfg(target_os = "macos")]
pub fn get_token_pin(profile_id: &str) -> Result<Option<String>, String> {
    get_secret(profile_id, "token-pin")
}

#[cfg(target_os = "macos")]
pub fn delete_token_pin(profile_id: &str) -> Result<(), String> {
    delete_secret(profile_id, "token-pin")
}

#[cfg(not(target_os = "macos"))]
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct PasswordStore {
    entries: std::collections::BTreeMap<String, String>,
}

#[cfg(not(target_os = "macos"))]
fn password_store_path() -> Result<std::path::PathBuf, String> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| "Failed to resolve config directory".to_string())?
        .join("openfortivpn-connect");

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    Ok(config_dir.join("passwords.json"))
}

#[cfg(not(target_os = "macos"))]
fn load_password_store() -> Result<PasswordStore, String> {
    let path = password_store_path()?;
    if !path.exists() {
        return Ok(PasswordStore::default());
    }

    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read password store: {}", e))?;
    serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse password store: {}", e))
}

#[cfg(not(target_os = "macos"))]
fn save_password_store(store: &PasswordStore) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let path = password_store_path()?;
    let contents = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize password store: {}", e))?;

    std::fs::write(&path, contents)
        .map_err(|e| format!("Failed to write password store: {}", e))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("Failed to set password store permissions: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn set_secret(profile_id: &str, kind: &str, secret: &str) -> Result<(), String> {
    let mut store = load_password_store()?;
    store
        .entries
        .insert(account_for(kind, profile_id), secret.to_string());
    save_password_store(&store)
}

#[cfg(not(target_os = "macos"))]
fn get_secret(profile_id: &str, kind: &str) -> Result<Option<String>, String> {
    let store = load_password_store()?;
    Ok(store.entries.get(&account_for(kind, profile_id)).cloned())
}

#[cfg(not(target_os = "macos"))]
fn delete_secret(profile_id: &str, kind: &str) -> Result<(), String> {
    let mut store = load_password_store()?;
    store.entries.remove(&account_for(kind, profile_id));
    save_password_store(&store)
}

#[cfg(not(target_os = "macos"))]
pub fn set_password(profile_id: &str, password: &str) -> Result<(), String> {
    set_secret(profile_id, "vpn", password)
}

#[cfg(not(target_os = "macos"))]
pub fn get_password(profile_id: &str) -> Result<Option<String>, String> {
    get_secret(profile_id, "vpn")
}

#[cfg(not(target_os = "macos"))]
pub fn delete_password(profile_id: &str) -> Result<(), String> {
    delete_secret(profile_id, "vpn")
}

#[cfg(not(target_os = "macos"))]
pub fn set_token_pin(profile_id: &str, pin: &str) -> Result<(), String> {
    set_secret(profile_id, "token-pin", pin)
}

#[cfg(not(target_os = "macos"))]
pub fn get_token_pin(profile_id: &str) -> Result<Option<String>, String> {
    get_secret(profile_id, "token-pin")
}

#[cfg(not(target_os = "macos"))]
pub fn delete_token_pin(profile_id: &str) -> Result<(), String> {
    delete_secret(profile_id, "token-pin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keychain_operations() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());

        // Set
        set_password(&test_id, "test-secret-123").unwrap();

        // Get
        let password = get_password(&test_id).unwrap();
        assert_eq!(password, Some("test-secret-123".to_string()));

        // Delete
        delete_password(&test_id).unwrap();
        let password = get_password(&test_id).unwrap();
        assert_eq!(password, None);

        // Delete non-existent is ok
        delete_password(&test_id).unwrap();
    }
}
