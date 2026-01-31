//! Windows PATH environment variable management module.
//!
//! This module provides functionality to manipulate the Windows PATH environment variable,
//! including adding, removing, listing, and cleaning PATH entries.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use winreg::enums::*;
use winreg::RegKey;

use crate::{color, utils};

// ============================================================================
// Path Normalization
// ============================================================================

/// Normalizes a Windows path by removing UNC prefixes and standardizing separators.
fn normalize_windows_path(path: &str) -> String {
    let path = path.trim();

    // Remove UNC prefix if present
    let without_prefix = if path.starts_with(r"\\?\") {
        &path[4..]
    } else {
        path
    };

    // Standardize path separators and remove trailing backslash
    without_prefix
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

/// Normalizes a path for case-insensitive comparison.
fn normalize_for_comparison(path: &str) -> String {
    normalize_windows_path(path).to_lowercase()
}

// ============================================================================
// Public API
// ============================================================================

/// Adds a directory to the PATH environment variable.
pub fn add(path_str: &str, system: bool, force: bool) -> Result<()> {
    // Resolve to absolute path
    let path = PathBuf::from(path_str);
    let absolute_path = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()?.join(&path)
    };

    // Check if path exists
    if !force && !absolute_path.exists() {
        utils::print_warn(&format!("Path does not exist: {}", absolute_path.display()));

        if !force {
            print!("  Add anyway? [y/N]: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                utils::print_info("Cancelled.");
                return Ok(());
            }
        }
    }

    // Load current PATH
    let current_path = get_path_variable(system)?;
    let paths: Vec<String> = current_path.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let abs_path_str = normalize_windows_path(&absolute_path.to_string_lossy());

    // Check for duplicates
    if paths.iter().any(|p| normalize_for_comparison(p) == normalize_for_comparison(&abs_path_str)) {
        utils::print_info(&format!("Already in PATH: {}", abs_path_str));
        return Ok(());
    }

    // Add new path
    let mut new_paths = paths;
    new_paths.push(abs_path_str.clone());
    let new_path_str = new_paths.join(";");

    // Update registry
    set_path_variable(system, &new_path_str)?;
    broadcast_environment_change();

    utils::print_success(&format!(
        "Added to {} PATH: {}",
        if system { "system" } else { "user" },
        abs_path_str
    ));

    if !utils::is_quiet() {
        println!();
        println!("  {}", color::note("Note: Restart your terminal for changes to take effect."));
    }

    Ok(())
}

/// Removes a directory from the PATH environment variable.
pub fn remove(path_str: &str, system: bool, force: bool) -> Result<()> {
    let current_path = get_path_variable(system)?;
    let paths: Vec<String> = current_path.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Try parsing as index first
    if let Ok(index) = path_str.parse::<usize>() {
        if index == 0 || index > paths.len() {
            utils::print_warn(&format!("Invalid index: {} (valid range: 1-{})", index, paths.len()));
            return Ok(());
        }

        let path_to_remove = paths[index - 1].clone();

        if !force {
            println!("  Path to remove: {}", path_to_remove);
            print!("  Remove from PATH? [y/N]: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                utils::print_info("Cancelled.");
                return Ok(());
            }
        }

        let mut new_paths = paths.clone();
        new_paths.remove(index - 1);
        let new_path_str = new_paths.join(";");

        set_path_variable(system, &new_path_str)?;
        broadcast_environment_change();

        utils::print_success(&format!(
            "Removed from {} PATH: {}",
            if system { "system" } else { "user" },
            path_to_remove
        ));

        return Ok(());
    }

    // Otherwise treat as path
    let absolute_path = if Path::new(path_str).is_absolute() {
        PathBuf::from(path_str)
    } else {
        std::env::current_dir()?.join(path_str)
    };

    let search_path = normalize_windows_path(&absolute_path.to_string_lossy());
    let normalized_search = normalize_for_comparison(&search_path);

    // Find exact matches only
    let mut found_indices = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let normalized = normalize_for_comparison(p);
        if normalized == normalized_search {
            found_indices.push(i);
        }
    }

    if found_indices.is_empty() {
        utils::print_warn(&format!("Not found in PATH: {}", search_path));
        if !absolute_path.exists() {
            println!("  Note: Path does not exist on disk: {}", absolute_path.display());
        }
        println!("  Tip: Use 'uwu path list' to see all PATH entries with their index numbers.");
        return Ok(());
    }

    // Multiple matches found
    if found_indices.len() > 1 {
        println!("  Multiple matches found:");
        for &idx in &found_indices {
            println!("    {} - {}", idx + 1, &paths[idx]);
        }
        println!("  Use the index number to remove a specific entry.");
        return Ok(());
    }

    // Single match found
    let idx = found_indices[0];
    let matched_path = paths[idx].clone();

    if !force {
        println!("  Path to remove: {}", matched_path);
        print!("  Remove from PATH? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            utils::print_info("Cancelled.");
            return Ok(());
        }
    }

    let filtered: Vec<String> = paths.into_iter()
        .enumerate()
        .filter(|(i, _)| *i != idx)
        .map(|(_, p)| p)
        .collect();

    let new_path_str = filtered.join(";");
    set_path_variable(system, &new_path_str)?;
    broadcast_environment_change();

    utils::print_success(&format!(
        "Removed from {} PATH: {}",
        if system { "system" } else { "user" },
        matched_path
    ));

    Ok(())
}

