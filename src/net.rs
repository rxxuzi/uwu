//! Network information module for uwu.
//!
//! Read-only network views (dashboard, IP, DNS, MAC, saved Wi-Fi) built on
//! native Win32 APIs — `GetAdaptersAddresses` (IpHelper) and the WLAN API — so
//! nothing depends on parsing localized `ipconfig`/`netsh` output.

use anyhow::Result;

use crate::color;

/// A network adapter's essentials.
#[cfg(windows)]
struct Adapter {
    name: String,
    mac: String,
    ipv4: Vec<String>,
    gateways: Vec<String>,
    dns: Vec<String>,
    up: bool,
}

#[cfg(windows)]
impl Adapter {
    /// Relevant for display: up, has an IPv4, and not the loopback pseudo-interface.
    fn relevant(&self) -> bool {
        self.up && !self.ipv4.is_empty() && !self.name.contains("Loopback")
    }
}

/// Dashboard: the primary adapter's SSID / IP / gateway / DNS / MAC.
#[cfg(windows)]
pub fn dashboard() -> Result<()> {
    let adapters = unsafe { collect_adapters()? };
    let (ssid, _) = unsafe { wlan_info() };

    // Primary = a relevant adapter that has a default gateway; else first relevant.
    let primary = adapters
        .iter()
        .find(|a| a.relevant() && !a.gateways.is_empty())
        .or_else(|| adapters.iter().find(|a| a.relevant()));

    println!();
    let primary = match primary {
        Some(a) => a,
        None => {
            println!("  {}", color::note("no active network connection"));
            return Ok(());
        }
    };

    if let Some(ssid) = ssid {
        row("SSID", &ssid);
    }
    row("adapter", &primary.name);
    row("IPv4", &primary.ipv4.join(", "));
    if !primary.gateways.is_empty() {
        row("gateway", &primary.gateways.join(", "));
    }
    if !primary.dns.is_empty() {
        row("DNS", &primary.dns.join(", "));
    }
    if !primary.mac.is_empty() {
        row("MAC", &primary.mac);
    }
    Ok(())
}

/// Show IPv4 address(es) per relevant adapter.
#[cfg(windows)]
pub fn show_ip() -> Result<()> {
    let adapters = unsafe { collect_adapters()? };
    print_per_adapter(&adapters, |a| Some(a.ipv4.join(", ")));
    Ok(())
}

/// Show DNS servers per relevant adapter.
#[cfg(windows)]
pub fn show_dns() -> Result<()> {
    let adapters = unsafe { collect_adapters()? };
    print_per_adapter(&adapters, |a| {
        if a.dns.is_empty() {
            None
        } else {
            Some(a.dns.join(", "))
        }
    });
    Ok(())
}

/// Show MAC address per relevant adapter.
#[cfg(windows)]
pub fn show_mac() -> Result<()> {
    let adapters = unsafe { collect_adapters()? };
    print_per_adapter(&adapters, |a| {
        if a.mac.is_empty() {
            None
        } else {
            Some(a.mac.clone())
        }
    });
    Ok(())
}

/// List saved Wi-Fi profiles, marking the currently-connected one.
#[cfg(windows)]
pub fn wifi_list() -> Result<()> {
    let (current, profiles) = unsafe { wlan_info() };

    println!();
    if profiles.is_empty() {
        println!("  {}", color::note("no saved Wi-Fi networks"));
        return Ok(());
    }

    println!("  {}", color::header("saved wi-fi:"));
    for (i, name) in profiles.iter().enumerate() {
        let num = format!("{}.", i + 1);
        if Some(name) == current.as_ref() {
            println!(
                "    {:<3} {} {}",
                num,
                color::accent(name),
                color::success("(connected)")
            );
        } else {
            println!("    {:<3} {}", num, name);
        }
    }
    Ok(())
}

/// Resolve a Wi-Fi argument (1-based index from `net wifi`, or a name matched
/// case-insensitively) to the exact saved profile name.
#[cfg(windows)]
fn resolve_profile(arg: &str) -> Result<String> {
    let (_, profiles) = unsafe { wlan_info() };

    if let Ok(idx) = arg.parse::<usize>() {
        if idx >= 1 && idx <= profiles.len() {
            return Ok(profiles[idx - 1].clone());
        }
        anyhow::bail!("no saved network at index {} (see 'uwu net wifi')", idx);
    }

    profiles
        .iter()
        .find(|p| p.eq_ignore_ascii_case(arg))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no saved network '{}' (see 'uwu net wifi')", arg))
}

