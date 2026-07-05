//! System volume module for uwu.
//!
//! Gets/sets the default audio output's master volume via the Windows Core
//! Audio API (`IAudioEndpointVolume`). Supports absolute levels (0-100),
//! relative steps (+N/-N), and mute/unmute/toggle.

use anyhow::{bail, Result};

use crate::{color, utils};

/// Parse a level argument against the current percentage.
/// Accepts `0-100`, `+N`, `-N`. Returns the resulting 0-100 level.
fn parse_target(arg: &str, current: u32) -> Result<u32> {
    let arg = arg.trim();
    if let Some(rest) = arg.strip_prefix('+') {
        let d: u32 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid amount: +{}", rest))?;
        Ok((current + d).min(100))
    } else if let Some(rest) = arg.strip_prefix('-') {
        let d: u32 = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid amount: -{}", rest))?;
        Ok(current.saturating_sub(d))
    } else {
        let v: u32 = arg
            .parse()
            .map_err(|_| anyhow::anyhow!("expected 0-100, +N, -N, mute, or unmute"))?;
        if v > 100 {
            bail!("volume must be between 0 and 100");
        }
        Ok(v)
    }
}

/// Render a small volume bar, e.g. `██████░░░░ 60%`.
fn bar(pct: u32) -> String {
    let filled = (pct as usize + 5) / 10; // round to nearest 10%
    let filled = filled.min(10);
    format!("{}{} {}%", "█".repeat(filled), "░".repeat(10 - filled), pct)
}

#[cfg(windows)]
pub fn run(level: Option<&str>) -> Result<()> {
    use anyhow::Context;
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        // COM init (RPC_E_CHANGED_MODE if already initialized elsewhere — harmless).
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("failed to create audio device enumerator")?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .context("no default audio output device")?;
        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .context("failed to access endpoint volume")?;

        let current_pct = || -> u32 {
            (endpoint.GetMasterVolumeLevelScalar().unwrap_or(0.0) * 100.0).round() as u32
        };
        let is_muted = || endpoint.GetMute().map(|b| b.as_bool()).unwrap_or(false);

        match level {
            None => {
                print_status(current_pct(), is_muted());
            }
            Some(arg) => match arg.trim().to_lowercase().as_str() {
                "mute" => {
                    endpoint.SetMute(BOOL::from(true), std::ptr::null())?;
                    println!();
                    utils::print_success("muted");
                }
                "unmute" => {
                    endpoint.SetMute(BOOL::from(false), std::ptr::null())?;
                    println!();
                    utils::print_success("unmuted");
                }
                "toggle" => {
                    let now = !is_muted();
                    endpoint.SetMute(BOOL::from(now), std::ptr::null())?;
                    println!();
                    utils::print_success(if now { "muted" } else { "unmuted" });
                }
                _ => {
                    let target = parse_target(arg, current_pct())?;
                    endpoint
                        .SetMasterVolumeLevelScalar(target as f32 / 100.0, std::ptr::null())
                        .context("failed to set volume")?;
                    // Setting a level implicitly unmutes.
                    if target > 0 {
                        let _ = endpoint.SetMute(BOOL::from(false), std::ptr::null());
                    }
                    println!();
                    utils::print_success(&format!("volume {}", bar(target)));
                }
            },
        }
    }

    Ok(())
}

#[cfg(windows)]
fn print_status(pct: u32, muted: bool) {
    println!();
    if muted {
        println!("  {} {}", color::warn("🔇"), color::note(&format!("muted ({}%)", pct)));
    } else {
        println!("  {} {}", color::info("🔊"), bar(pct));
    }
}

#[cfg(not(windows))]
pub fn run(_level: Option<&str>) -> Result<()> {
    bail!("sound is only supported on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_absolute() {
        assert_eq!(parse_target("50", 20).unwrap(), 50);
        assert_eq!(parse_target("0", 20).unwrap(), 0);
        assert_eq!(parse_target("100", 20).unwrap(), 100);
    }

    #[test]
    fn parse_relative_clamps() {
        assert_eq!(parse_target("+10", 95).unwrap(), 100); // clamp high
        assert_eq!(parse_target("-30", 20).unwrap(), 0); // clamp low
        assert_eq!(parse_target("+15", 50).unwrap(), 65);
    }

    #[test]
    fn parse_rejects_out_of_range_and_garbage() {
        assert!(parse_target("101", 0).is_err());
        assert!(parse_target("loud", 0).is_err());
    }

    #[test]
    fn bar_shape() {
        assert!(bar(0).starts_with("░"));
        assert!(bar(100).starts_with("██████████"));
        assert!(bar(50).contains("50%"));
    }
}