/// Lists all PATH entries.
pub fn list(system: bool) -> Result<()> {
    println!();

    if !system {
        println!("  {}", color::header("User PATH:"));
        let user_path = get_path_variable(false)?;
        print_path_entries(&user_path, "    ");
        println!();
    }

    if system {
        println!("  {}", color::header("System PATH:"));
        let system_path = get_path_variable(true)?;
        print_path_entries(&system_path, "    ");
        println!();
    }

    if !system {
        println!("  {}", color::note("Tip: Use --system to show system PATH"));
    }

    Ok(())
}

/// Removes duplicate and invalid PATH entries.
pub fn clean(system: bool, force: bool) -> Result<()> {
    let current_path = get_path_variable(system)?;
    let paths: Vec<String> = current_path.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut seen = HashSet::new();
    let mut cleaned = Vec::new();
    let mut duplicates = Vec::new();
    let mut invalid = Vec::new();

    for path in paths {
        let normalized = normalize_for_comparison(&path);

        if seen.contains(&normalized) {
            duplicates.push(path.clone());
        } else if !Path::new(&path).exists() {
            invalid.push(path.clone());
        } else {
            seen.insert(normalized);
            cleaned.push(path);
        }
    }

    if duplicates.is_empty() && invalid.is_empty() {
        utils::print_info("PATH is already clean!");
        return Ok(());
    }

    // Show what will be removed
    println!();
    if !duplicates.is_empty() {
        println!("  {} duplicate(s) found:", duplicates.len());
        for dup in &duplicates {
            println!("    - {}", dup);
        }
    }

    if !invalid.is_empty() {
        println!("  {} invalid path(s) found:", invalid.len());
        for inv in &invalid {
            println!("    × {}", color::error(inv));
        }
    }

    if !force {
        print!("\n  Clean PATH? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            utils::print_info("Cancelled.");
            return Ok(());
        }
    }

    let new_path_str = cleaned.join(";");
    set_path_variable(system, &new_path_str)?;
    broadcast_environment_change();

    utils::print_success(&format!(
        "Cleaned {} PATH: removed {} duplicate(s) and {} invalid path(s)",
        if system { "system" } else { "user" },
        duplicates.len(),
        invalid.len()
    ));

    Ok(())
}

// ============================================================================
// Registry Operations
// ============================================================================

fn get_path_variable(system: bool) -> Result<String> {
    let reg_key = if system {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")?
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Environment")?
    };

    let path: String = reg_key.get_value("Path")
        .unwrap_or_else(|_| String::new());

    Ok(path)
}

fn set_path_variable(system: bool, new_path: &str) -> Result<()> {
    let reg_key = if system {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(
                r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
                KEY_READ | KEY_WRITE
            ).context("Failed to open system registry. Run as administrator.")?
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?
    };

    reg_key.set_value("Path", &new_path)?;
    Ok(())
}

fn broadcast_environment_change() {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::*;

        unsafe {
            let env_str: Vec<u16> = "Environment\0".encode_utf16().collect();
            let mut result: usize = 0;

            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(env_str.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                Some(&mut result as *mut usize),
            );
        }
    }

    #[cfg(not(windows))]
    {
        // Non-Windows platforms: do nothing
    }
}

fn print_path_entries(path_str: &str, indent: &str) {
    let paths: Vec<&str> = path_str.split(';')
        .filter(|s| !s.is_empty())
        .collect();

    if paths.is_empty() {
        println!("{}{}", indent, color::note("(empty)"));
        return;
    }

    for (i, path) in paths.iter().enumerate() {
        let num = format!("{}.", i + 1);
        let exists = Path::new(path).exists();

        if exists {
            println!("{}{:<4} {}", indent, num, path);
        } else {
            println!("{}{:<4} {} {}",
                     indent,
                     num,
                     path,
                     color::error("(not found)")
            );
        }
    }
}