/// Show the saved password for a Wi-Fi profile (explicit opt-in via `-p`).
#[cfg(windows)]
pub fn wifi_password(name: &str) -> Result<()> {
    let name = resolve_profile(name)?;
    let xml = unsafe { wlan_profile_xml(&name)? };
    println!();
    match extract_key_material(&xml) {
        Some(key) => println!("  {} {} {}", color::accent(&name), color::note("key"), key),
        None => crate::utils::print_info("no password stored (open network?)"),
    }
    Ok(())
}

/// Connect to a Wi-Fi network. With a password, a profile is created first.
#[cfg(windows)]
pub fn wifi_connect(name: &str, pass: Option<&str>) -> Result<()> {
    use anyhow::{bail, Context};
    use std::process::Command;

    // Without a password we connect to a saved profile, so accept an index or
    // case-insensitive name. With a password, `name` is the SSID to join as-is.
    let name = match pass {
        Some(_) => name.to_string(),
        None => resolve_profile(name)?,
    };

    println!();

    // With a password, add a WPA2-PSK profile before connecting.
    if let Some(pass) = pass {
        let xml = build_profile_xml(&name, pass);
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("uwu_wifi_{}.xml", std::process::id()));
        std::fs::write(&tmp, xml).context("failed to write Wi-Fi profile")?;

        let add = Command::new("netsh")
            .args(["wlan", "add", "profile"])
            .arg(format!("filename={}", tmp.display()))
            .output()
            .context("failed to run netsh")?;
        let _ = std::fs::remove_file(&tmp);
        if !add.status.success() {
            bail!("failed to add Wi-Fi profile for {}", name);
        }
    }

    let status = Command::new("netsh")
        .args(["wlan", "connect"])
        .arg(format!("name={}", name))
        .status()
        .context("failed to run netsh")?;

    if !status.success() {
        bail!("failed to connect to {}", name);
    }

    crate::utils::print_success(&format!("connecting to {}", name));
    Ok(())
}

/// Flush the DNS resolver cache (harmless, no admin needed).
#[cfg(windows)]
pub fn flush() -> Result<()> {
    println!();
    run("ipconfig", &["/flushdns".to_string()])?;
    crate::utils::print_success("flushed DNS cache");
    Ok(())
}

/// Disconnect the current Wi-Fi (radio stays on).
#[cfg(windows)]
pub fn disconnect() -> Result<()> {
    println!();
    if !confirm("disconnect Wi-Fi?")? {
        return Ok(());
    }
    run("netsh", &["wlan".into(), "disconnect".into()])?;
    crate::utils::print_success("disconnected");
    Ok(())
}

/// Turn the Wi-Fi radio on or off.
///
/// This drives the *software radio state* through the WLAN API
/// (`WlanSetInterface` + `wlan_intf_opcode_radio_state`) — the same soft kill
/// switch as the Windows Wi-Fi toggle. Unlike disabling the adapter itself
/// (`netsh interface set interface admin=...`), this needs **no admin**.
#[cfg(windows)]
pub fn radio(on: bool) -> Result<()> {
    println!();
    if !on && !confirm("turn Wi-Fi off?")? {
        return Ok(());
    }
    unsafe { set_radio_state(on)? };
    crate::utils::print_success(if on { "Wi-Fi on" } else { "Wi-Fi off" });
    Ok(())
}

