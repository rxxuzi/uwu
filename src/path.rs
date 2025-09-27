use anyhow::{Context, Result};
use colored::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use winreg::enums::*;
use winreg::RegKey;

use crate::utils;

// パステルカラー定義
fn pink_color(text: &str) -> ColoredString {
    text.truecolor(255, 182, 193)
}

fn cyan_color(text: &str) -> ColoredString {
    text.truecolor(173, 216, 230)
}

fn mint_color(text: &str) -> ColoredString {
    text.truecolor(189, 252, 201)
}

fn lavender_color(text: &str) -> ColoredString {
    text.truecolor(230, 230, 250)
}

pub fn add(path_str: &str, system: bool, force: bool) -> Result<()> {
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

    let current_path = get_path_variable(system)?;
    let paths: Vec<String> = current_path.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let abs_path_str = absolute_path.to_string_lossy().to_string();

    // Check if already exists
    if paths.iter().any(|p| p.eq_ignore_ascii_case(&abs_path_str)) {
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
        println!("  {}", lavender_color("Note: Restart your terminal for changes to take effect."));
    }

    Ok(())
}

pub fn remove(path_str: &str, system: bool, force: bool) -> Result<()> {
    let current_path = get_path_variable(system)?;
    let paths: Vec<String> = current_path.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let path_to_remove = PathBuf::from(path_str)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path_str))
        .to_string_lossy()
        .to_string();

    let filtered: Vec<String> = paths.iter()
        .filter(|p| !p.eq_ignore_ascii_case(&path_to_remove))
        .cloned()
        .collect();

    if filtered.len() == paths.len() {
        utils::print_warn(&format!("Not found in PATH: {}", path_to_remove));
        return Ok(());
    }

    if !force {
        print!("  Remove from PATH? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            utils::print_info("Cancelled.");
            return Ok(());
        }
    }

    let new_path_str = filtered.join(";");
    set_path_variable(system, &new_path_str)?;
    broadcast_environment_change();

    utils::print_success(&format!(
        "Removed from {} PATH: {}",
        if system { "system" } else { "user" },
        path_to_remove
    ));

    Ok(())
}

pub fn list(system: bool) -> Result<()> {
    println!();

    if !system {
        println!("  {}", cyan_color("User PATH:"));
        let user_path = get_path_variable(false)?;
        print_path_entries(&user_path, "    ");
        println!();
    }

    if system {
        println!("  {}", cyan_color("System PATH:"));
        let system_path = get_path_variable(true)?;
        print_path_entries(&system_path, "    ");
        println!();
    }

    if !system {
        println!("  {}", lavender_color("Tip: Use --system to show system PATH"));
    }

    Ok(())
}

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
        let normalized = path.to_lowercase();

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
        println!("  {} duplicates found:", duplicates.len());
        for dup in &duplicates {
            println!("    - {}", dup);
        }
    }

    if !invalid.is_empty() {
        println!("  {} invalid paths found:", invalid.len());
        for inv in &invalid {
            println!("    x {}", pink_color(inv));
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
        "Cleaned {} PATH: removed {} duplicates and {} invalid paths",
        if system { "system" } else { "user" },
        duplicates.len(),
        invalid.len()
    ));

    Ok(())
}

// Helper functions
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
    // TODO
}

fn print_path_entries(path_str: &str, indent: &str) {
    let paths: Vec<&str> = path_str.split(';')
        .filter(|s| !s.is_empty())
        .collect();

    if paths.is_empty() {
        println!("{}{}", indent, lavender_color("(empty)"));
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
                     pink_color("(not found)")
            );
        }
    }
}