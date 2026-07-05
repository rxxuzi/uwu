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
    for name in profiles {
        if Some(&name) == current.as_ref() {
            println!("    {} {}", color::accent(&name), color::success("(connected)"));
        } else {
            println!("    {}", name);
        }
    }
    Ok(())
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
