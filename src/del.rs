//! Safe recursive delete module for uwu.

use anyhow::{bail, Result};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::color;
use crate::utils;

/// Summary of a delete operation
#[derive(Debug, Default)]
pub struct DeleteSummary {
    pub files_deleted: u64,
    pub dirs_deleted: u64,
    pub bytes_freed: u64,
}

impl DeleteSummary {
    fn add_file(&mut self, size: u64) {
        self.files_deleted += 1;
        self.bytes_freed += size;
    }

    fn add_dir(&mut self) {
        self.dirs_deleted += 1;
    }
}

/// List of protected path prefixes that should never be deleted
const PROTECTED_PATH_PREFIXES: &[&str] = &[
    // Windows system directories
    r"C:\Windows",
    r"C:\Program Files",
    r"C:\Program Files (x86)",
    r"C:\ProgramData",
    r"C:\Users\Default",
    r"C:\Users\Public",
    r"C:\Recovery",
    r"C:\$Recycle.Bin",
];

/// Check if a path is protected and should not be deleted
fn is_protected_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let path_upper = path_str.to_uppercase();

    // Check drive root (e.g., "C:\", "D:\")
    let normalized = path_upper.trim_end_matches('\\');
    if normalized.len() == 2 && normalized.ends_with(':') {
        return true;
    }

    // Check protected prefixes
    for protected in PROTECTED_PATH_PREFIXES {
        let protected_upper = protected.to_uppercase();
        // Match exact path or as prefix with backslash
        if path_upper == protected_upper
            || path_upper.starts_with(&format!("{}\\", protected_upper))
        {
            return true;
        }
    }

    false
}

/// Ask user for confirmation before proceeding
fn confirm_deletion(path: &Path, file_count: u64, total_size: u64) -> Result<bool> {
    let size_str = format_size(total_size);

    println!();
    println!("  {} {}", color::warn("target:"), path.display());
    println!(
        "  {} {} files, {}",
        color::warn("size:"),
        file_count,
        size_str
    );
    println!();
    print!("  {} ", color::warn("delete? [y/N]:"));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Count files and total size in a directory
fn count_contents(path: &Path) -> Result<(u64, u64)> {
    let mut file_count = 0u64;
    let mut total_size = 0u64;

    if path.is_file() {
        let meta = fs::metadata(path)?;
        return Ok((1, meta.len()));
    }

    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();

            if entry_path.is_file() {
                file_count += 1;
                if let Ok(meta) = fs::metadata(&entry_path) {
                    total_size += meta.len();
                }
            } else if entry_path.is_dir() {
                let (sub_count, sub_size) = count_contents(&entry_path)?;
                file_count += sub_count;
                total_size += sub_size;
            }
        }
    }

    Ok((file_count, total_size))
}

/// Recursively delete a path (file or directory)
fn delete_recursive(
    path: &Path,
    dry_run: bool,
    verbose: bool,
    summary: &mut DeleteSummary,
) -> Result<()> {
    if path.is_file() {
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        if verbose {
            println!("  {} {}", color::error("-"), path.display());
        }

        if !dry_run {
            fs::remove_file(path)?;
        }

        summary.add_file(size);
    } else if path.is_dir() {
        // First, recursively delete contents
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            delete_recursive(&entry.path(), dry_run, verbose, summary)?;
        }

        if verbose {
            println!("  {} {}/", color::error("-"), path.display());
        }

        if !dry_run {
            fs::remove_dir(path)?;
        }

        summary.add_dir();
    }

    Ok(())
}

