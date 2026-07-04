//! Process termination module for uwu.
//!
//! Kills processes by name (glob), TCP port, or PID — natively via the Win32
//! API (Toolhelp for enumeration, IpHelper for port→PID, TerminateProcess to
//! kill). Follows uwu's destructive-command conventions: it lists matches,
//! confirms before killing (unless `-f`), supports `-n` dry-run, and refuses to
//! touch critical system processes.

use anyhow::{bail, Result};
use std::io::{self, Write};

use crate::{color, utils};

/// How the user selected the target process(es).
pub enum Selector {
    Name(String),
    Port(u16),
    Pid(u32),
}

/// A process matched for termination.
struct Match {
    pid: u32,
    name: String,
    port: Option<u16>,
}

/// Critical processes that must never be killed (would crash/BSOD Windows).
const PROTECTED: &[&str] = &[
    "system idle process",
    "system",
    "registry",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
];

/// Resolve the CLI arguments into a single selector.
///
/// A bare numeric `target` is treated as a **port** (uwu's headline
/// "address already in use" fix); explicit `--pid`/`--port`/`--name` win.
pub fn resolve_selector(
    target: Option<String>,
    port: Option<u16>,
    pid: Option<u32>,
    name: Option<String>,
) -> Result<Selector> {
    let sources = target.is_some() as u8
        + port.is_some() as u8
        + pid.is_some() as u8
        + name.is_some() as u8;
    if sources == 0 {
        bail!("specify a target: a name, a port, or --pid");
    }
    if sources > 1 {
        bail!("specify only one target (name, port, or pid)");
    }

    if let Some(p) = pid {
        return Ok(Selector::Pid(p));
    }
    if let Some(p) = port {
        return Ok(Selector::Port(p));
    }
    if let Some(n) = name {
        return Ok(Selector::Name(n));
    }

    // Positional: numeric -> port, otherwise a name/glob.
    let t = target.unwrap();
    match t.parse::<u16>() {
        Ok(p) => Ok(Selector::Port(p)),
        Err(_) => Ok(Selector::Name(t)),
    }
}

/// Kill processes matching `selector`.
pub fn run(selector: Selector, force: bool, dry_run: bool) -> Result<()> {
    let matches = find_matches(&selector)?;

    if matches.is_empty() {
        utils::print_info(&describe_empty(&selector));
        return Ok(());
    }

    // Show what we found.
    println!("  {}:", color::info("matched"));
    for m in &matches {
        match m.port {
            Some(port) => println!(
                "    {} {} {}",
                color::accent(&m.pid.to_string()),
                m.name,
                color::note(&format!("(port {})", port))
            ),
            None => println!("    {} {}", color::accent(&m.pid.to_string()), m.name),
        }
    }

    if dry_run {
        println!();
        utils::print_info(&format!(
            "dry run: {} process(es) would be killed",
            matches.len()
        ));
        return Ok(());
    }

    // Confirm unless forced.
    if !force {
        println!();
        print!(
            "  {} kill {} process(es)? [y/N]: ",
            color::warn("!"),
            matches.len()
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            utils::print_info("cancelled.");
            return Ok(());
        }
    }

    // Terminate.
    let mut killed = 0;
    let mut denied = false;
    for m in &matches {
        match terminate(m.pid) {
            Ok(()) => {
                killed += 1;
                utils::print_success(&format!("killed {} ({})", m.name, m.pid));
            }
            Err(e) => {
                if is_access_denied(&e) {
                    denied = true;
                }
                utils::print_error(&format!("failed to kill {} ({}): {}", m.name, m.pid, e));
            }
        }
    }

    if denied {
        println!(
            "  {} some kills were denied — try running as administrator",
            color::note("note:")
        );
    }
    if killed == 0 {
        bail!("no processes were killed");
    }

    Ok(())
}

/// Find processes matching the selector, excluding protected/self processes.
fn find_matches(selector: &Selector) -> Result<Vec<Match>> {
    let procs = sys::list_processes()?;
    let me = std::process::id();

    let mut matches: Vec<Match> = match selector {
        Selector::Pid(pid) => procs
            .into_iter()
            .filter(|p| p.pid == *pid)
            .map(|p| Match {
                pid: p.pid,
                name: p.name,
                port: None,
            })
            .collect(),

        Selector::Name(pattern) => procs
            .into_iter()
            .filter(|p| name_matches(pattern, &p.name))
            .map(|p| Match {
                pid: p.pid,
                name: p.name,
                port: None,
            })
            .collect(),

        Selector::Port(port) => {
            let pids = sys::port_to_pids(*port)?;
            pids.into_iter()
                .map(|pid| {
                    let name = procs
                        .iter()
                        .find(|p| p.pid == pid)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "<unknown>".to_string());
                    Match {
                        pid,
                        name,
                        port: Some(*port),
                    }
                })
                .collect()
        }
    };

    // Drop protected processes and ourselves.
    matches.retain(|m| {
        if m.pid == me {
            return false;
        }
        if is_protected(&m.name) {
            utils::print_warn(&format!("skipping protected process: {} ({})", m.name, m.pid));
            return false;
        }
        true
    });

    Ok(matches)
}

