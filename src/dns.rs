//! DNS-level blocking module for uwu.
//!
//! Domains are sinkholed through the Windows hosts file
//! (`%SystemRoot%\System32\drivers\etc\hosts`); raw IPs and CIDR ranges can't be
//! expressed there, so those get a pair of Windows Firewall block rules instead.
//!
//! Both live in a uwu-owned namespace — a marker-fenced section in `hosts`, and
//! an `uwu-block:` rule-name prefix in the firewall — so hand-written entries are
//! never touched, and `clear` can undo exactly what uwu created.

use anyhow::{bail, Result};

use crate::color;

/// Marker lines fencing off the section of the hosts file uwu owns.
const HOSTS_BEGIN: &str = "# >>> uwu block start";
const HOSTS_END: &str = "# <<< uwu block end";

/// Sinkhole addresses. Both families are written: an A-only entry still lets a
/// dual-stack machine reach the site over IPv6.
const SINK_V4: &str = "0.0.0.0";
const SINK_V6: &str = "::";

/// Display-name prefix for the firewall rules uwu manages.
const RULE_PREFIX: &str = "uwu-block:";

/// Names that must never be sinkholed — doing so breaks local resolution.
const PROTECTED: &[&str] = &["localhost", "localhost.localdomain", "broadcasthost"];

/// Registrable two-label suffixes, so `example.co.jp` is recognised as an apex
/// domain and gets its `www.` variant blocked too. Not exhaustive by design —
/// anything missed just means one fewer auto-added variant.
const MULTI_TLDS: &[&str] = &[
    "co.jp", "ne.jp", "or.jp", "ac.jp", "go.jp", "co.uk", "org.uk", "ac.uk", "gov.uk", "co.kr",
    "com.au", "net.au", "org.au", "com.br", "com.cn", "co.nz", "com.tw", "co.in", "com.mx",
];

/// What a user-supplied target resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// A hostname — blocked via the hosts file.
    Domain(String),
    /// An IPv4/IPv6 address or CIDR range — blocked via the firewall.
    Addr(String),
}

// Public API

/// List everything uwu is currently blocking.
pub fn list() -> Result<()> {
    let domains = parse_section(&read_hosts()?);
    let addrs = firewall_list().unwrap_or_default();

    if domains.is_empty() && addrs.is_empty() {
        println!("  {}", color::note("nothing blocked"));
        return Ok(());
    }

    if !domains.is_empty() {
        println!(
            "  {} {}",
            color::header("blocked domains"),
            color::note(&format!("({})", domains.len()))
        );
        for d in &domains {
            println!("    {} {}", color::error("-"), d);
        }
    }

    if !addrs.is_empty() {
        if !domains.is_empty() {
            println!();
        }
        println!(
            "  {} {}",
            color::header("blocked addresses"),
            color::note(&format!("({})", addrs.len()))
        );
        for a in &addrs {
            println!("    {} {}", color::error("-"), a);
        }
    }

    Ok(())
}

/// Block domains (hosts file) and/or addresses (firewall).
pub fn block(targets: &[String], exact: bool, dry_run: bool) -> Result<()> {
    let (domains, addrs) = split_targets(targets, exact)?;

    if dry_run {
        return preview("block", &domains, &addrs);
    }
    if !crate::utils::is_elevated() {
        return elevate("block", &domains, &addrs, &[]);
    }

    let content = read_hosts()?;
    let mut current = parse_section(&content);
    let mut added = Vec::new();

    for d in &domains {
        if current.contains(d) {
            println!("  {} {} already blocked", color::note("·"), d);
        } else {
            current.push(d.clone());
            added.push(d.clone());
        }
    }

    if !added.is_empty() {
        current.sort();
        current.dedup();
        write_hosts(&render_hosts(&content, &current))?;
        for d in &added {
            crate::utils::print_success(&format!("blocked {}", d));
        }
        flush_dns();
    }

    let existing = firewall_list().unwrap_or_default();
    for a in &addrs {
        if existing.contains(a) {
            println!("  {} {} already blocked", color::note("·"), a);
            continue;
        }
        firewall_add(a)?;
        crate::utils::print_success(&format!("blocked {} {}", a, color::note("(firewall)")));
    }

    Ok(())
}

