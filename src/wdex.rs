//! Windows Defender exclusion management module.
//!
//! Provides functionality to add, remove, and list Windows Defender exclusions.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::{self, Write};

use crate::{color, utils};

// ============================================================================
// Public API
// ============================================================================

/// Adds a path or process to Windows Defender exclusions.
///
/// # Arguments
/// * `path_str` - The path or process name to exclude
/// * `process` - If true, adds as process exclusion; otherwise as path exclusion
///
/// # Behavior
/// - Requires administrator privileges
/// - Prompts for elevation if not running as admin
/// - Warns if path doesn't exist (for path exclusions)
pub fn add(path_str: &str, process: bool) -> Result<()> {
    // Check for admin privileges
    if !utils::is_elevated() {
        return run_elevated("add", path_str, process);
    }

    let path = resolve_exclusion_path(path_str, process)?;
    let path_string = path.to_string_lossy().to_string();

    // Validate path existence (for non-process exclusions)
    if !process && !path.exists() {
        if !prompt_for_non_existent_path(&path_string)? {
            return Ok(());
        }
    }

    // Execute PowerShell command
    execute_defender_command(build_add_command(&path_string, process))?;

    utils::print_success(&format!(
        "Added to Windows Defender exclusions: {}",
        path_string
    ));

    Ok(())
}

/// Removes a path or process from Windows Defender exclusions.
///
/// # Arguments
/// * `path_str` - The path or process name to remove
/// * `process` - If true, removes from process exclusions; otherwise from path exclusions
///
/// # Behavior
/// - Requires administrator privileges
/// - Prompts for confirmation before removal
pub fn remove(path_str: &str, process: bool) -> Result<()> {
    // Check for admin privileges
    if !utils::is_elevated() {
        return run_elevated("remove", path_str, process);
    }

    let path = resolve_exclusion_path(path_str, process)?;
    let path_string = path.to_string_lossy().to_string();

    // Confirm removal
    if !confirm_removal()? {
        return Ok(());
    }

    // Execute PowerShell command
    execute_defender_command(build_remove_command(&path_string, process))?;

    utils::print_success(&format!(
        "Removed from Windows Defender exclusions: {}",
        path_string
    ));

    Ok(())
}

/// Lists all Windows Defender exclusions.
///
/// # Output
/// Displays path, process, and extension exclusions with existence checks.
pub fn list() -> Result<()> {
    println!();

    let exclusion_types = vec![
        ("Path exclusions", "(Get-MpPreference).ExclusionPath"),
        ("Process exclusions", "(Get-MpPreference).ExclusionProcess"),
        ("Extension exclusions", "(Get-MpPreference).ExclusionExtension"),
    ];

    let mut has_exclusions = false;

    for (title, command) in exclusion_types {
        let items = get_exclusion_items(command)?;

        if !items.is_empty() {
            has_exclusions = true;
            display_exclusion_list(title, &items, title.contains("Path"));
        }
    }

    if !has_exclusions {
        println!("  {}", color::note("No exclusions configured."));
    }

    if !utils::is_elevated() {
        println!("\n  {}", color::note("Note: Some details may require administrator privileges."));
    }

    println!();
    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Resolves a path string to an absolute PathBuf.
fn resolve_exclusion_path(path_str: &str, is_process: bool) -> Result<PathBuf> {
    if is_process {
        // Process names are used as-is
        Ok(PathBuf::from(path_str))
    } else {
        // Convert relative paths to absolute
        let p = PathBuf::from(path_str);
        if p.is_absolute() {
            Ok(p)
        } else {
            Ok(std::env::current_dir()?.join(p))
        }
    }
}

/// Builds the PowerShell command to add an exclusion.
fn build_add_command(path: &str, is_process: bool) -> String {
    if is_process {
        format!("Add-MpPreference -ExclusionProcess '{}'", path)
    } else {
        format!("Add-MpPreference -ExclusionPath '{}'", path)
    }
}

/// Builds the PowerShell command to remove an exclusion.
fn build_remove_command(path: &str, is_process: bool) -> String {
    if is_process {
        format!("Remove-MpPreference -ExclusionProcess '{}'", path)
    } else {
        format!("Remove-MpPreference -ExclusionPath '{}'", path)
    }
}

/// Executes a PowerShell command for Windows Defender.
fn execute_defender_command(command: String) -> Result<()> {
    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &command
        ])
        .output()
        .context("Failed to execute PowerShell")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("Windows Defender operation failed: {}", error);
    }

    Ok(())
}

/// Gets exclusion items from Windows Defender.
fn get_exclusion_items(command: &str) -> Result<Vec<String>> {
    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command
        ])
        .output()
        .context("Failed to execute PowerShell")?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(output_str
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect())
    } else {
        Ok(Vec::new())
    }
}

/// Displays a list of exclusions with optional existence check.
fn display_exclusion_list(title: &str, items: &[String], check_existence: bool) {
    println!("  {}:", color::header(title));

    for item in items {
        if check_existence {
            let exists = Path::new(item).exists();
            if exists {
                println!("    - {}", item);
            } else {
                println!("    - {}", color::missing_path(item));
            }
        } else {
            println!("    - {}", item);
        }
    }

    println!();
}

/// Prompts for confirmation when adding a non-existent path.
fn prompt_for_non_existent_path(path: &str) -> Result<bool> {
    utils::print_warn(&format!("Path does not exist: {}", path));

    print!("  Add anyway? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        utils::print_info("Cancelled.");
        return Ok(false);
    }

    Ok(true)
}

/// Prompts for removal confirmation.
fn confirm_removal() -> Result<bool> {
    print!("  Remove from exclusions? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        utils::print_info("Cancelled.");
        return Ok(false);
    }

    Ok(true)
}

/// Runs the command with elevated privileges.
fn run_elevated(action: &str, path: &str, process: bool) -> Result<()> {
    utils::print_warn("Administrator privileges required.");

    print!("  Restart as administrator? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        utils::print_info("Cancelled.");
        return Ok(());
    }

    let exe_path = std::env::current_exe()?;
    let mut args = vec!["wdex", action, path];
    if process {
        args.push("--process");
    }

    // Windows-specific elevation using ShellExecute
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::*;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::*;

        let exe_path_str = exe_path.to_string_lossy();
        let args_str = args.join(" ");

        unsafe {
            let exe_path_utf16: Vec<u16> = exe_path_str
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let args_utf16: Vec<u16> = args_str
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let result = ShellExecuteW(
                None,
                w!("runas"),
                PCWSTR::from_raw(exe_path_utf16.as_ptr()),
                PCWSTR::from_raw(args_utf16.as_ptr()),
                None,
                SW_SHOWNORMAL
            );

            if result.0 <= 32 {
                bail!("Failed to run as administrator");
            }
        }
    }

    utils::print_info("Command executed in elevated process.");
    Ok(())
}