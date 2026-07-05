//! Elevation module for uwu.
//!
//! `uwu admin` relaunches an elevated PowerShell in the current directory —
//! the CLI equivalent of right-clicking "Run as administrator". Uses
//! ShellExecute with the `runas` verb to trigger the UAC prompt.

use anyhow::{bail, Result};

use crate::utils;

#[cfg(windows)]
pub fn run() -> Result<()> {
    if utils::is_elevated() {
        println!();
        utils::print_info("already running as administrator");
        return Ok(());
    }

    let cwd = std::env::current_dir()?;

    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // Working directory for the elevated shell (null-terminated UTF-16).
    let dir: Vec<u16> = cwd
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            w!("powershell.exe"),
            PCWSTR::null(),
            PCWSTR::from_raw(dir.as_ptr()),
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW returns a value > 32 on success.
    if result.0 <= 32 {
        bail!("failed to elevate (UAC declined or cancelled)");
    }

    println!();
    utils::print_success("opened an elevated PowerShell in this directory");
    Ok(())
}

#[cfg(not(windows))]
pub fn run() -> Result<()> {
    bail!("admin is only supported on Windows")
}