/// Remove blocks previously added by `block`.
pub fn unblock(targets: &[String], exact: bool, dry_run: bool) -> Result<()> {
    let (domains, addrs) = split_targets(targets, exact)?;

    if dry_run {
        return preview("unblock", &domains, &addrs);
    }
    if !crate::utils::is_elevated() {
        return elevate("unblock", &domains, &addrs, &[]);
    }

    let content = read_hosts()?;
    let mut current = parse_section(&content);
    let mut removed = Vec::new();

    for d in &domains {
        if current.contains(d) {
            current.retain(|c| c != d);
            removed.push(d.clone());
        } else {
            println!("  {} {} was not blocked", color::note("·"), d);
        }
    }

    if !removed.is_empty() {
        write_hosts(&render_hosts(&content, &current))?;
        for d in &removed {
            crate::utils::print_success(&format!("unblocked {}", d));
        }
        flush_dns();
    }

    let existing = firewall_list().unwrap_or_default();
    for a in &addrs {
        if !existing.contains(a) {
            println!("  {} {} was not blocked", color::note("·"), a);
            continue;
        }
        firewall_delete(a)?;
        crate::utils::print_success(&format!("unblocked {} {}", a, color::note("(firewall)")));
    }

    Ok(())
}

/// Drop every uwu-managed block, leaving hand-written hosts entries alone.
pub fn clear(force: bool) -> Result<()> {
    let content = read_hosts()?;
    let domains = parse_section(&content);
    let addrs = firewall_list().unwrap_or_default();
    let total = domains.len() + addrs.len();

    if total == 0 {
        println!("  {}", color::note("nothing blocked"));
        return Ok(());
    }

    if !force && !confirm(&format!("remove all {} block(s)?", total))? {
        crate::utils::print_info("cancelled");
        return Ok(());
    }

    if !crate::utils::is_elevated() {
        // Already confirmed here, so the elevated child skips its own prompt.
        return elevate("clear", &[], &[], &["--force"]);
    }

    if !domains.is_empty() {
        write_hosts(&render_hosts(&content, &[]))?;
        flush_dns();
    }
    for a in &addrs {
        firewall_delete(a)?;
    }

    crate::utils::print_success(&format!("cleared {} block(s)", total));
    Ok(())
}

// Target parsing (platform-independent, unit-tested)

/// Classify a user-supplied target as a domain or an address/range.
///
/// Accepts a bare host, a URL (`https://youtube.com/feed` → `youtube.com`), a
/// `host:port`, an IPv4/IPv6 literal, or a CIDR range.
fn parse_target(raw: &str) -> Result<Target> {
    let t = raw.trim();
    if t.is_empty() {
        bail!("empty target");
    }

    // CIDR and bare IPs first — a CIDR's "/" would otherwise look like a URL path.
    if let Some((addr, prefix)) = t.split_once('/') {
        if addr.parse::<std::net::IpAddr>().is_ok() {
            let max = if addr.contains(':') { 128 } else { 32 };
            match prefix.parse::<u8>() {
                Ok(p) if p <= max => return Ok(Target::Addr(t.to_string())),
                _ => bail!("invalid CIDR prefix: {}", t),
            }
        }
    }
    if t.parse::<std::net::IpAddr>().is_ok() {
        return Ok(Target::Addr(t.to_string()));
    }

    // Strip scheme, path, credentials and port down to a bare host.
    let host = t.split("://").last().unwrap_or(t);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim_end_matches('.').to_ascii_lowercase();

    if PROTECTED.contains(&host.as_str()) {
        bail!("{} is protected and cannot be blocked", host);
    }
    validate_domain(&host)?;
    Ok(Target::Domain(host))
}

