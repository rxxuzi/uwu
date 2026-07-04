//! Toast notification module for uwu.
//!
//! Sends a Windows toast notification via the WinRT ToastNotificationManager.
//! Useful for long-running jobs, e.g. `cargo build; uwu notify done`.
//!
//! Implemented through a PowerShell subprocess (same approach as `wdex`) so no
//! app registration is required — we borrow PowerShell's registered AppUserModelID
//! so the toast is allowed to show on Windows 10/11.

use anyhow::{bail, Context, Result};

/// PowerShell's registered AppUserModelID. Reusing it lets toasts appear
/// without registering a dedicated Start-menu shortcut for uwu.
#[cfg(windows)]
const APP_ID: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// Send a toast notification with the given title and message.
///
/// # Arguments
/// * `title` - The bold heading line of the toast
/// * `message` - The body text of the toast
#[cfg(windows)]
pub fn send(title: &str, message: &str) -> Result<()> {
    use std::process::Command;

    let script = build_toast_script(title, message);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .context("Failed to execute PowerShell")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("failed to send notification: {}", error.trim());
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn send(_title: &str, _message: &str) -> Result<()> {
    bail!("notify is only supported on Windows");
}

/// Escape a string for embedding inside a PowerShell single-quoted literal.
/// Single quotes are doubled; control characters (newlines/tabs) are flattened
/// to spaces so a multi-line message cannot break the toast layout or the script.
fn ps_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\'' => "''".to_string(),
            '\r' | '\n' | '\t' => " ".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// Build the PowerShell script that shows a ToastText02 (title + body) toast.
///
/// The title/message are inserted as PowerShell single-quoted literals (with
/// quotes doubled), then passed to `CreateTextNode`, which performs XML escaping —
/// so neither PowerShell nor XML injection is possible from user input.
#[cfg(windows)]
fn build_toast_script(title: &str, message: &str) -> String {
    format!(
        "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
         $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
         $x = $t.GetElementsByTagName('text'); \
         $x.Item(0).AppendChild($t.CreateTextNode('{title}')) | Out-Null; \
         $x.Item(1).AppendChild($t.CreateTextNode('{message}')) | Out-Null; \
         $toast = [Windows.UI.Notifications.ToastNotification]::new($t); \
         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{app_id}').Show($toast);",
        title = ps_escape(title),
        message = ps_escape(message),
        app_id = APP_ID.replace('\'', "''"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_plain_text_unchanged() {
        assert_eq!(ps_escape("build done"), "build done");
    }

    #[test]
    fn escape_doubles_single_quotes() {
        assert_eq!(ps_escape("it's done"), "it''s done");
    }

    #[test]
    fn escape_flattens_newlines_and_tabs() {
        assert_eq!(ps_escape("a\nb\tc\rd"), "a b c d");
    }

    #[test]
    fn escape_preserves_unicode() {
        // Japanese / emoji must survive intact (UTF-8 all the way through)
        assert_eq!(ps_escape("ビルド完了 🎉"), "ビルド完了 🎉");
    }

    #[cfg(windows)]
    #[test]
    fn script_contains_escaped_content() {
        let script = build_toast_script("uwu", "it's done");
        assert!(script.contains("CreateTextNode('uwu')"));
        assert!(script.contains("CreateTextNode('it''s done')"));
        assert!(script.contains("ToastText02"));
    }

    #[cfg(windows)]
    #[test]
    fn script_uses_powershell_app_id() {
        let script = build_toast_script("t", "m");
        assert!(script.contains("WindowsPowerShell"));
        assert!(script.contains("CreateToastNotifier"));
    }
}
