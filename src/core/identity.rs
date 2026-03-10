//! Client identity — machine UUID → SHA-256 hex string.
//!
//! Compatible with JS `lib/identity.js`.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command as SysCommand;

/// Get or create a cached client ID.
/// First call generates from machine UUID, writes to `{chub_dir}/client_id`.
/// Subsequent calls return the cached value.
pub fn get_or_create_client_id(chub_dir: &Path) -> Result<String> {
    let id_path = chub_dir.join("client_id");

    // Try reading cached ID
    if id_path.exists() {
        let cached = std::fs::read_to_string(&id_path)?;
        let trimmed = cached.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    // Generate from machine UUID
    let machine_id = get_machine_uuid()?;
    let mut hasher = Sha256::new();
    hasher.update(machine_id.as_bytes());
    let hash = hasher.finalize();
    let hex_id = format!("{:x}", hash);

    // Cache to disk
    if let Some(parent) = id_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&id_path, &hex_id)?;

    Ok(hex_id)
}

/// Get machine UUID (platform-specific).
fn get_machine_uuid() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let output = SysCommand::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("IOPlatformUUID")
                && let Some(uuid) = line.split('"').nth(3)
            {
                return Ok(uuid.to_string());
            }
        }
        anyhow::bail!("Could not find IOPlatformUUID")
    }

    #[cfg(target_os = "linux")]
    {
        // Try /etc/machine-id first, then /var/lib/dbus/machine-id
        if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
            return Ok(id.trim().to_string());
        }
        if let Ok(id) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
            return Ok(id.trim().to_string());
        }
        anyhow::bail!("Could not find machine-id")
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        // Fallback: use hostname
        let output = SysCommand::new("hostname").output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// Get or create client ID with a pre-seeded UUID (for testing).
pub fn get_or_create_client_id_with_uuid(chub_dir: &Path, machine_uuid: &str) -> Result<String> {
    let id_path = chub_dir.join("client_id");

    if id_path.exists() {
        let cached = std::fs::read_to_string(&id_path)?;
        let trimmed = cached.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(machine_uuid.as_bytes());
    let hash = hasher.finalize();
    let hex_id = format!("{:x}", hash);

    if let Some(parent) = id_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&id_path, &hex_id)?;

    Ok(hex_id)
}
