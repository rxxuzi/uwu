//! Directory bookmark module for uwu.
//!
//! Bookmarks named directories in ~/.uwu/bookmarks.json and jumps to them.
//! Since a child process cannot change the parent shell's working directory,
//! jumping is done by the PowerShell wrapper (installed by `uwu init`), which
//! calls `uwu go --resolve <name>` to get the path and then `Set-Location`s.

use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::{color, utils};

/// Get the bookmarks.json file path (~/.uwu/bookmarks.json)
fn bookmarks_file() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".uwu").join("bookmarks.json")
}

/// Ensure ~/.uwu directory and bookmarks.json exist
fn ensure_bookmarks_file() -> Result<PathBuf> {
    let path = bookmarks_file();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    if !path.exists() {
        fs::write(&path, "{}")?;
    }
    Ok(path)
}

/// Load bookmarks from disk
fn load() -> Result<BTreeMap<String, String>> {
    let path = ensure_bookmarks_file()?;
    let content = fs::read_to_string(&path)?;
    let map: BTreeMap<String, String> = serde_json::from_str(&content)?;
    Ok(map)
}

/// Save bookmarks to disk
fn save(map: &BTreeMap<String, String>) -> Result<()> {
    let path = ensure_bookmarks_file()?;
    let content = serde_json::to_string_pretty(map)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Resolve a user-supplied path to an absolute, existing directory.
fn resolve_dir(path: &str) -> Result<String> {
    let p = PathBuf::from(path);
    let abs = if p.is_absolute() {
        p
    } else {
        std::env::current_dir()?.join(&p)
    };

    if !abs.exists() {
        bail!("directory not found: {}", abs.display());
    }
    if !abs.is_dir() {
        bail!("not a directory: {}", abs.display());
    }

    // Canonicalize to collapse `.`/`..`, then drop the Windows \\?\ prefix.
    let canon = fs::canonicalize(&abs)?;
    let s = canon.to_string_lossy();
    Ok(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

/// Set (or update) a bookmark.
pub fn set(alias: &str, path: &str) -> Result<()> {
    let target = resolve_dir(path)?;
    let mut map = load()?;
    let updating = map.contains_key(alias);
    map.insert(alias.to_string(), target.clone());
    save(&map)?;

    println!();
    if updating {
        utils::print_success(&format!("updated bookmark: {} -> {}", alias, target));
    } else {
        utils::print_success(&format!("bookmarked: {} -> {}", alias, target));
    }
    Ok(())
}

/// Remove a bookmark.
pub fn remove(alias: &str) -> Result<()> {
    let mut map = load()?;
    if map.remove(alias).is_none() {
        bail!("no bookmark '{}'", alias);
    }
    save(&map)?;

    println!();
    utils::print_success(&format!("removed bookmark: {}", alias));
    Ok(())
}

/// List all bookmarks.
pub fn list() -> Result<()> {
    let map = load()?;

    println!();
    if map.is_empty() {
        println!("  {}", color::note("no bookmarks yet."));
        println!();
        println!("  {}", color::info("usage: uwu go <name> <path>"));
        return Ok(());
    }

    println!("  {}", color::header("bookmarks:"));
    for (name, path) in &map {
        let missing = !PathBuf::from(path).is_dir();
        if missing {
            println!(
                "    {} {} {} {}",
                color::accent(name),
                color::note("->"),
                path,
                color::error("(missing)")
            );
        } else {
            println!("    {} {} {}", color::accent(name), color::note("->"), path);
        }
    }
    Ok(())
}

/// Show a bookmark (fallback when the shell wrapper isn't installed — the exe
/// itself can't change the parent shell's directory).
pub fn show(alias: &str) -> Result<()> {
    let map = load()?;
    match map.get(alias) {
        Some(path) => {
            println!();
            println!("  {} {} {}", color::accent(alias), color::note("->"), path);
            if !utils::is_quiet() {
                println!();
                println!(
                    "  {}",
                    color::note("run 'uwu init' to enable 'uwu go <name>' to cd there")
                );
            }
            Ok(())
        }
        None => bail!("no bookmark '{}'", alias),
    }
}

/// Print the resolved path only (used by the PowerShell wrapper for Set-Location).
/// Emits nothing but the path on success; errors go to stderr with a non-zero exit.
pub fn resolve(alias: &str) -> Result<()> {
    let map = load()?;
    match map.get(alias) {
        Some(path) => {
            print!("{}", path);
            Ok(())
        }
        None => bail!("no bookmark '{}'", alias),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_file_path_structure() {
        let path = bookmarks_file();
        assert_eq!(path.file_name().unwrap(), "bookmarks.json");
        assert_eq!(path.parent().unwrap().file_name().unwrap(), ".uwu");
    }

    #[test]
    fn resolve_dir_accepts_existing_dir() {
        // The system temp dir always exists.
        let tmp = std::env::temp_dir();
        let got = resolve_dir(&tmp.to_string_lossy()).unwrap();
        assert!(PathBuf::from(&got).is_dir());
        assert!(!got.starts_with(r"\\?\"));
    }

    #[test]
    fn resolve_dir_rejects_missing() {
        assert!(resolve_dir(r"C:\this\does\not\exist\uwu-xyz").is_err());
    }

    #[test]
    fn resolve_dir_rejects_file() {
        // Create a temp file and confirm it's rejected (not a directory).
        let mut f = std::env::temp_dir();
        f.push("uwu_go_test_file.txt");
        fs::write(&f, "x").unwrap();
        let result = resolve_dir(&f.to_string_lossy());
        let _ = fs::remove_file(&f);
        assert!(result.is_err());
    }
}
