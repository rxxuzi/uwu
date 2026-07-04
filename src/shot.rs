//! Screenshot module for uwu.
//!
//! Captures the screen natively via the Win32 GDI API (no PowerShell — inline
//! screen-capture scripts get flagged by AMSI as malicious). Full-screen and
//! active-window captures use BitBlt + GetDIBits; output is a PNG file or the
//! clipboard (CF_DIB). Interactive region selection launches the OS overlay
//! (`ms-screenclip:`) via ShellExecute, which is AMSI-independent.

use anyhow::{bail, Result};
#[cfg(windows)]
use anyhow::Context;
#[cfg(windows)]
use std::path::{Path, PathBuf};

/// Capture a screenshot.
///
/// # Arguments
/// * `file` - Explicit output path; `None` auto-names `uwu_screenshot_<timestamp>.png`
/// * `window` - Capture the active foreground window instead of the full screen
/// * `region` - Interactive region selection (Win+Shift+S style)
/// * `clip` - Copy to the clipboard instead of saving a file
/// * `delay` - Seconds to wait before capturing (handy with `--window`)
///
/// Returns the saved file path (or `None` when copied to the clipboard).
#[cfg(windows)]
pub fn capture(
    file: Option<&str>,
    window: bool,
    region: bool,
    clip: bool,
    delay: u64,
) -> Result<Option<String>> {
    if window && region {
        bail!("--window and --region cannot be combined");
    }

    if delay > 0 {
        std::thread::sleep(std::time::Duration::from_secs(delay));
    }

    // Region selection uses the OS overlay, which delivers to the clipboard.
    if region {
        win::launch_region()?;
        if clip {
            return Ok(None);
        }
        // Poll the clipboard for the freshly-snipped image, then save it.
        let path = resolve_path(file);
        win::save_clipboard_image(&path)?;
        return Ok(Some(path));
    }

    // Determine the capture rectangle.
    let rect = if window {
        win::foreground_window_rect()?
    } else {
        win::virtual_screen_rect()
    };

    if clip {
        win::capture_to_clipboard(rect)?;
        Ok(None)
    } else {
        let path = resolve_path(file);
        win::capture_to_png(rect, &path)?;
        Ok(Some(path))
    }
}

#[cfg(not(windows))]
pub fn capture(
    _file: Option<&str>,
    _window: bool,
    _region: bool,
    _clip: bool,
    _delay: u64,
) -> Result<Option<String>> {
    bail!("shot is only supported on Windows");
}

/// Resolve the output path: use the given name (making it absolute) or an
/// auto-generated `uwu_screenshot_<timestamp>.png`. A missing extension → `.png`.
#[cfg(windows)]
fn resolve_path(file: Option<&str>) -> String {
    let name = file
        .map(|s| s.to_string())
        .unwrap_or_else(default_filename);

    let mut path = if Path::new(&name).is_absolute() {
        PathBuf::from(&name)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&name)
    };

    if path.extension().is_none() {
        path.set_extension("png");
    }

    path.to_string_lossy().to_string()
}

