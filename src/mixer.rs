//! Per-application volume mixer (TUI) for uwu.
//!
//! Lists each app currently playing audio (via the Windows Core Audio session
//! API) and lets you adjust per-app volume and mute from an interactive
//! terminal UI — the Volume Mixer, without leaving the terminal.

use anyhow::Result;

#[cfg(windows)]
pub fn run(list: bool) -> Result<()> {
    imp::run(list)
}

#[cfg(not(windows))]
pub fn run(_list: bool) -> Result<()> {
    anyhow::bail!("mixer is only supported on Windows")
}

/// Pad or truncate a name to a fixed display width.
#[cfg(windows)]
fn pad(name: &str, width: usize) -> String {
    let count = name.chars().count();
    if count > width {
        let head: String = name.chars().take(width - 1).collect();
        format!("{}…", head)
    } else {
        format!("{:<width$}", name, width = width)
    }
}

/// A 10-cell volume bar.
#[cfg(windows)]
fn bar(pct: u32) -> String {
    let filled = ((pct as usize + 5) / 10).min(10);
    format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
}

#[cfg(windows)]
mod imp {
    use super::{bar, pad};
    use anyhow::{Context, Result};
    use std::collections::BTreeMap;
    use std::io::{stdout, Write};
    use std::time::Duration;

    use crossterm::{
        cursor::{Hide, MoveTo, Show},
        event::{self, Event, KeyCode, KeyEventKind},
        execute, queue,
        style::{Color, Print, ResetColor, SetForegroundColor},
        terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use windows::core::ComInterface;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    use crate::{color, utils};

    /// One mixer row: an app (grouped by PID) and its audio-volume controls.
    struct Session {
        name: String,
        volumes: Vec<ISimpleAudioVolume>,
    }

    impl Session {
        fn volume(&self) -> f32 {
            unsafe { self.volumes[0].GetMasterVolume().unwrap_or(0.0) }
        }
        fn muted(&self) -> bool {
            unsafe { self.volumes[0].GetMute().map(|b| b.as_bool()).unwrap_or(false) }
        }
        fn set_volume(&self, v: f32) {
            let v = v.clamp(0.0, 1.0);
            for vol in &self.volumes {
                unsafe {
                    let _ = vol.SetMasterVolume(v, std::ptr::null());
                }
            }
        }
        fn toggle_mute(&self) {
            let target = !self.muted();
            for vol in &self.volumes {
                unsafe {
                    let _ = vol.SetMute(target, std::ptr::null());
                }
            }
        }
    }

    pub fn run(list: bool) -> Result<()> {
        let sessions = unsafe { collect_sessions()? };

        if sessions.is_empty() {
            println!();
            utils::print_info("no apps are playing audio right now");
            return Ok(());
        }

        // Non-interactive listing (also handy for scripts).
        if list {
            println!();
            for s in &sessions {
                let pct = (s.volume() * 100.0).round() as u32;
                let muted = if s.muted() { "  muted" } else { "" };
                println!(
                    "  {} {} {:>3}%{}",
                    color::accent(&pad(&s.name, 16)),
                    bar(pct),
                    pct,
                    muted
                );
            }
            return Ok(());
        }

        // Enter the TUI, ensuring the terminal is always restored afterwards.
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, Hide)?;

        let result = event_loop(&mut out, &sessions);

        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
        result
    }

    fn event_loop(out: &mut impl Write, sessions: &[Session]) -> Result<()> {
        let mut selected = 0usize;
        const STEP: f32 = 0.05;

        loop {
            draw(out, sessions, selected)?;

            // Poll so external volume changes refresh the view periodically.
            if !event::poll(Duration::from_millis(500))? {
                continue;
            }
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if selected + 1 < sessions.len() {
                            selected += 1;
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        let s = &sessions[selected];
                        s.set_volume(s.volume() - STEP);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        let s = &sessions[selected];
                        s.set_volume(s.volume() + STEP);
                    }
                    KeyCode::Char('m') => sessions[selected].toggle_mute(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn draw(out: &mut impl Write, sessions: &[Session], selected: usize) -> Result<()> {
        queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
        queue!(
            out,
            SetForegroundColor(Color::Rgb { r: 173, g: 216, b: 230 }),
            Print("  uwu mixer"),
            ResetColor,
            Print("   ↑↓ select · ←→ volume · m mute · q quit"),
        )?;

        for (i, s) in sessions.iter().enumerate() {
            let row = (i + 2) as u16;
            let pct = (s.volume() * 100.0).round() as u32;
            let marker = if i == selected { "›" } else { " " };
            let muted = if s.muted() { "  muted" } else { "" };
            let text = format!("  {} {} {} {:>3}%{}", marker, pad(&s.name, 16), bar(pct), pct, muted);

            queue!(out, MoveTo(0, row))?;
            if i == selected {
                queue!(
                    out,
                    SetForegroundColor(Color::Rgb { r: 255, g: 208, b: 175 }),
                    Print(text),
                    ResetColor
                )?;
            } else {
                queue!(out, Print(text))?;
            }
        }

        let _ = color::info; // keep color module referenced for future styling
        out.flush()?;
        Ok(())
    }

    /// Enumerate active audio sessions grouped by process, with friendly names.
    unsafe fn collect_sessions() -> Result<Vec<Session>> {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("failed to create audio device enumerator")?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .context("no default audio output device")?;
        let manager: IAudioSessionManager2 = device
            .Activate(CLSCTX_ALL, None)
            .context("failed to access the session manager")?;

        let list = manager
            .GetSessionEnumerator()
            .context("failed to enumerate audio sessions")?;
        let count = list.GetCount().unwrap_or(0);

        let names = process_names();

        // Group sessions by PID so multi-session apps (e.g. browsers) act as one row.
        let mut groups: BTreeMap<u32, (String, Vec<ISimpleAudioVolume>)> = BTreeMap::new();
        for i in 0..count {
            let ctrl = match list.GetSession(i) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let ctrl2: IAudioSessionControl2 = match ctrl.cast() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let volume: ISimpleAudioVolume = match ctrl.cast() {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Name by owning process. The system-sounds session reports PID 0.
            // (IsSystemSoundsSession can't be used here: it returns S_FALSE for
            // real apps, which windows-rs still maps to Ok — so it's always true.)
            let pid = ctrl2.GetProcessId().unwrap_or(0);
            let name = if pid == 0 {
                // PID 0 is the system-sounds session ("[System Process]" in Toolhelp).
                "(system sounds)".to_string()
            } else {
                names
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| format!("pid {}", pid))
            };

            groups.entry(pid).or_insert((name, Vec::new())).1.push(volume);
        }

        let mut sessions: Vec<Session> = groups
            .into_values()
            .map(|(name, volumes)| Session { name, volumes })
            .collect();
        sessions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(sessions)
    }

    /// Map PID -> process name (without `.exe`) via a Toolhelp snapshot.
    unsafe fn process_names() -> std::collections::HashMap<u32, String> {
        use std::collections::HashMap;
        use std::mem::size_of;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };

        let mut map = HashMap::new();
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(s) => s,
            Err(_) => return map,
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let mut name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if let Some(stripped) = name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")) {
                    name = stripped.to_string();
                }
                map.insert(entry.th32ProcessID, name);
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        map
    }
}