/// Human-readable "nothing matched" message.
fn describe_empty(selector: &Selector) -> String {
    match selector {
        Selector::Pid(pid) => format!("no process with pid {}", pid),
        Selector::Name(name) => format!("no process matching '{}'", name),
        Selector::Port(port) => format!("no process listening on port {}", port),
    }
}

/// Terminate a process by PID.
fn terminate(pid: u32) -> Result<()> {
    sys::terminate(pid)
}

fn is_access_denied(err: &anyhow::Error) -> bool {
    err.to_string().to_lowercase().contains("denied")
}

/// Is `name` a protected system process? (case-insensitive, `.exe` optional)
fn is_protected(name: &str) -> bool {
    let base = strip_exe(name).to_lowercase();
    PROTECTED.iter().any(|p| strip_exe(p) == base)
}

/// Match a process name against a glob pattern (case-insensitive, `.exe` optional).
fn name_matches(pattern: &str, name: &str) -> bool {
    glob_match(&strip_exe(pattern), &strip_exe(name))
}

/// Strip a trailing `.exe` (case-insensitive).
fn strip_exe(name: &str) -> String {
    let lower = name.to_lowercase();
    lower
        .strip_suffix(".exe")
        .map(|s| s.to_string())
        .unwrap_or(lower)
}

/// Glob matcher supporting `*` and `?`, case-insensitive.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// A running process.
struct Proc {
    pid: u32,
    name: String,
}

#[cfg(windows)]
mod sys {
    use super::*;
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    };
    use windows::Win32::Networking::WinSock::AF_INET;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    /// Enumerate all running processes.
    pub fn list_processes() -> Result<Vec<Proc>> {
        let mut out = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|e| anyhow::anyhow!("CreateToolhelp32Snapshot failed: {e}"))?;

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
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    out.push(Proc {
                        pid: entry.th32ProcessID,
                        name,
                    });
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        Ok(out)
    }

    /// Map a local TCP port to the owning process IDs (any connection state).
    pub fn port_to_pids(port: u16) -> Result<Vec<u32>> {
        let mut pids = Vec::new();
        unsafe {
            // First call: get required buffer size.
            let mut size: u32 = 0;
            GetExtendedTcpTable(
                None,
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if size == 0 {
                return Ok(pids);
            }

            let mut buf = vec![0u8; size as usize];
            let ret = GetExtendedTcpTable(
                Some(buf.as_mut_ptr() as *mut c_void),
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if ret != 0 {
                bail!("GetExtendedTcpTable failed (code {ret})");
            }

            let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
            let rows =
                std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
            for row in rows {
                // dwLocalPort holds the port in network byte order in its low 16 bits.
                let host_port = (row.dwLocalPort as u16).swap_bytes();
                if host_port == port && !pids.contains(&row.dwOwningPid) {
                    pids.push(row.dwOwningPid);
                }
            }
        }
        Ok(pids)
    }

    /// Terminate a process by PID.
    pub fn terminate(pid: u32) -> Result<()> {
        unsafe {
            let handle: HANDLE = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let result = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
            result.map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod sys {
    use super::*;

    pub fn list_processes() -> Result<Vec<Proc>> {
        bail!("kill is only supported on Windows")
    }
    pub fn port_to_pids(_port: u16) -> Result<Vec<u32>> {
        bail!("kill is only supported on Windows")
    }
    pub fn terminate(_pid: u32) -> Result<()> {
        bail!("kill is only supported on Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_exact_and_wildcards() {
        assert!(glob_match("node", "node"));
        assert!(glob_match("*server", "myserver"));
        assert!(glob_match("chrome*", "chromedriver"));
        assert!(glob_match("py?hon", "python"));
        assert!(!glob_match("node", "nodejs"));
        assert!(!glob_match("*server", "servers"));
    }

    #[test]
    fn name_match_ignores_exe_and_case() {
        assert!(name_matches("node", "node.exe"));
        assert!(name_matches("NODE", "node.exe"));
        assert!(name_matches("*server", "MyServer.exe"));
        assert!(!name_matches("node", "notepad.exe"));
    }

    #[test]
    fn protected_processes_detected() {
        assert!(is_protected("csrss.exe"));
        assert!(is_protected("LSASS.EXE"));
        assert!(is_protected("System"));
        assert!(!is_protected("node.exe"));
        assert!(!is_protected("chrome.exe"));
    }

    #[test]
    fn resolve_bare_number_is_port() {
        assert!(matches!(
            resolve_selector(Some("3000".into()), None, None, None).unwrap(),
            Selector::Port(3000)
        ));
    }

    #[test]
    fn resolve_bare_word_is_name() {
        assert!(matches!(
            resolve_selector(Some("node".into()), None, None, None).unwrap(),
            Selector::Name(_)
        ));
    }

    #[test]
    fn resolve_explicit_flags() {
        assert!(matches!(
            resolve_selector(None, None, Some(1234), None).unwrap(),
            Selector::Pid(1234)
        ));
        assert!(matches!(
            resolve_selector(None, Some(8080), None, None).unwrap(),
            Selector::Port(8080)
        ));
    }

    #[test]
    fn resolve_rejects_none_and_multiple() {
        assert!(resolve_selector(None, None, None, None).is_err());
        assert!(resolve_selector(Some("x".into()), Some(80), None, None).is_err());
    }
}
