//! Environment variable management module for uwu.
//!
//! Sets, shows, lists, and removes persistent environment variables in the
//! Windows registry (user or system scope), mirroring the `path` command.
//! Changes are broadcast via `WM_SETTINGCHANGE`; `uwu reload` re-imports them
//! into the current session.

use anyhow::{bail, Result};

#[cfg(windows)]
use crate::path;
use crate::{color, utils};

/// Registry path for system-wide environment variables.
#[cfg(windows)]
const SYSTEM_ENV: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// Is this the PATH variable? (managed by the dedicated `uwu path` command)
fn is_path_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("path")
}

/// Truncate a display value to `max` chars, adding an ellipsis if cut.
fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() > max {
        let head: String = value.chars().take(max).collect();
        format!("{}…", head)
    } else {
        value.to_string()
    }
}

/// Set (or update) an environment variable.
#[cfg(windows)]
pub fn set(key: &str, value: &str, system: bool) -> Result<()> {
    use winreg::enums::REG_EXPAND_SZ;
    use winreg::RegValue;

    if is_path_key(key) {
        bail!("use 'uwu path' to manage PATH");
    }
    if system && !utils::is_elevated() {
        bail!("system environment variables require administrator privileges");
    }

    let regkey = open_env_key(system, true)?;

    // Values containing %VAR% references are stored as REG_EXPAND_SZ.
    if value.contains('%') {
        let rv = RegValue {
            bytes: utf16_bytes(value),
            vtype: REG_EXPAND_SZ,
        };
        regkey.set_raw_value(key, &rv)?;
    } else {
        regkey.set_value(key, &value.to_string())?;
    }

    path::broadcast_environment_change();

    println!();
    utils::print_success(&format!("set {} = {}", key, value));
    reload_hint();
    Ok(())
}

/// Show a single variable's value.
#[cfg(windows)]
pub fn get(key: &str, system: bool) -> Result<()> {
    let regkey = open_env_key(system, false)?;
    let value: String = regkey
        .get_value(key)
        .map_err(|_| anyhow::anyhow!("'{}' is not set", key))?;

    println!();
    println!("  {} {} {}", color::accent(key), color::note("="), value);
    Ok(())
}

/// Remove (unset) a variable.
#[cfg(windows)]
pub fn remove(key: &str, system: bool) -> Result<()> {
    if is_path_key(key) {
        bail!("refusing to delete PATH; use 'uwu path'");
    }
    if system && !utils::is_elevated() {
        bail!("system environment variables require administrator privileges");
    }

    let regkey = open_env_key(system, true)?;
    regkey
        .delete_value(key)
        .map_err(|_| anyhow::anyhow!("'{}' is not set", key))?;

    path::broadcast_environment_change();

    println!();
    utils::print_success(&format!("unset {}", key));
    reload_hint();
    Ok(())
}

/// List all variables in the selected scope.
#[cfg(windows)]
pub fn list(system: bool) -> Result<()> {
    let regkey = open_env_key(system, false)?;

    let mut items: Vec<(String, String)> = regkey
        .enum_values()
        .filter_map(|r| r.ok())
        .map(|(name, _)| {
            let value: String = regkey.get_value(&name).unwrap_or_default();
            (name, value)
        })
        .collect();
    items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    println!();
    println!(
        "  {}",
        color::header(if system {
            "system environment:"
        } else {
            "user environment:"
        })
    );
    for (name, value) in items {
        println!(
            "    {} {} {}",
            color::accent(&name),
            color::note("="),
            truncate(&value, 70)
        );
    }
    Ok(())
}

/// Open the environment registry key for the given scope.
#[cfg(windows)]
fn open_env_key(system: bool, write: bool) -> Result<winreg::RegKey> {
    use anyhow::Context;
    use winreg::enums::*;
    use winreg::RegKey;

    let flags = if write {
        KEY_READ | KEY_WRITE
    } else {
        KEY_READ
    };

    if system {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(SYSTEM_ENV, flags)
            .context("failed to open system environment (run as administrator)")
    } else {
        Ok(RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", flags)?)
    }
}

/// Encode a string as a null-terminated UTF-16LE byte buffer (for REG_EXPAND_SZ).
#[cfg(windows)]
fn utf16_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

#[cfg(windows)]
fn reload_hint() {
    if !utils::is_quiet() {
        println!();
        println!(
            "  {}",
            color::note("run 'uwu reload' to apply in current session")
        );
    }
}

// Non-Windows stubs.
#[cfg(not(windows))]
pub fn set(_key: &str, _value: &str, _system: bool) -> Result<()> {
    bail!("env is only supported on Windows")
}
#[cfg(not(windows))]
pub fn get(_key: &str, _system: bool) -> Result<()> {
    bail!("env is only supported on Windows")
}
#[cfg(not(windows))]
pub fn remove(_key: &str, _system: bool) -> Result<()> {
    bail!("env is only supported on Windows")
}
#[cfg(not(windows))]
pub fn list(_system: bool) -> Result<()> {
    bail!("env is only supported on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_key_detected_case_insensitively() {
        assert!(is_path_key("PATH"));
        assert!(is_path_key("Path"));
        assert!(is_path_key("path"));
        assert!(!is_path_key("PATHEXT"));
        assert!(!is_path_key("MYVAR"));
    }

    #[test]
    fn truncate_leaves_short_values() {
        assert_eq!(truncate("hello", 70), "hello");
    }

    #[test]
    fn truncate_cuts_long_values_with_ellipsis() {
        let long = "a".repeat(100);
        let out = truncate(&long, 70);
        assert_eq!(out.chars().count(), 71); // 70 + ellipsis
        assert!(out.ends_with('…'));
    }

    #[cfg(windows)]
    #[test]
    fn utf16_bytes_are_null_terminated_le() {
        let b = utf16_bytes("A"); // 'A' = 0x41
        assert_eq!(b, vec![0x41, 0x00, 0x00, 0x00]);
    }
}