/// Reject anything that isn't a plausible hostname before it reaches the hosts file.
fn validate_domain(host: &str) -> Result<()> {
    if host.is_empty() || host.len() > 253 {
        bail!("invalid domain: {}", host);
    }
    if !host.contains('.') {
        bail!("invalid domain: {} (expected something like example.com)", host);
    }
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            bail!("invalid domain: {}", host);
        }
        if label.starts_with('-') || label.ends_with('-') {
            bail!("invalid domain: {}", host);
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            bail!("invalid domain: {}", host);
        }
    }
    Ok(())
}

/// A bare apex domain also gets its `www.` variant — blocking `youtube.com`
/// without `www.youtube.com` blocks nothing in practice. `--exact` opts out.
fn expand(domain: &str, exact: bool) -> Vec<String> {
    if exact || domain.starts_with("www.") || !is_apex(domain) {
        return vec![domain.to_string()];
    }
    vec![domain.to_string(), format!("www.{}", domain)]
}

/// True for `example.com` and `example.co.jp`, false for `mail.example.com`.
fn is_apex(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    match labels.len() {
        2 => true,
        3 => MULTI_TLDS.contains(&labels[1..].join(".").as_str()),
        _ => false,
    }
}

/// Parse every target, expand domains, and split them by blocking mechanism.
fn split_targets(raw: &[String], exact: bool) -> Result<(Vec<String>, Vec<String>)> {
    if raw.is_empty() {
        bail!("no targets given");
    }
    let mut domains: Vec<String> = Vec::new();
    let mut addrs: Vec<String> = Vec::new();

    for r in raw {
        match parse_target(r)? {
            Target::Domain(d) => {
                for v in expand(&d, exact) {
                    if !domains.contains(&v) {
                        domains.push(v);
                    }
                }
            }
            Target::Addr(a) => {
                if !addrs.contains(&a) {
                    addrs.push(a);
                }
            }
        }
    }
    Ok((domains, addrs))
}

// Hosts file section (platform-independent, unit-tested)

/// Read back the domains inside uwu's fenced section.
fn parse_section(content: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();

    for line in content.lines() {
        let t = line.trim();
        if t == HOSTS_BEGIN {
            inside = true;
        } else if t == HOSTS_END {
            inside = false;
        } else if inside {
            // Only the v4 line is collected; the v6 twin would just duplicate it.
            let mut fields = t.split_whitespace();
            if let (Some(sink), Some(domain)) = (fields.next(), fields.next()) {
                if sink == SINK_V4 {
                    out.push(domain.to_ascii_lowercase());
                }
            }
        }
    }
    out
}

/// Rewrite `original` with uwu's section holding exactly `domains`.
///
/// Everything outside the markers is preserved verbatim. An empty `domains`
/// drops the section entirely, so a cleared hosts file looks untouched.
fn render_hosts(original: &str, domains: &[String]) -> String {
    let nl = if original.contains("\r\n") || !original.contains('\n') {
        "\r\n"
    } else {
        "\n"
    };

    let lines: Vec<&str> = original.lines().collect();
    let begin = lines.iter().position(|l| l.trim() == HOSTS_BEGIN);
    let end = lines.iter().position(|l| l.trim() == HOSTS_END);

    let (mut head, mut tail): (Vec<&str>, Vec<&str>) = match (begin, end) {
        (Some(b), Some(e)) if e > b => (lines[..b].to_vec(), lines[e + 1..].to_vec()),
        // Missing or crossed markers: drop the strays, keep every real line.
        _ => (
            lines
                .iter()
                .copied()
                .filter(|l| l.trim() != HOSTS_BEGIN && l.trim() != HOSTS_END)
                .collect(),
            Vec::new(),
        ),
    };

    while head.last().is_some_and(|l| l.trim().is_empty()) {
        head.pop();
    }
    while tail.first().is_some_and(|l| l.trim().is_empty()) {
        tail.remove(0);
    }

    let mut out: Vec<String> = head.iter().map(|s| s.to_string()).collect();

    if !domains.is_empty() {
        if !out.is_empty() {
            out.push(String::new());
        }
        out.push(HOSTS_BEGIN.to_string());
        for d in domains {
            out.push(format!("{:<8}{}", SINK_V4, d));
            out.push(format!("{:<8}{}", SINK_V6, d));
        }
        out.push(HOSTS_END.to_string());
    }

    if !tail.is_empty() {
        out.push(String::new());
        out.extend(tail.iter().map(|s| s.to_string()));
    }

    let mut s = out.join(nl);
    if !s.is_empty() {
        s.push_str(nl);
    }
    s
}