/// Set the software radio state on the first wireless interface.
#[cfg(windows)]
unsafe fn set_radio_state(on: bool) -> Result<()> {
    use anyhow::bail;
    use std::ffi::c_void;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::{
        dot11_radio_state_off, dot11_radio_state_on, dot11_radio_state_unknown,
        wlan_intf_opcode_radio_state, WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory,
        WlanOpenHandle, WlanSetInterface, WLAN_INTERFACE_INFO_LIST, WLAN_PHY_RADIO_STATE,
    };

    let mut handle = HANDLE::default();
    let mut negotiated = 0u32;
    if WlanOpenHandle(2, None, &mut negotiated, &mut handle) != 0 {
        bail!("failed to open the WLAN service");
    }

    // Use the first wireless interface.
    let mut iface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if WlanEnumInterfaces(handle, None, &mut iface_list) != 0 || iface_list.is_null() {
        WlanCloseHandle(handle, None);
        bail!("no Wi-Fi interface found");
    }
    let list = &*iface_list;
    if list.dwNumberOfItems == 0 {
        WlanFreeMemory(iface_list as *const c_void);
        WlanCloseHandle(handle, None);
        bail!("no Wi-Fi interface found");
    }
    let guid = (*list.InterfaceInfo.as_ptr()).InterfaceGuid;
    WlanFreeMemory(iface_list as *const c_void);

    // Only the software radio state is settable; hardware state is read-only,
    // so leave it unknown.
    let state = WLAN_PHY_RADIO_STATE {
        dwPhyIndex: 0,
        dot11SoftwareRadioState: if on {
            dot11_radio_state_on
        } else {
            dot11_radio_state_off
        },
        dot11HardwareRadioState: dot11_radio_state_unknown,
    };

    let ret = WlanSetInterface(
        handle,
        &guid,
        wlan_intf_opcode_radio_state,
        std::mem::size_of::<WLAN_PHY_RADIO_STATE>() as u32,
        &state as *const _ as *const c_void,
        None,
    );
    WlanCloseHandle(handle, None);

    match ret {
        0 => Ok(()),
        // ERROR_ACCESS_DENIED — some hardware/policies gate the radio behind admin.
        5 => bail!("access denied — this device requires admin to change the Wi-Fi radio"),
        code => bail!("WlanSetInterface failed (code {code})"),
    }
}

/// Set DNS servers on the primary adapter (needs admin).
#[cfg(windows)]
pub fn set_dns(servers: &[String]) -> Result<()> {
    println!();
    if !crate::utils::is_elevated() {
        let mut args = vec!["dns"];
        args.extend(servers.iter().map(|s| s.as_str()));
        return elevate_self(&args);
    }
    let name = primary_adapter_name()?;
    if !confirm(&format!("set DNS to {} on {}?", servers.join(", "), name))? {
        return Ok(());
    }

    run(
        "netsh",
        &[
            "interface".into(),
            "ip".into(),
            "set".into(),
            "dns".into(),
            format!("name={}", name),
            "static".into(),
            servers[0].clone(),
        ],
    )?;
    for (i, srv) in servers.iter().enumerate().skip(1) {
        run(
            "netsh",
            &[
                "interface".into(),
                "ip".into(),
                "add".into(),
                "dns".into(),
                format!("name={}", name),
                srv.clone(),
                format!("index={}", i + 1),
            ],
        )?;
    }
    crate::utils::print_success(&format!("DNS set to {}", servers.join(", ")));
    Ok(())
}

/// Revert the primary adapter's DNS to automatic (DHCP) (needs admin).
#[cfg(windows)]
pub fn dns_auto() -> Result<()> {
    println!();
    if !crate::utils::is_elevated() {
        return elevate_self(&["dns", "auto"]);
    }
    let name = primary_adapter_name()?;
    run(
        "netsh",
        &[
            "interface".into(),
            "ip".into(),
            "set".into(),
            "dns".into(),
            format!("name={}", name),
            "dhcp".into(),
        ],
    )?;
    crate::utils::print_success("DNS reverted to automatic (DHCP)");
    Ok(())
}

/// Reset the network stack — winsock catalog (needs admin, reboot required).
#[cfg(windows)]
pub fn reset() -> Result<()> {
    println!();
    if !crate::utils::is_elevated() {
        return elevate_self(&["reset"]);
    }
    if !confirm("reset winsock? (a reboot will be required)")? {
        return Ok(());
    }
    run("netsh", &["winsock".into(), "reset".into()])?;
    crate::utils::print_success("winsock reset — restart your PC to finish");
    Ok(())
}

/// Run a command silently, mapping a non-zero exit to an error.
/// Output is captured (not shown) so we only surface uwu's own messages —
/// this also avoids leaking localized `netsh`/`ipconfig` text.
#[cfg(windows)]
fn run(program: &str, args: &[String]) -> Result<()> {
    use anyhow::{bail, Context};
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", program))?;
    if !output.status.success() {
        bail!("{} failed", program);
    }
    Ok(())
}

