use colored::*;
use std::sync::atomic::{AtomicBool, Ordering};

static QUIET_MODE: AtomicBool = AtomicBool::new(false);

// パステルカラー定義
pub fn pink(text: &str) -> ColoredString {
    text.truecolor(255, 182, 193)
}

pub fn cyan(text: &str) -> ColoredString {
    text.truecolor(173, 216, 230)
}

pub fn mint(text: &str) -> ColoredString {
    text.truecolor(189, 252, 201)
}

pub fn lavender(text: &str) -> ColoredString {
    text.truecolor(230, 230, 250)
}

// 静かモードの設定
pub fn set_quiet_mode(quiet: bool) {
    QUIET_MODE.store(quiet, Ordering::Relaxed);
}

pub fn is_quiet() -> bool {
    QUIET_MODE.load(Ordering::Relaxed)
}

// メッセージ出力ヘルパー
pub fn print_success(message: &str) {
    if !is_quiet() {
        println!("  {} {}", mint("+"), message);
    }
}

pub fn print_error(message: &str) {
    eprintln!("  {} {}", pink("x"), message);
}

pub fn print_warn(message: &str) {
    if !is_quiet() {
        println!("  {} {}", lavender("!"), message);
    }
}

pub fn print_info(message: &str) {
    if !is_quiet() {
        println!("  {}", cyan(message));
    }
}

// Windows管理者権限チェック
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Security::*;
    use windows::Win32::Foundation::*;

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
        ).is_ok() {
            elevation.TokenIsElevated != 0
        } else {
            false
        }
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}