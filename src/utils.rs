//! Utility module for uwu.
//!
//! Provides global settings and message printing utilities.

use crate::color;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global quiet mode flag
static QUIET_MODE: AtomicBool = AtomicBool::new(false);

/// Set the global quiet mode
pub fn set_quiet_mode(quiet: bool) {
    QUIET_MODE.store(quiet, Ordering::Relaxed);
}

/// Check if quiet mode is enabled
pub fn is_quiet() -> bool {
    QUIET_MODE.load(Ordering::Relaxed)
}

/// Print a success message with appropriate formatting
pub fn print_success(message: &str) {
    if !is_quiet() {
        println!("  {} {}", color::success_symbol(), message);
    }
}

/// Print an error message (always shown, even in quiet mode)
pub fn print_error(message: &str) {
    eprintln!("  {} {}", color::error_symbol(), message);
}

/// Print a warning message
pub fn print_warn(message: &str) {
    if !is_quiet() {
        println!("  {} {}", color::warn_symbol(), message);
    }
}

/// Print an info message
pub fn print_info(message: &str) {
    if !is_quiet() {
        println!("  {}", color::info(message));
    }
}

// Platform-specific Utilities

/// Check if the current process has administrator/elevated privileges
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::*;
    use windows::Win32::Security::*;

    unsafe {
        let mut elevation = TOKEN_ELEVATION::default();
        let token_handle = HANDLE::default();
        let mut bytes_needed = 0u32;

        if GetTokenInformation(
            token_handle,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut bytes_needed,
        )
        .is_ok()
        {
            elevation.TokenIsElevated != 0
        } else {
            false
        }
    }
}

/// Check if the current process has administrator/elevated privileges (non-Windows)
#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}
