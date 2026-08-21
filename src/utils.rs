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

/// Check if the current process has administrator/elevated privileges.
///
/// The process token has to be opened explicitly — querying elevation through a
/// default (null) handle always fails, which would report every process as
/// unelevated and send self-elevating commands into a re-launch loop.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut bytes_needed = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut bytes_needed,
        )
        .is_ok();

        let _ = CloseHandle(token);
        queried && elevation.TokenIsElevated != 0
    }
}

/// Check if the current process has administrator/elevated privileges (non-Windows)
#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}