// Shared output helpers

/// Show what a `block`/`unblock` would touch, without touching anything.
fn preview(action: &str, domains: &[String], addrs: &[String]) -> Result<()> {
    println!("  {} {}", color::info("dry run —"), color::note("nothing changed"));
    for d in domains {
        println!("    {} {} {}", color::accent(action), d, color::note("(hosts)"));
    }
    for a in addrs {
        println!("    {} {} {}", color::accent(action), a, color::note("(firewall)"));
    }
    Ok(())
}

/// Yes/no confirmation prompt (default no).
fn confirm(question: &str) -> Result<bool> {
    use std::io::{self, Write};
    print!("  {} {} [y/N]: ", color::warn("!"), question);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

// Windows I/O

/// `%SystemRoot%\System32\drivers\etc\hosts`.
#[cfg(windows)]
fn hosts_path() -> std::path::PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    std::path::Path::new(&root)
        .join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts")
}

/// Read the hosts file. A missing file is treated as empty; non-UTF-8 content is
/// refused rather than mangled by a lossy round-trip.
#[cfg(windows)]
fn read_hosts() -> Result<String> {
    use anyhow::Context;

    let path = hosts_path();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
    };

    String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("hosts file is not UTF-8 — refusing to rewrite it"))
}

#[cfg(windows)]
fn write_hosts(content: &str) -> Result<()> {
    let path = hosts_path();
    std::fs::write(&path, content).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            anyhow::anyhow!(
                "cannot write {} — admin is required, and Defender's \
                 'HostsFileHijack' protection can also lock this file",
                path.display()
            )
        } else {
            anyhow::anyhow!("failed to write {}: {}", path.display(), e)
        }
    })
}

/// Drop the resolver cache so a fresh block/unblock takes effect immediately.
/// Best-effort: a failure here doesn't invalidate the hosts change.
#[cfg(windows)]
fn flush_dns() {
    let _ = std::process::Command::new("ipconfig")
        .arg("/flushdns")
        .output();
}

/// Block an address or range in both directions.
#[cfg(windows)]
fn firewall_add(addr: &str) -> Result<()> {
    for dir in ["in", "out"] {
        let ok = netsh(&[
            "advfirewall".to_string(),
            "firewall".to_string(),
            "add".to_string(),
            "rule".to_string(),
            format!("name={}{}", RULE_PREFIX, addr),
            format!("dir={}", dir),
            "action=block".to_string(),
            format!("remoteip={}", addr),
        ])?;
        if !ok {
            bail!("failed to add a firewall rule for {}", addr);
        }
    }
    Ok(())
}

/// Delete both rules for an address. Deleting by name removes every direction.
#[cfg(windows)]
fn firewall_delete(addr: &str) -> Result<()> {
    netsh(&[
        "advfirewall".to_string(),
        "firewall".to_string(),
        "delete".to_string(),
        "rule".to_string(),
        format!("name={}{}", RULE_PREFIX, addr),
    ])?;
    Ok(())
}