/// Re-launch `uwu net <args>` elevated via UAC (the `runas` verb), since these
/// operations need admin. The elevated child runs the command — and any
/// confirmation — in its own console window, mirroring how `wdex` self-elevates.
/// Returns once the UAC prompt is accepted; bails if it's declined or cancelled.
#[cfg(windows)]
fn elevate_self(args: &[&str]) -> Result<()> {
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

    // "net" plus the sub-action/arguments, space-joined (all tokens are simple:
    // on/off/reset/auto or dotted IPs, so no quoting is needed).
    let mut argv = vec!["net"];
    argv.extend_from_slice(args);
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
        anyhow::bail!("failed to elevate (UAC declined or cancelled)");
    }
    crate::utils::print_info("running in the elevated window");
    Ok(())
}

/// Yes/no confirmation prompt (default no).
#[cfg(windows)]
fn confirm(question: &str) -> Result<bool> {
    use std::io::{self, Write};
    print!("  {} {} [y/N]: ", color::warn("!"), question);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Friendly name of the primary adapter (has a default gateway).
#[cfg(windows)]
fn primary_adapter_name() -> Result<String> {
    let adapters = unsafe { collect_adapters()? };
    adapters
        .iter()
        .find(|a| a.relevant() && !a.gateways.is_empty())
        .or_else(|| adapters.iter().find(|a| a.relevant()))
        .map(|a| a.name.clone())
        .ok_or_else(|| anyhow::anyhow!("no active network adapter"))
}

/// Extract `<keyMaterial>...</keyMaterial>` from a WLAN profile XML.
fn extract_key_material(xml: &str) -> Option<String> {
    let start = xml.find("<keyMaterial>")? + "<keyMaterial>".len();
    let end = xml[start..].find("</keyMaterial>")?;
    Some(xml[start..start + end].to_string())
}

/// Minimal XML-escape for profile values.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build a WPA2-PSK WLAN profile XML for `name` with the given passphrase.
fn build_profile_xml(name: &str, pass: &str) -> String {
    let name = xml_escape(name);
    let pass = xml_escape(pass);
    format!(
        r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{name}</name>
  <SSIDConfig><SSID><name>{name}</name></SSID></SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM><security>
    <authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption>
    <sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{pass}</keyMaterial></sharedKey>
  </security></MSM>
</WLANProfile>"#
    )
}

#[cfg(windows)]
fn row(label: &str, value: &str) {
    // Pad the plain label first, then colorize — otherwise the ANSI codes are
    // counted in the width and the columns misalign.
    println!("  {} {}", color::note(&format!("{:<7}", label)), value);
}

#[cfg(windows)]
fn print_per_adapter(adapters: &[Adapter], field: impl Fn(&Adapter) -> Option<String>) {
    println!();
    let mut printed = false;
    for a in adapters.iter().filter(|a| a.relevant()) {
        if let Some(value) = field(a) {
            println!("  {} {} {}", color::accent(&a.name), color::note("·"), value);
            printed = true;
        }
    }
    if !printed {
        println!("  {}", color::note("no active network connection"));
    }
}

#[cfg(windows)]
unsafe fn collect_adapters() -> anyhow::Result<Vec<Adapter>> {
    use anyhow::bail;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::AF_UNSPEC;

    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let family = AF_UNSPEC.0 as u32;

    // First call sizes the buffer.
    let mut size = 0u32;
    GetAdaptersAddresses(family, flags, None, None, &mut size);
    if size == 0 {
        return Ok(Vec::new());
    }

    let mut buf = vec![0u8; size as usize];
    let ret = GetAdaptersAddresses(
        family,
        flags,
        None,
        Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
        &mut size,
    );
    if ret != 0 {
        bail!("GetAdaptersAddresses failed (code {ret})");
    }

    let mut adapters = Vec::new();
    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() {
        let a = &*cur;

        let mut ipv4 = Vec::new();
        let mut uni = a.FirstUnicastAddress;
        while !uni.is_null() {
            if let Some(ip) = sockaddr_ipv4((*uni).Address.lpSockaddr) {
                ipv4.push(ip);
            }
            uni = (*uni).Next;
        }

        let mut gateways = Vec::new();
        let mut g = a.FirstGatewayAddress;
        while !g.is_null() {
            if let Some(ip) = sockaddr_ipv4((*g).Address.lpSockaddr) {
                gateways.push(ip);
            }
            g = (*g).Next;
        }

        let mut dns = Vec::new();
        let mut d = a.FirstDnsServerAddress;
        while !d.is_null() {
            if let Some(ip) = sockaddr_ipv4((*d).Address.lpSockaddr) {
                dns.push(ip);
            }
            d = (*d).Next;
        }

        adapters.push(Adapter {
            name: a.FriendlyName.to_string().unwrap_or_default(),
            mac: format_mac(&a.PhysicalAddress, a.PhysicalAddressLength),
            ipv4,
            gateways,
            dns,
            up: a.OperStatus.0 == 1, // IfOperStatusUp
        });

        cur = a.Next;
    }

    Ok(adapters)
}

/// Extract a dotted IPv4 string from a SOCKADDR (ignores non-IPv4).
#[cfg(windows)]
unsafe fn sockaddr_ipv4(sa: *const windows::Win32::Networking::WinSock::SOCKADDR) -> Option<String> {
    use windows::Win32::Networking::WinSock::AF_INET;
    if sa.is_null() {
        return None;
    }
    if (*sa).sa_family != AF_INET {
        return None;
    }
    // SOCKADDR: { u16 family; u8 sa_data[14] }; sa_data[0..2]=port, [2..6]=IPv4.
    let d = &(*sa).sa_data;
    Some(format!(
        "{}.{}.{}.{}",
        d[2] as u8, d[3] as u8, d[4] as u8, d[5] as u8
    ))
}

/// Format the first `len` bytes of a physical address as `AA:BB:CC:...`.
#[cfg(windows)]
fn format_mac(addr: &[u8; 8], len: u32) -> String {
    let n = (len as usize).min(8);
    if n == 0 {
        return String::new();
    }
    addr[..n]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":")
}

