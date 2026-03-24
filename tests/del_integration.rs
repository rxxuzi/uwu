//! Integration tests for the del command
//!
//! These tests verify the del command works correctly through the CLI interface.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

/// Get the path to the compiled binary
fn get_binary_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test binary name
    path.pop(); // Remove deps
    path.push("uwu");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

/// Helper to create a test directory structure
fn create_test_structure(base: &std::path::Path) -> std::path::PathBuf {
    let test_dir = base.join("test_delete_me");
    fs::create_dir_all(&test_dir).unwrap();

    // Create some files
    let mut f1 = File::create(test_dir.join("file1.txt")).unwrap();
    writeln!(f1, "Hello, World!").unwrap();

    let mut f2 = File::create(test_dir.join("file2.txt")).unwrap();
    writeln!(f2, "Test content").unwrap();

    // Create subdirectory with files
    let subdir = test_dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let mut f3 = File::create(subdir.join("nested.txt")).unwrap();
    writeln!(f3, "Nested content").unwrap();

    test_dir
}

#[test]
fn test_del_with_force_flag() {
    let dir = tempdir().unwrap();
    let test_dir = create_test_structure(dir.path());

    let output = Command::new(get_binary_path())
        .args(["del", test_dir.to_str().unwrap(), "-f", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    assert!(!test_dir.exists(), "Directory should be deleted");
}

#[test]
fn test_del_dry_run() {
    let dir = tempdir().unwrap();
    let test_dir = create_test_structure(dir.path());

    let output = Command::new(get_binary_path())
        .args(["del", test_dir.to_str().unwrap(), "-n", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    assert!(
        test_dir.exists(),
        "Directory should still exist after dry-run"
    );

    // Verify files still exist
    assert!(test_dir.join("file1.txt").exists());
    assert!(test_dir.join("subdir/nested.txt").exists());
}

#[test]
fn test_del_verbose_output() {
    let dir = tempdir().unwrap();
    let test_dir = create_test_structure(dir.path());

    let output = Command::new(get_binary_path())
        .args(["del", test_dir.to_str().unwrap(), "-f", "-v", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verbose mode should show files being deleted
    assert!(
        stdout.contains("file1.txt") || stdout.contains("-"),
        "Verbose output should show files"
    );
}

#[test]
fn test_del_nonexistent_path() {
    let output = Command::new(get_binary_path())
        .args(["del", r"C:\nonexistent\path\12345", "-f", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "Command should fail for nonexistent path"
    );
}

#[test]
fn test_del_protected_path() {
    // Try to delete Windows directory (should be rejected)
    let output = Command::new(get_binary_path())
        .args(["del", r"C:\Windows", "-f", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "Command should fail for protected path"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("protected") || stderr.contains("refusing"),
        "Error message should mention protected path"
    );
}

#[test]
fn test_del_single_file() {
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("single_file.txt");

    let mut f = File::create(&test_file).unwrap();
    writeln!(f, "Single file content").unwrap();
    drop(f);

    let output = Command::new(get_binary_path())
        .args(["del", test_file.to_str().unwrap(), "-f", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    assert!(!test_file.exists(), "File should be deleted");
}

#[test]
fn test_del_empty_directory() {
    let dir = tempdir().unwrap();
    let empty_dir = dir.path().join("empty_dir");
    fs::create_dir(&empty_dir).unwrap();

    let output = Command::new(get_binary_path())
        .args(["del", empty_dir.to_str().unwrap(), "-f", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    assert!(!empty_dir.exists(), "Empty directory should be deleted");
}

#[test]
fn test_del_combined_flags() {
    let dir = tempdir().unwrap();
    let test_dir = create_test_structure(dir.path());

    // Test -n -v combined (dry-run + verbose)
    let output = Command::new(get_binary_path())
        .args(["del", test_dir.to_str().unwrap(), "-n", "-v", "-q"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);
    assert!(
        test_dir.exists(),
        "Directory should still exist after dry-run"
    );
}
