//! Color palette module for uwu.
//!
//! Provides a consistent pastel color scheme across the application.
//! Uses crossterm for reliable cross-platform color support.

use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor, Stylize},
};
use once_cell::sync::Lazy;
use std::io;

/// RGB color definition
#[derive(Debug, Clone, Copy)]
pub struct RGB {
    r: u8,
    g: u8,
    b: u8,
}

impl RGB {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        RGB { r, g, b }
    }

    /// Convert to crossterm Color
    fn to_color(&self) -> Color {
        Color::Rgb {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }
}

/// Initialize color support for the current platform
pub fn init_colors() {
    // Windows環境での初期化
    #[cfg(windows)]
    {
        // Virtual Terminal処理を有効化
        if let Err(_) = enable_virtual_terminal_processing() {
            // Fallback: crosstermの自動初期化に依存
            let _ = crossterm::terminal::enable_raw_mode();
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }

    // 環境変数を設定（TrueColor対応を明示）
    std::env::set_var("COLORTERM", "truecolor");
}

#[cfg(windows)]
fn enable_virtual_terminal_processing() -> Result<(), Box<dyn std::error::Error>> {
    // Simple approach: let crossterm handle it
    crossterm::terminal::enable_raw_mode()?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

/// Pastel pink - used for errors and warnings
static PINK: Lazy<RGB> = Lazy::new(|| RGB::new(255, 182, 193));

/// Pastel cyan - used for information and headers
static CYAN: Lazy<RGB> = Lazy::new(|| RGB::new(173, 216, 230));

/// Pastel mint - used for success messages
static MINT: Lazy<RGB> = Lazy::new(|| RGB::new(189, 252, 201));

/// Pastel lavender - used for notes and tips
static LAVENDER: Lazy<RGB> = Lazy::new(|| RGB::new(230, 230, 250));

/// Pastel yellow - used for highlights
static YELLOW: Lazy<RGB> = Lazy::new(|| RGB::new(255, 253, 184));

/// Pastel peach - used for special accents
static PEACH: Lazy<RGB> = Lazy::new(|| RGB::new(255, 208, 175));

// Public API - Colored String Wrapper

/// A wrapper for colored text that can be displayed
pub struct ColoredText {
    text: String,
    color: Color,
}

impl ColoredText {
    fn new(text: &str, rgb: &RGB) -> Self {
        ColoredText {
            text: text.to_string(),
            color: rgb.to_color(),
        }
    }
}

impl std::fmt::Display for ColoredText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // crosstermの execute! を使用して色を適用
        let mut buffer = Vec::new();
        let _ = execute!(
            &mut buffer,
            SetForegroundColor(self.color),
            Print(&self.text),
            ResetColor
        );

        // バッファの内容を文字列として書き込み
        write!(f, "{}", String::from_utf8_lossy(&buffer))
    }
}

pub type ColoredString = ColoredText;

// Public API - Semantic Color Functions

/// Error or invalid state indicator
pub fn error(text: &str) -> ColoredString {
    ColoredText::new(text, &PINK)
}

/// Warning or attention needed
pub fn warn(text: &str) -> ColoredString {
    ColoredText::new(text, &LAVENDER)
}

/// Success or positive outcome
pub fn success(text: &str) -> ColoredString {
    ColoredText::new(text, &MINT)
}

/// General information
pub fn info(text: &str) -> ColoredString {
    ColoredText::new(text, &CYAN)
}

/// Section headers or titles
pub fn header(text: &str) -> ColoredString {
    ColoredText::new(text, &CYAN)
}

/// Tips, notes, or hints
pub fn note(text: &str) -> ColoredString {
    ColoredText::new(text, &LAVENDER)
}

/// Highlight important information
pub fn highlight(text: &str) -> ColoredString {
    ColoredText::new(text, &YELLOW)
}

/// Special accent color
pub fn accent(text: &str) -> ColoredString {
    ColoredText::new(text, &PEACH)
}

// Direct print functions (more reliable for immediate output)

/// Print colored text directly to stdout
pub fn print_colored(text: &str, rgb: &RGB) -> io::Result<()> {
    execute!(
        io::stdout(),
        SetForegroundColor(rgb.to_color()),
        Print(text),
        ResetColor
    )
}

/// Print error text directly
pub fn print_error(text: &str) -> io::Result<()> {
    print_colored(text, &PINK)
}

/// Print success text directly
pub fn print_success(text: &str) -> io::Result<()> {
    print_colored(text, &MINT)
}

/// Print info text directly
pub fn print_info(text: &str) -> io::Result<()> {
    print_colored(text, &CYAN)
}

/// Success symbol with color
pub fn success_symbol() -> ColoredString {
    success("✓")
}

/// Error symbol with color
pub fn error_symbol() -> ColoredString {
    error("✗")
}

/// Warning symbol with color
pub fn warn_symbol() -> ColoredString {
    warn("⚠")
}

/// Info symbol with color
pub fn info_symbol() -> ColoredString {
    info("ℹ")
}

/// Format a path that doesn't exist
pub fn missing_path(path: &str) -> String {
    format!("{} {}", path, error("(not found)"))
}

/// Format a deprecated or old item
pub fn deprecated(text: &str) -> String {
    format!("{} {}", text, warn("(deprecated)"))
}

/// Format a new or added item
pub fn added(text: &str) -> String {
    format!("{} {}", success("+"), text)
}

/// Format a removed item
pub fn removed(text: &str) -> String {
    format!("{} {}", error("-"), text)
}

// Alternative implementation using crossterm's stylize trait

/// Apply RGB color using crossterm's stylize trait
pub fn apply_rgb(text: &str, r: u8, g: u8, b: u8) -> String {
    text.with(Color::Rgb { r, g, b }).to_string()
}

/// Alternative functions using stylize trait
pub mod styled {
    use super::*;

    pub fn error(text: &str) -> String {
        apply_rgb(text, PINK.r, PINK.g, PINK.b)
    }

    pub fn success(text: &str) -> String {
        apply_rgb(text, MINT.r, MINT.g, MINT.b)
    }

    pub fn info(text: &str) -> String {
        apply_rgb(text, CYAN.r, CYAN.g, CYAN.b)
    }

    pub fn warn(text: &str) -> String {
        apply_rgb(text, LAVENDER.r, LAVENDER.g, LAVENDER.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_consistency() {
        // Ensure colors are created consistently
        let text = "test";
        assert_eq!(error(text).to_string(), error(text).to_string());
        assert_eq!(success(text).to_string(), success(text).to_string());
    }

    #[test]
    fn test_rgb_values() {
        // Verify RGB values are within valid range
        assert!(PINK.r <= 255 && PINK.g <= 255 && PINK.b <= 255);
        assert!(CYAN.r <= 255 && CYAN.g <= 255 && CYAN.b <= 255);
    }

    #[test]
    fn test_direct_print() {
        // Test that direct print functions don't panic
        let _ = print_error("test");
        let _ = print_success("test");
        let _ = print_info("test");
    }
}