/// Run netsh silently; `Ok(false)` means it ran but reported no match/failure.
/// Output is captured so localized netsh text never leaks into uwu's output.
#[cfg(windows)]
fn netsh(args: &[String]) -> Result<bool> {
    use anyhow::Context;
    let output = std::process::Command::new("netsh")
        .args(args)
        .output()
        .context("failed to run netsh")?;
    Ok(output.status.success())
}

/// The addresses uwu currently blocks, read back from the firewall itself.
///
/// `Get-NetFirewallRule` is used rather than `netsh show rule` because its
/// output is structured and not localized. The address is carried in the rule
/// name, so no second lookup is needed.
#[cfg(windows)]
fn firewall_list() -> Result<Vec<String>> {
    use anyhow::Context;

    let script = format!(
        "(Get-NetFirewallRule -DisplayName '{}*' -ErrorAction SilentlyContinue).DisplayName",
        RULE_PREFIX
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .context("failed to run powershell")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let mut addrs = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        // One rule per direction shares a name, so the same address appears twice.
        if let Some(addr) = line.trim().strip_prefix(RULE_PREFIX) {
            let addr = addr.to_string();
            if !addr.is_empty() && !addrs.contains(&addr) {
                addrs.push(addr);
            }
        }
    }
    Ok(addrs)
}

/// Re-launch `uwu dns <action> ...` elevated via UAC, mirroring how `net` and
/// `wdex` self-elevate. Only normalized targets are forwarded (validated domains,
/// IPs and CIDRs — never raw user text), so space-joining is safe.
#[cfg(windows)]
fn elevate(action: &str, domains: &[String], addrs: &[String], flags: &[&str]) -> Result<()> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    crate::utils::print_warn("administrator privileges required — requesting elevation");

    let exe = std::env::current_exe()?;
    let exe_w: Vec<u16> = exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut argv: Vec<String> = vec!["dns".to_string(), action.to_string()];
    argv.extend(domains.iter().cloned());
    argv.extend(addrs.iter().cloned());
    argv.extend(flags.iter().map(|f| f.to_string()));
    // Domains are already expanded here — don't let the child expand them again.
    if !domains.is_empty() {
        argv.push("--exact".to_string());
    }

    let args_w: Vec<u16> = argv
        .join(" ")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR::from_raw(exe_w.as_ptr()),
            PCWSTR::from_raw(args_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW returns a value > 32 on success.
    if result.0 <= 32 {
        bail!("failed to elevate (UAC declined or cancelled)");
    }
    crate::utils::print_info("running in the elevated window");
    Ok(())
}