/// Query the WLAN service for the current SSID and the list of saved profiles.
#[cfg(windows)]
unsafe fn wlan_info() -> (Option<String>, Vec<String>) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::{
        wlan_intf_opcode_current_connection, WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory,
        WlanGetProfileList, WlanOpenHandle, WlanQueryInterface, WLAN_CONNECTION_ATTRIBUTES,
        WLAN_INTERFACE_INFO_LIST, WLAN_PROFILE_INFO_LIST,
    };

    let mut ssid = None;
    let mut profiles = Vec::new();

    let mut handle = HANDLE::default();
    let mut negotiated = 0u32;
    if WlanOpenHandle(2, None, &mut negotiated, &mut handle) != 0 {
        return (ssid, profiles);
    }

    let mut iface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if WlanEnumInterfaces(handle, None, &mut iface_list) == 0 && !iface_list.is_null() {
        let list = &*iface_list;
        let ifaces = std::slice::from_raw_parts(
            list.InterfaceInfo.as_ptr(),
            list.dwNumberOfItems as usize,
        );
        for iface in ifaces {
            let guid = iface.InterfaceGuid;

            // Current connection -> SSID.
            let mut data_size = 0u32;
            let mut data_ptr: *mut c_void = std::ptr::null_mut();
            if WlanQueryInterface(
                handle,
                &guid,
                wlan_intf_opcode_current_connection,
                None,
                &mut data_size,
                &mut data_ptr,
                None,
            ) == 0
                && !data_ptr.is_null()
            {
                let attr = &*(data_ptr as *const WLAN_CONNECTION_ATTRIBUTES);
                let dot11 = attr.wlanAssociationAttributes.dot11Ssid;
                let len = (dot11.uSSIDLength as usize).min(32);
                let s = String::from_utf8_lossy(&dot11.ucSSID[..len]).to_string();
                if !s.is_empty() {
                    ssid = Some(s);
                }
                WlanFreeMemory(data_ptr);
            }

            // Saved profiles.
            let mut prof_list: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
            if WlanGetProfileList(handle, &guid, None, &mut prof_list) == 0 && !prof_list.is_null() {
                let pl = &*prof_list;
                let items =
                    std::slice::from_raw_parts(pl.ProfileInfo.as_ptr(), pl.dwNumberOfItems as usize);
                for p in items {
                    let name = wide_to_string(&p.strProfileName);
                    if !name.is_empty() {
                        profiles.push(name);
                    }
                }
                WlanFreeMemory(prof_list as *const c_void);
            }
        }
        WlanFreeMemory(iface_list as *const c_void);
    }

    WlanCloseHandle(handle, None);
    (ssid, profiles)
}