/// Auto-generated file name using the local time: `uwu_screenshot_YYYYMMDD_HHMMSS.png`.
#[cfg(windows)]
fn default_filename() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    format!(
        "uwu_screenshot_{:04}{:02}{:02}_{:02}{:02}{:02}.png",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// Convert a top-down BGRA buffer (as GDI gives us) to opaque RGBA for PNG.
/// Screen BitBlt leaves the alpha channel as 0, so we force it to 255.
#[cfg(windows)]
fn bgra_to_rgba_opaque(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = vec![0u8; bgra.len()];
    for (src, dst) in bgra.chunks_exact(4).zip(rgba.chunks_exact_mut(4)) {
        dst[0] = src[2]; // R
        dst[1] = src[1]; // G
        dst[2] = src[0]; // B
        dst[3] = 255; // A (opaque)
    }
    rgba
}

/// A capture rectangle in virtual-screen coordinates.
#[cfg(windows)]
#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HANDLE, HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_DIB;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWNORMAL,
    };

    /// The full virtual screen (spans all monitors).
    pub fn virtual_screen_rect() -> Rect {
        unsafe {
            Rect {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                w: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                h: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }

    /// Rect of the active foreground window.
    pub fn foreground_window_rect() -> Result<Rect> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0 == 0 {
                bail!("no active window found");
            }
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok().context("GetWindowRect failed")?;
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            if w <= 0 || h <= 0 {
                bail!("active window has an invalid size");
            }
            Ok(Rect {
                x: r.left,
                y: r.top,
                w,
                h,
            })
        }
    }

    /// Grab pixels for `rect`. Returns a BGRA buffer; `top_down` controls row order
    /// (top-down for PNG, bottom-up for a CF_DIB clipboard payload).
    unsafe fn grab(rect: Rect, top_down: bool) -> Result<Vec<u8>> {
        let Rect { x, y, w, h } = rect;

        let screen = GetDC(HWND(0));
        if screen.is_invalid() {
            bail!("GetDC failed");
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));

        let blt = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT);

        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = if top_down { -h } else { h };
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0 as u32;

        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let scanned = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Clean up GDI objects.
        SelectObject(mem, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(HWND(0), screen);

        if blt.is_err() {
            bail!("BitBlt failed (screen capture)");
        }
        if scanned == 0 {
            bail!("GetDIBits failed (reading pixels)");
        }

        Ok(buf)
    }

    /// Capture `rect` and write it to `path` as a PNG.
    pub fn capture_to_png(rect: Rect, path: &str) -> Result<()> {
        let bgra = unsafe { grab(rect, true)? };
        write_png(path, &bgra, rect.w, rect.h)
    }

    /// Capture `rect` and place it on the clipboard as CF_DIB.
    pub fn capture_to_clipboard(rect: Rect) -> Result<()> {
        let bgra = unsafe { grab(rect, false)? }; // bottom-up for DIB
        unsafe { set_clipboard_dib(&bgra, rect.w, rect.h) }
    }

    /// Encode `bgra` (top-down) to a PNG file at `path`.
    fn write_png(path: &str, bgra: &[u8], w: i32, h: i32) -> Result<()> {
        let rgba = bgra_to_rgba_opaque(bgra);
        let file = std::fs::File::create(path)
            .with_context(|| format!("cannot create file: {}", path))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("png header")?;
        writer.write_image_data(&rgba).context("png data")?;
        Ok(())
    }

    /// Build a packed DIB (BITMAPINFOHEADER + bottom-up BGRA) and put it on the clipboard.
    unsafe fn set_clipboard_dib(bgra_bottom_up: &[u8], w: i32, h: i32) -> Result<()> {
        let header = size_of::<BITMAPINFOHEADER>();
        let total = header + bgra_bottom_up.len();

        let hglobal = GlobalAlloc(GMEM_MOVEABLE, total).context("GlobalAlloc failed")?;
        let ptr = GlobalLock(hglobal) as *mut u8;
        if ptr.is_null() {
            bail!("GlobalLock failed");
        }

        let mut hdr = BITMAPINFOHEADER::default();
        hdr.biSize = header as u32;
        hdr.biWidth = w;
        hdr.biHeight = h; // positive = bottom-up
        hdr.biPlanes = 1;
        hdr.biBitCount = 32;
        hdr.biCompression = BI_RGB.0 as u32;
        hdr.biSizeImage = bgra_bottom_up.len() as u32;

        std::ptr::copy_nonoverlapping(&hdr as *const _ as *const u8, ptr, header);
        std::ptr::copy_nonoverlapping(bgra_bottom_up.as_ptr(), ptr.add(header), bgra_bottom_up.len());
        let _ = GlobalUnlock(hglobal);

        OpenClipboard(HWND(0)).ok().context("OpenClipboard failed")?;
        let _ = EmptyClipboard();
        // On success the clipboard owns hglobal; we must not free it.
        let set = SetClipboardData(CF_DIB.0 as u32, HANDLE(hglobal.0 as isize));
        let _ = CloseClipboard();
        set.context("SetClipboardData failed")?;

        Ok(())
    }

    /// Launch the interactive region-selection overlay (Win+Shift+S).
    pub fn launch_region() -> Result<()> {
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let target: Vec<u16> = "ms-screenclip:\0".encode_utf16().collect();
        let inst = unsafe {
            ShellExecuteW(
                HWND(0),
                PCWSTR(verb.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns > 32 on success.
        if inst.0 as isize <= 32 {
            bail!("failed to launch the region selector (ms-screenclip)");
        }
        Ok(())
    }

    /// Poll the clipboard for an image (from the region overlay) and save it as PNG.
    pub fn save_clipboard_image(path: &str) -> Result<()> {
        // Wait up to ~60s for the user to finish snipping.
        for _ in 0..240 {
            if unsafe { IsClipboardFormatAvailable(CF_DIB.0 as u32).is_ok() } {
                if let Some((bgra, w, h)) = unsafe { read_clipboard_dib() } {
                    return write_png(path, &bgra, w, h);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        bail!("no image captured (region selection cancelled or timed out)")
    }

    /// Read a CF_DIB payload from the clipboard into a top-down BGRA buffer.
    /// Supports the common 32/24-bit uncompressed DIBs the snip tool produces.
    unsafe fn read_clipboard_dib() -> Option<(Vec<u8>, i32, i32)> {
        if OpenClipboard(HWND(0)).is_err() {
            return None;
        }
        let result = (|| {
            let handle = GetClipboardData(CF_DIB.0 as u32).ok()?;
            let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0 as *mut c_void);
            let ptr = GlobalLock(hglobal) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let hdr = &*(ptr as *const BITMAPINFOHEADER);
            let w = hdr.biWidth;
            let h_raw = hdr.biHeight;
            let bits = hdr.biBitCount as i32;
            if w <= 0 || h_raw == 0 || (bits != 32 && bits != 24) {
                let _ = GlobalUnlock(hglobal);
                return None;
            }
            let h = h_raw.abs();
            let top_down = h_raw < 0;
            let src_stride = (((w * bits + 31) / 32) * 4) as usize;
            let pixels = ptr.add(hdr.biSize as usize);

            let mut bgra = vec![0u8; (w as usize) * (h as usize) * 4];
            for row in 0..h as usize {
                let src_row = if top_down { row } else { h as usize - 1 - row };
                let src = pixels.add(src_row * src_stride);
                for col in 0..w as usize {
                    let sp = src.add(col * (bits as usize / 8));
                    let d = (row * w as usize + col) * 4;
                    bgra[d] = *sp; // B
                    bgra[d + 1] = *sp.add(1); // G
                    bgra[d + 2] = *sp.add(2); // R
                    bgra[d + 3] = 255;
                }
            }
            let _ = GlobalUnlock(hglobal);
            Some((bgra, w, h))
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn resolve_appends_png_when_no_extension() {
        let p = resolve_path(Some(r"C:\tmp\shot"));
        assert!(p.ends_with(r"\shot.png"));
    }

    #[test]
    fn resolve_keeps_given_png_name() {
        let p = resolve_path(Some(r"C:\tmp\bug.png"));
        assert_eq!(p, r"C:\tmp\bug.png");
    }

    #[test]
    fn resolve_makes_relative_absolute() {
        let p = resolve_path(Some("shot.png"));
        assert!(Path::new(&p).is_absolute());
    }

    #[test]
    fn resolve_none_uses_timestamp_name() {
        let p = resolve_path(None);
        assert!(p.contains("uwu_screenshot_"));
        assert!(p.ends_with(".png"));
    }

    #[test]
    fn default_filename_shape() {
        let name = default_filename();
        // uwu_screenshot_YYYYMMDD_HHMMSS.png
        assert!(name.starts_with("uwu_screenshot_"));
        assert!(name.ends_with(".png"));
        let digits = "uwu_screenshot_".len() + 8 + 1 + 6; // + "_" + time
        assert_eq!(name.len(), digits + ".png".len());
    }

    #[test]
    fn bgra_to_rgba_swaps_channels_and_forces_alpha() {
        // one pixel: B=10 G=20 R=30 A=0  ->  R=30 G=20 B=10 A=255
        let bgra = [10u8, 20, 30, 0];
        let rgba = bgra_to_rgba_opaque(&bgra);
        assert_eq!(rgba, vec![30, 20, 10, 255]);
    }
}