// Non-Windows stubs.
#[cfg(not(windows))]
fn read_hosts() -> Result<String> {
    unsupported()
}
#[cfg(not(windows))]
fn write_hosts(_content: &str) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn flush_dns() {}
#[cfg(not(windows))]
fn firewall_add(_addr: &str) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn firewall_delete(_addr: &str) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn firewall_list() -> Result<Vec<String>> {
    anyhow::bail!("dns is only supported on Windows")
}
#[cfg(not(windows))]
fn elevate(_a: &str, _d: &[String], _p: &[String], _f: &[&str]) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn unsupported<T>() -> Result<T> {
    anyhow::bail!("dns is only supported on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_domains_and_addresses() {
        assert_eq!(
            parse_target("youtube.com").unwrap(),
            Target::Domain("youtube.com".into())
        );
        assert_eq!(
            parse_target("1.2.3.4").unwrap(),
            Target::Addr("1.2.3.4".into())
        );
        assert_eq!(
            parse_target("10.0.0.0/8").unwrap(),
            Target::Addr("10.0.0.0/8".into())
        );
        assert_eq!(
            parse_target("2606:4700::1111").unwrap(),
            Target::Addr("2606:4700::1111".into())
        );
    }

    #[test]
    fn normalizes_urls_and_case() {
        assert_eq!(
            parse_target("https://WWW.YouTube.com/feed/trending").unwrap(),
            Target::Domain("www.youtube.com".into())
        );
        assert_eq!(
            parse_target("example.com:8080").unwrap(),
            Target::Domain("example.com".into())
        );
        assert_eq!(
            parse_target("example.com.").unwrap(),
            Target::Domain("example.com".into())
        );
    }

    #[test]
    fn rejects_bad_targets() {
        assert!(parse_target("localhost").is_err());
        assert!(parse_target("not a domain").is_err());
        assert!(parse_target("-bad.com").is_err());
        assert!(parse_target("单").is_err());
        assert!(parse_target("1.2.3.4/99").is_err());
        assert!(parse_target("").is_err());
    }

    #[test]
    fn expands_apex_domains_only() {
        assert_eq!(expand("youtube.com", false), ["youtube.com", "www.youtube.com"]);
        assert_eq!(expand("example.co.jp", false), ["example.co.jp", "www.example.co.jp"]);
        assert_eq!(expand("mail.google.com", false), ["mail.google.com"]);
        assert_eq!(expand("www.youtube.com", false), ["www.youtube.com"]);
        assert_eq!(expand("youtube.com", true), ["youtube.com"]);
    }

    #[test]
    fn round_trips_the_managed_section() {
        let original = "127.0.0.1 localhost\r\n";
        let rendered = render_hosts(original, &["youtube.com".to_string()]);

        assert!(rendered.starts_with("127.0.0.1 localhost"));
        assert!(rendered.contains("0.0.0.0 youtube.com"));
        assert!(rendered.contains("::      youtube.com"));
        assert_eq!(parse_section(&rendered), ["youtube.com"]);
    }

    #[test]
    fn preserves_entries_outside_the_markers() {
        let original = "# mine\r\n10.0.0.1 nas\r\n";
        let with = render_hosts(original, &["a.com".to_string(), "b.com".to_string()]);
        let cleared = render_hosts(&with, &[]);

        assert_eq!(parse_section(&with), ["a.com", "b.com"]);
        assert!(cleared.contains("10.0.0.1 nas"));
        assert!(!cleared.contains(HOSTS_BEGIN));
        assert!(parse_section(&cleared).is_empty());
    }

    #[test]
    fn keeps_trailing_entries_when_the_section_is_rewritten() {
        let original = format!(
            "# head\r\n\r\n{}\r\n0.0.0.0 old.com\r\n::      old.com\r\n{}\r\n\r\n10.0.0.1 nas\r\n",
            HOSTS_BEGIN, HOSTS_END
        );
        let rewritten = render_hosts(&original, &["new.com".to_string()]);

        assert_eq!(parse_section(&rewritten), ["new.com"]);
        assert!(rewritten.contains("# head"));
        assert!(rewritten.contains("10.0.0.1 nas"));
        assert!(!rewritten.contains("old.com"));
        // Exactly one section survives the rewrite.
        assert_eq!(rewritten.matches(HOSTS_BEGIN).count(), 1);
    }

    #[test]
    fn heals_a_stray_marker() {
        let original = format!("127.0.0.1 localhost\n{}\n0.0.0.0 x.com\n", HOSTS_BEGIN);
        let rendered = render_hosts(&original, &[]);

        assert!(!rendered.contains(HOSTS_BEGIN));
        assert!(rendered.contains("127.0.0.1 localhost"));
    }

    #[test]
    fn splits_and_dedupes_targets() {
        let raw = vec![
            "youtube.com".to_string(),
            "https://youtube.com/watch".to_string(),
            "1.2.3.4".to_string(),
            "1.2.3.4".to_string(),
        ];
        let (domains, addrs) = split_targets(&raw, false).unwrap();

        assert_eq!(domains, ["youtube.com", "www.youtube.com"]);
        assert_eq!(addrs, ["1.2.3.4"]);
    }
}