/// Fetch a Wi-Fi profile's XML including the plaintext key.
#[cfg(windows)]
unsafe fn wlan_profile_xml(name: &str) -> Result<String> {
    use anyhow::bail;
    use std::ffi::c_void;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::{
        WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetProfile, WlanOpenHandle,
        WLAN_INTERFACE_INFO_LIST,
    };

    const WLAN_PROFILE_GET_PLAINTEXT_KEY: u32 = 4;

    let mut handle = HANDLE::default();
    let mut negotiated = 0u32;
    if WlanOpenHandle(2, None, &mut negotiated, &mut handle) != 0 {
        bail!("failed to open the WLAN service");
    }

    // Use the first wireless interface.
    let mut iface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if WlanEnumInterfaces(handle, None, &mut iface_list) != 0 || iface_list.is_null() {
        WlanCloseHandle(handle, None);
        bail!("no Wi-Fi interface found");
    }
    let list = &*iface_list;
    if list.dwNumberOfItems == 0 {
        WlanFreeMemory(iface_list as *const c_void);
        WlanCloseHandle(handle, None);
        bail!("no Wi-Fi interface found");
    }
    let guid = (*list.InterfaceInfo.as_ptr()).InterfaceGuid;
    WlanFreeMemory(iface_list as *const c_void);

    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut flags: u32 = WLAN_PROFILE_GET_PLAINTEXT_KEY;
    let mut xml_ptr = PWSTR::null();

    let ret = WlanGetProfile(
        handle,
        &guid,
        PCWSTR(name_w.as_ptr()),
        None,
        &mut xml_ptr,
        Some(&mut flags),
        None,
    );

    if ret != 0 || xml_ptr.is_null() {
        WlanCloseHandle(handle, None);
        bail!("no saved profile named '{}'", name);
    }

    let xml = xml_ptr.to_string().unwrap_or_default();
    WlanFreeMemory(xml_ptr.0 as *const c_void);
    WlanCloseHandle(handle, None);
    Ok(xml)
}

/// Read a null-terminated UTF-16 fixed buffer into a String.
#[cfg(windows)]
fn wide_to_string(w: &[u16]) -> String {
    let len = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..len])
}

// Non-Windows stubs.
#[cfg(not(windows))]
pub fn dashboard() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn show_ip() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn show_dns() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn show_mac() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn wifi_list() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn wifi_password(_name: &str) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn wifi_connect(_name: &str, _pass: Option<&str>) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn flush() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn disconnect() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn radio(_on: bool) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn set_dns(_servers: &[String]) -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn dns_auto() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
pub fn reset() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn unsupported() -> Result<()> {
    anyhow::bail!("net is only supported on Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn format_mac_six_bytes() {
        let addr = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0, 0];
        assert_eq!(format_mac(&addr, 6), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn format_mac_empty() {
        assert_eq!(format_mac(&[0; 8], 0), "");
    }

    #[test]
    fn wide_to_string_stops_at_null() {
        let mut w = [0u16; 8];
        for (i, c) in "hi".encode_utf16().enumerate() {
            w[i] = c;
        }
        assert_eq!(wide_to_string(&w), "hi");
    }

    #[test]
    fn extract_key_material_finds_password() {
        let xml = "<foo/><keyMaterial>hunter2</keyMaterial><bar/>";
        assert_eq!(extract_key_material(xml).as_deref(), Some("hunter2"));
    }

    #[test]
    fn extract_key_material_absent() {
        assert_eq!(extract_key_material("<open/>"), None);
    }

    #[test]
    fn build_profile_escapes_and_embeds() {
        let xml = build_profile_xml("My&Net", "p<a>ss");
        assert!(xml.contains("<name>My&amp;Net</name>"));
        assert!(xml.contains("<keyMaterial>p&lt;a&gt;ss</keyMaterial>"));
        assert!(xml.contains("WPA2PSK"));
    }

    #[test]
    fn relevant_excludes_loopback_and_down() {
        let mk = |name: &str, up: bool, ip: bool| Adapter {
            name: name.to_string(),
            mac: String::new(),
            ipv4: if ip { vec!["1.2.3.4".into()] } else { vec![] },
            gateways: vec![],
            dns: vec![],
            up,
        };
        assert!(mk("Wi-Fi", true, true).relevant());
        assert!(!mk("Loopback Pseudo-Interface 1", true, true).relevant());
        assert!(!mk("Ethernet", false, true).relevant());
        assert!(!mk("Ethernet", true, false).relevant());
    }
}