/// Delete a file or directory recursively with safety checks
///
/// # Arguments
/// * `path` - Path to delete (file or directory)
/// * `force` - Skip confirmation prompt
/// * `dry_run` - Show what would be deleted without actually deleting
/// * `verbose` - Print each file/directory as it's deleted
pub fn delete(path: &str, force: bool, dry_run: bool, verbose: bool) -> Result<DeleteSummary> {
    // Resolve to absolute path
    let path = PathBuf::from(path);
    let abs_path = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()?.join(&path)
    };

    // Check for protected paths BEFORE checking existence
    // (prevents timing attacks and handles non-canonicalized paths)
    if is_protected_path(&abs_path) {
        bail!("refusing to delete protected path: {}", abs_path.display());
    }

    // Check if path exists
    if !abs_path.exists() {
        bail!("path not found: {}", abs_path.display());
    }

    // Canonicalize and check protection again with resolved path
    let abs_path = abs_path.canonicalize().unwrap_or(abs_path);
    if is_protected_path(&abs_path) {
        bail!("refusing to delete protected path: {}", abs_path.display());
    }

    // Count contents for confirmation
    let (file_count, total_size) = count_contents(&abs_path)?;

    // Confirm unless forced
    if !force && !dry_run {
        if !confirm_deletion(&abs_path, file_count, total_size)? {
            utils::print_info("cancelled");
            return Ok(DeleteSummary::default());
        }
    }

    // Show dry-run header
    if dry_run {
        println!();
        println!(
            "  {} (dry-run, nothing will be deleted)",
            color::warn("preview")
        );
        println!();
    }

    // Perform deletion
    let mut summary = DeleteSummary::default();
    delete_recursive(&abs_path, dry_run, verbose || dry_run, &mut summary)?;

    Ok(summary)
}

/// Format bytes as human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Format the delete summary for display
pub fn format_summary(summary: &DeleteSummary) -> String {
    format!(
        "deleted {} files, {} directories ({})",
        summary.files_deleted,
        summary.dirs_deleted,
        format_size(summary.bytes_freed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_protected_paths() {
        // System directories should be protected
        assert!(is_protected_path(Path::new(r"C:\Windows")));
        assert!(is_protected_path(Path::new(r"C:\Windows\System32")));
        assert!(is_protected_path(Path::new(r"C:\Program Files")));
        assert!(is_protected_path(Path::new(r"C:\Program Files\SomeApp")));

        // Drive roots should be protected
        assert!(is_protected_path(Path::new(r"C:\")));
        assert!(is_protected_path(Path::new(r"C:")));
        assert!(is_protected_path(Path::new(r"D:\")));
        assert!(is_protected_path(Path::new(r"D:")));

        // User directories should NOT be protected (except Default/Public)
        assert!(!is_protected_path(Path::new(r"C:\Users\testuser\projects")));
        assert!(!is_protected_path(Path::new(
            r"C:\Users\testuser\Downloads"
        )));
        assert!(!is_protected_path(Path::new(r"D:\temp")));
        assert!(!is_protected_path(Path::new(r"D:\projects\myapp")));

        // But Default and Public users should be protected
        assert!(is_protected_path(Path::new(r"C:\Users\Default")));
        assert!(is_protected_path(Path::new(r"C:\Users\Public")));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 bytes");
        assert_eq!(format_size(512), "512 bytes");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_delete_empty_dir() {
        let dir = tempdir().unwrap();
        let test_dir = dir.path().join("empty");
        fs::create_dir(&test_dir).unwrap();

        let summary = delete(test_dir.to_str().unwrap(), true, false, false).unwrap();

        assert_eq!(summary.dirs_deleted, 1);
        assert_eq!(summary.files_deleted, 0);
        assert!(!test_dir.exists());
    }

    #[test]
    fn test_delete_dir_with_files() {
        let dir = tempdir().unwrap();
        let test_dir = dir.path().join("with_files");
        fs::create_dir(&test_dir).unwrap();

        // Create some test files
        File::create(test_dir.join("file1.txt")).unwrap();
        File::create(test_dir.join("file2.txt")).unwrap();
        fs::create_dir(test_dir.join("subdir")).unwrap();
        File::create(test_dir.join("subdir").join("file3.txt")).unwrap();

        let summary = delete(test_dir.to_str().unwrap(), true, false, false).unwrap();

        assert_eq!(summary.files_deleted, 3);
        assert_eq!(summary.dirs_deleted, 2); // test_dir + subdir
        assert!(!test_dir.exists());
    }

    #[test]
    fn test_dry_run() {
        let dir = tempdir().unwrap();
        let test_dir = dir.path().join("dry_run_test");
        fs::create_dir(&test_dir).unwrap();
        File::create(test_dir.join("file.txt")).unwrap();

        let summary = delete(test_dir.to_str().unwrap(), true, true, false).unwrap();

        // Dry run should count but not delete
        assert_eq!(summary.files_deleted, 1);
        assert!(test_dir.exists()); // Should still exist!
    }

    #[test]
    fn test_reject_protected_path() {
        let result = delete(r"C:\Windows", true, false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("protected"));
    }

    #[test]
    fn test_nonexistent_path() {
        let result = delete(r"C:\this\path\does\not\exist\12345", true, false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
