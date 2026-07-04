//! Init module for uwu.
//!
//! Sets up PowerShell profile integration so uwu aliases are loaded on startup.
//! Keeps all uwu logic in ~/.uwu/profile.ps1, only adds a one-line source to $PROFILE.

use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::{color, utils};

/// The source line we add to $PROFILE
const SOURCE_LINE: &str = ". \"$env:USERPROFILE\\.uwu\\profile.ps1\"";

/// The profile script content written to ~/.uwu/profile.ps1
const PROFILE_CONTENT: &str = r#"# uwu - windows utilities
# This file is managed by 'uwu init'. Do not edit manually.

# Load aliases on startup
Invoke-Expression (uwu.exe alias --load | Out-String)

# Wrapper function: intercepts 'uwu reload' to reload in current session
function uwu {
    if ($args[0] -eq 'reload') {
        . $PROFILE
        return
    }
    uwu.exe @args
}
"#;

/// Check if $PROFILE already has the uwu source line
fn profile_has_uwu(content: &str) -> bool {
    content.contains(".uwu") && content.contains("profile.ps1")
}

/// Build the new $PROFILE content with uwu source line appended
fn build_profile_addition(existing: &str) -> String {
    let addition = format!("\n# uwu\n{}\n", SOURCE_LINE);
    if existing.is_empty() {
        addition.trim_start().to_string()
    } else {
        format!("{}{}", existing.trim_end(), addition)
    }
}

/// Get the PowerShell $PROFILE path
fn get_profile_path() -> Result<PathBuf> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Write-Output $PROFILE",
        ])
        .output()
        .context("Failed to run powershell")?;

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path_str))
}

/// Get ~/.uwu directory path
fn uwu_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".uwu")
}

/// Write the uwu profile script to ~/.uwu/profile.ps1
fn write_uwu_profile(uwu_dir: &PathBuf) -> Result<()> {
    fs::write(uwu_dir.join("profile.ps1"), PROFILE_CONTENT)?;
    Ok(())
}

/// Run init
pub fn run() -> Result<()> {
    let profile_path = get_profile_path()?;
    let uwu_dir = uwu_dir();

    println!();

    // Ensure ~/.uwu/ exists
    if !uwu_dir.exists() {
        fs::create_dir_all(&uwu_dir)?;
        utils::print_info("created ~/.uwu/");
    }

    // Ensure aliases.json exists
    let aliases_file = uwu_dir.join("aliases.json");
    if !aliases_file.exists() {
        fs::write(&aliases_file, "{}")?;
        utils::print_info("created ~/.uwu/aliases.json");
    }

    // Write ~/.uwu/profile.ps1 (always overwrite to keep up-to-date)
    write_uwu_profile(&uwu_dir)?;
    utils::print_success("wrote ~/.uwu/profile.ps1");

    // Ensure $PROFILE directory exists
    if let Some(parent) = profile_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    // Add source line to $PROFILE if not already there
    let existing = if profile_path.exists() {
        fs::read_to_string(&profile_path)?
    } else {
        String::new()
    };

    if profile_has_uwu(&existing) {
        utils::print_info("$PROFILE already configured ~");
    } else {
        // Confirm before writing to $PROFILE
        println!();
        println!("  the following line will be added to $PROFILE:");
        println!("    {}", color::accent(SOURCE_LINE));
        println!();
        println!("  {} {}", color::note("$PROFILE:"), profile_path.display());
        println!();
        print!("  {} ", color::warn("proceed? [y/N]:"));
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            utils::print_info("cancelled.");
            return Ok(());
        }

        let new_content = build_profile_addition(&existing);
        fs::write(&profile_path, new_content)?;
        utils::print_success("added source line to $PROFILE");
    }

    println!();
    println!("  {} ~/.uwu/profile.ps1", color::note("uwu config:"));
    println!("  {} {}", color::note("ps profile:"), profile_path.display());
    println!(
        "  {}",
        color::note("restart your terminal to activate")
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // --- SOURCE_LINE ---

    #[test]
    fn source_line_dots_uwu_profile() {
        assert!(SOURCE_LINE.starts_with(". "));
        assert!(SOURCE_LINE.contains(".uwu"));
        assert!(SOURCE_LINE.contains("profile.ps1"));
    }

    #[test]
    fn source_line_uses_env_userprofile() {
        assert!(SOURCE_LINE.contains("$env:USERPROFILE"));
    }

    // --- PROFILE_CONTENT ---

    #[test]
    fn profile_content_has_alias_loader() {
        assert!(PROFILE_CONTENT.contains("uwu.exe alias --load"));
    }

    #[test]
    fn profile_content_has_reload_wrapper() {
        assert!(PROFILE_CONTENT.contains("function uwu"));
        assert!(PROFILE_CONTENT.contains("'reload'"));
        assert!(PROFILE_CONTENT.contains(". $PROFILE"));
    }

    #[test]
    fn profile_content_forwards_to_exe() {
        assert!(PROFILE_CONTENT.contains("uwu.exe @args"));
    }

    #[test]
    fn profile_content_has_do_not_edit_warning() {
        assert!(PROFILE_CONTENT.contains("Do not edit manually"));
    }

    // --- profile_has_uwu ---

    #[test]
    fn has_uwu_with_source_line() {
        assert!(profile_has_uwu(SOURCE_LINE));
    }

    #[test]
    fn has_uwu_with_full_profile() {
        let content = "# my profile\nsome stuff\n. \"$env:USERPROFILE\\.uwu\\profile.ps1\"\n";
        assert!(profile_has_uwu(content));
    }

    #[test]
    fn has_uwu_empty() {
        assert!(!profile_has_uwu(""));
    }

    #[test]
    fn has_uwu_unrelated() {
        assert!(!profile_has_uwu("Set-Alias ll Get-ChildItem"));
    }

    #[test]
    fn has_uwu_partial_uwu_only() {
        assert!(!profile_has_uwu("something .uwu something"));
    }

    #[test]
    fn has_uwu_partial_profile_only() {
        assert!(!profile_has_uwu("something profile.ps1 something"));
    }

    #[test]
    fn has_uwu_both_on_different_lines() {
        let content = "line with .uwu here\nanother line profile.ps1\n";
        assert!(profile_has_uwu(content));
    }

    // --- build_profile_addition ---

    #[test]
    fn addition_empty_profile() {
        let result = build_profile_addition("");
        assert!(result.starts_with("# uwu"));
        assert!(result.contains(SOURCE_LINE));
        assert!(!result.starts_with('\n'));
    }

    #[test]
    fn addition_existing_profile() {
        let existing = "# my profile\nSet-Alias ll Get-ChildItem";
        let result = build_profile_addition(existing);
        assert!(result.starts_with("# my profile"));
        assert!(result.contains("Set-Alias ll Get-ChildItem"));
        assert!(result.contains("# uwu"));
        assert!(result.contains(SOURCE_LINE));
    }

    #[test]
    fn addition_trims_trailing_whitespace() {
        let existing = "content\n\n\n";
        let result = build_profile_addition(existing);
        assert!(result.contains("content\n# uwu"));
    }

    #[test]
    fn addition_ends_with_newline() {
        let result = build_profile_addition("stuff");
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn addition_preserves_source_line_exact() {
        let result = build_profile_addition("");
        assert!(result.contains(SOURCE_LINE));
    }

    #[test]
    fn addition_empty_has_no_double_newline_at_start() {
        let result = build_profile_addition("");
        assert!(!result.starts_with("\n\n"));
    }

    // --- write_uwu_profile ---

    #[test]
    fn write_profile_creates_file() {
        let dir = tempdir().unwrap();
        write_uwu_profile(&dir.path().to_path_buf()).unwrap();
        assert!(dir.path().join("profile.ps1").exists());
    }

    #[test]
    fn write_profile_content_matches() {
        let dir = tempdir().unwrap();
        write_uwu_profile(&dir.path().to_path_buf()).unwrap();
        let content = fs::read_to_string(dir.path().join("profile.ps1")).unwrap();
        assert_eq!(content, PROFILE_CONTENT);
    }

    #[test]
    fn write_profile_overwrites() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.ps1");
        fs::write(&path, "old").unwrap();
        write_uwu_profile(&dir.path().to_path_buf()).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, PROFILE_CONTENT);
    }

    // --- uwu_dir ---

    #[test]
    fn uwu_dir_ends_with_dot_uwu() {
        assert_eq!(uwu_dir().file_name().unwrap(), ".uwu");
    }

    #[test]
    fn uwu_dir_under_userprofile() {
        if let Ok(home) = std::env::var("USERPROFILE") {
            assert_eq!(uwu_dir(), PathBuf::from(home).join(".uwu"));
        }
    }
}
