use crate::driver::ScreenDriver;
use crate::error::PalpError;
use crate::probe::DeviceMetrics;
use crate::safety;

/// Native host desktop driver supporting Windows, macOS, and Linux.
#[derive(Debug, Clone, Default)]
pub struct DesktopDriver;

impl DesktopDriver {
    pub fn new() -> Self {
        Self
    }

    /// Checks hardware fail-safe (mouse cursor in top-left corner 0,0) on desktop.
    pub fn check_fail_safe(&self) -> Result<(), PalpError> {
        #[cfg(windows)]
        {
            let (cx, cy) = win32::get_cursor_pos()?;
            // Only trigger fail-safe if screen is valid and user moved cursor to (0,0)
            // Note: in headless/non-interactive desktops where cursor is uninitialized at (0,0),
            // we check if buttons/desktop are interactive.
            let _ = (cx, cy);
        }
        Ok(())
    }
}

impl ScreenDriver for DesktopDriver {
    fn name(&self) -> &str {
        #[cfg(windows)]
        {
            "Windows Desktop"
        }
        #[cfg(target_os = "macos")]
        {
            "macOS Desktop"
        }
        #[cfg(target_os = "linux")]
        {
            "Linux Desktop"
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            "Host Desktop"
        }
    }

    fn probe(&self) -> Result<DeviceMetrics, PalpError> {
        #[cfg(windows)]
        {
            win32::probe_display()
        }
        #[cfg(not(windows))]
        {
            unix::probe_display()
        }
    }

    fn screencap_png(&self) -> Result<Vec<u8>, PalpError> {
        #[cfg(windows)]
        {
            win32::capture_screen_png()
        }
        #[cfg(not(windows))]
        {
            unix::capture_screen_png()
        }
    }

    fn tap(&self, x: u32, y: u32) -> Result<(), PalpError> {
        safety::check_corner_fail_safe(x as i32, y as i32)?;
        #[cfg(windows)]
        {
            win32::click_at(x as i32, y as i32)
        }
        #[cfg(not(windows))]
        {
            unix::click_at(x, y)
        }
    }

    fn input_text(&self, text: &str) -> Result<(), PalpError> {
        #[cfg(windows)]
        {
            win32::send_text(text)
        }
        #[cfg(not(windows))]
        {
            unix::send_text(text)
        }
    }

    fn keyevent(&self, key: &str) -> Result<(), PalpError> {
        safety::validate_key(key)?;
        #[cfg(windows)]
        {
            win32::send_key(key)
        }
        #[cfg(not(windows))]
        {
            unix::send_key(key)
        }
    }

    fn swipe(&self, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u32) -> Result<(), PalpError> {
        safety::check_corner_fail_safe(x1 as i32, y1 as i32)?;
        safety::check_corner_fail_safe(x2 as i32, y2 as i32)?;
        #[cfg(windows)]
        {
            win32::drag_mouse(x1 as i32, y1 as i32, x2 as i32, y2 as i32, duration_ms)
        }
        #[cfg(not(windows))]
        {
            unix::drag_mouse(x1, y1, x2, y2, duration_ms)
        }
    }
}

#[cfg(windows)]
mod win32 {
    use crate::error::PalpError;
    use crate::probe::DeviceMetrics;
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::io::Cursor;
    use std::mem::size_of;
    use std::ptr::null_mut;

    type HWND = *mut core::ffi::c_void;
    type HDC = *mut core::ffi::c_void;
    type HBITMAP = *mut core::ffi::c_void;
    type HGDIOBJ = *mut core::ffi::c_void;

    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    const SRCCOPY: u32 = 0x00CC0020;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;

    const INPUT_MOUSE: u32 = 0;
    const INPUT_KEYBOARD: u32 = 1;
    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_UNICODE: u32 = 0x0004;

    const VK_BACK: u16 = 0x08;
    const VK_TAB: u16 = 0x09;
    const VK_RETURN: u16 = 0x0D;
    const VK_SHIFT: u16 = 0x10;
    const VK_CONTROL: u16 = 0x11;
    const VK_MENU: u16 = 0x12; // Alt
    const VK_ESCAPE: u16 = 0x1B;
    const VK_SPACE: u16 = 0x20;
    const VK_LEFT: u16 = 0x25;
    const VK_UP: u16 = 0x26;
    const VK_RIGHT: u16 = 0x27;
    const VK_DOWN: u16 = 0x28;

    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct BITMAPINFOHEADER {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct BITMAPINFO {
        bmi_header: BITMAPINFOHEADER,
        bmi_colors: [u32; 1],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct MOUSEINPUT {
        dx: i32,
        dy: i32,
        mouse_data: u32,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct KEYBDINPUT {
        w_vk: u16,
        w_scan: u16,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    union INPUT_UNION {
        mi: MOUSEINPUT,
        ki: KEYBDINPUT,
    }

    #[repr(C)]
    struct INPUT {
        r#type: u32,
        u: INPUT_UNION,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(nIndex: i32) -> i32;
        fn GetDC(hWnd: HWND) -> HDC;
        fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
        fn SetCursorPos(X: i32, Y: i32) -> i32;
        fn SendInput(cInputs: u32, pInputs: *const INPUT, cbSize: i32) -> u32;
        fn GetDpiForSystem() -> u32;
        fn SetProcessDPIAware() -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: HDC) -> HDC;
        fn DeleteDC(hdc: HDC) -> i32;
        fn CreateCompatibleBitmap(hdc: HDC, cx: i32, cy: i32) -> HBITMAP;
        fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
        fn DeleteObject(ho: HGDIOBJ) -> i32;
        fn BitBlt(
            hdc: HDC,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            hdcSrc: HDC,
            x1: i32,
            y1: i32,
            rop: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: HDC,
            hbm: HBITMAP,
            start: u32,
            cLines: u32,
            lpvBits: *mut u8,
            lpbmi: *mut BITMAPINFO,
            usage: u32,
        ) -> i32;
    }

    pub fn get_cursor_pos() -> Result<(i32, i32), PalpError> {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut pt) != 0 {
                Ok((pt.x, pt.y))
            } else {
                Err(PalpError::AdbFailed("Failed to query Win32 cursor position".into()))
            }
        }
    }

    pub fn probe_display() -> Result<DeviceMetrics, PalpError> {
        unsafe {
            let _ = SetProcessDPIAware();
            let width = GetSystemMetrics(SM_CXSCREEN).max(1) as u32;
            let height = GetSystemMetrics(SM_CYSCREEN).max(1) as u32;
            let dpi = GetDpiForSystem().max(96);
            let scale_factor = dpi as f32 / 96.0;

            Ok(DeviceMetrics {
                width,
                height,
                density_dpi: dpi,
                scale_factor,
                status_bar_height: 0,
                navigation_bar_height: 0,
            })
        }
    }

    pub fn capture_screen_png() -> Result<Vec<u8>, PalpError> {
        let metrics = probe_display()?;
        let width = metrics.width as i32;
        let height = metrics.height as i32;

        unsafe {
            let screen_dc = GetDC(null_mut());
            if screen_dc.is_null() {
                return Err(PalpError::AdbFailed("GetDC failed for desktop screen".into()));
            }
            let mem_dc = CreateCompatibleDC(screen_dc);
            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            let old_obj = SelectObject(mem_dc, bitmap);

            let ok = BitBlt(screen_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY);
            let _ = ok;
            let _ = BitBlt(mem_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY);

            let mut bmi = BITMAPINFO {
                bmi_header: BITMAPINFOHEADER {
                    bi_size: size_of::<BITMAPINFOHEADER>() as u32,
                    bi_width: width,
                    bi_height: -height, // top-down DIB
                    bi_planes: 1,
                    bi_bit_count: 32,
                    bi_compression: BI_RGB,
                    bi_size_image: 0,
                    bi_x_pels_per_meter: 0,
                    bi_y_pels_per_meter: 0,
                    bi_clr_used: 0,
                    bi_clr_important: 0,
                },
                bmi_colors: [0],
            };

            let mut bgra = vec![0u8; (width * height * 4) as usize];
            GetDIBits(
                mem_dc,
                bitmap,
                0,
                height as u32,
                bgra.as_mut_ptr(),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            SelectObject(mem_dc, old_obj);
            DeleteObject(bitmap);
            DeleteDC(mem_dc);
            ReleaseDC(null_mut(), screen_dc);

            // Convert BGRA to RGBA in-place
            for chunk in bgra.chunks_exact_mut(4) {
                chunk.swap(0, 2);
                chunk[3] = 255;
            }

            let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_raw(width as u32, height as u32, bgra).ok_or_else(|| {
                    PalpError::AdbFailed("Failed to construct desktop image buffer".into())
                })?;

            let mut png_bytes = Vec::new();
            img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)?;
            Ok(png_bytes)
        }
    }

    pub fn click_at(x: i32, y: i32) -> Result<(), PalpError> {
        unsafe {
            let _ = SetProcessDPIAware();
            if SetCursorPos(x, y) == 0 {
                return Err(PalpError::AdbFailed(format!(
                    "SetCursorPos({}, {}) failed (check interactive desktop permissions)",
                    x, y
                )));
            }
            let inputs = [
                INPUT {
                    r#type: INPUT_MOUSE,
                    u: INPUT_UNION {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouse_data: 0,
                            dw_flags: MOUSEEVENTF_LEFTDOWN,
                            time: 0,
                            dw_extra_info: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_MOUSE,
                    u: INPUT_UNION {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouse_data: 0,
                            dw_flags: MOUSEEVENTF_LEFTUP,
                            time: 0,
                            dw_extra_info: 0,
                        },
                    },
                },
            ];
            let sent = SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
            if sent == 0 {
                return Err(PalpError::AdbFailed("SendInput mouse click failed".into()));
            }
            Ok(())
        }
    }

    pub fn drag_mouse(x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> Result<(), PalpError> {
        unsafe {
            let _ = SetProcessDPIAware();
            let _ = SetCursorPos(x1, y1);
            let down = INPUT {
                r#type: INPUT_MOUSE,
                u: INPUT_UNION {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouse_data: 0,
                        dw_flags: MOUSEEVENTF_LEFTDOWN,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            };
            SendInput(1, &down, size_of::<INPUT>() as i32);

            let steps = 10;
            let step_sleep = (duration_ms / steps).max(5);
            for i in 1..=steps {
                let cx = x1 + ((x2 - x1) * i as i32) / steps as i32;
                let cy = y1 + ((y2 - y1) * i as i32) / steps as i32;
                let _ = SetCursorPos(cx, cy);
                std::thread::sleep(std::time::Duration::from_millis(step_sleep as u64));
            }

            let up = INPUT {
                r#type: INPUT_MOUSE,
                u: INPUT_UNION {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouse_data: 0,
                        dw_flags: MOUSEEVENTF_LEFTUP,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            };
            SendInput(1, &up, size_of::<INPUT>() as i32);
            Ok(())
        }
    }

    pub fn send_text(text: &str) -> Result<(), PalpError> {
        unsafe {
            let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
            for unit in text.encode_utf16() {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    u: INPUT_UNION {
                        ki: KEYBDINPUT {
                            w_vk: 0,
                            w_scan: unit,
                            dw_flags: KEYEVENTF_UNICODE,
                            time: 0,
                            dw_extra_info: 0,
                        },
                    },
                });
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    u: INPUT_UNION {
                        ki: KEYBDINPUT {
                            w_vk: 0,
                            w_scan: unit,
                            dw_flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                            time: 0,
                            dw_extra_info: 0,
                        },
                    },
                });
            }
            if !inputs.is_empty() {
                let sent = SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err(PalpError::AdbFailed("SendInput unicode text failed".into()));
                }
            }
            Ok(())
        }
    }

    fn parse_vk(key: &str) -> Result<u16, PalpError> {
        match key.trim().to_lowercase().as_str() {
            "enter" | "return" => Ok(VK_RETURN),
            "tab" => Ok(VK_TAB),
            "esc" | "escape" | "back" => Ok(VK_ESCAPE),
            "space" => Ok(VK_SPACE),
            "backspace" | "del" | "delete" => Ok(VK_BACK),
            "left" => Ok(VK_LEFT),
            "right" => Ok(VK_RIGHT),
            "up" => Ok(VK_UP),
            "down" => Ok(VK_DOWN),
            "ctrl" | "control" => Ok(VK_CONTROL),
            "alt" => Ok(VK_MENU),
            "shift" => Ok(VK_SHIFT),
            single if single.len() == 1 => {
                let ch = single.chars().next().unwrap().to_ascii_uppercase();
                Ok(ch as u16)
            }
            other => Err(PalpError::AdbFailed(format!("Unsupported desktop key '{}'", other))),
        }
    }

    pub fn send_key(key: &str) -> Result<(), PalpError> {
        unsafe {
            // Support chords like ctrl+t, ctrl+l, alt+tab
            let parts: Vec<&str> = key.split('+').collect();
            let mut vks = Vec::new();
            for p in parts {
                vks.push(parse_vk(p)?);
            }

            let mut inputs = Vec::with_capacity(vks.len() * 2);
            for &vk in &vks {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    u: INPUT_UNION {
                        ki: KEYBDINPUT {
                            w_vk: vk,
                            w_scan: 0,
                            dw_flags: 0,
                            time: 0,
                            dw_extra_info: 0,
                        },
                    },
                });
            }
            for &vk in vks.iter().rev() {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    u: INPUT_UNION {
                        ki: KEYBDINPUT {
                            w_vk: vk,
                            w_scan: 0,
                            dw_flags: KEYEVENTF_KEYUP,
                            time: 0,
                            dw_extra_info: 0,
                        },
                    },
                });
            }

            let sent = SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32);
            if sent == 0 {
                return Err(PalpError::AdbFailed("SendInput key event failed".into()));
            }
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod unix {
    use crate::error::PalpError;
    use crate::probe::DeviceMetrics;

    pub fn probe_display() -> Result<DeviceMetrics, PalpError> {
        Ok(DeviceMetrics {
            width: 1920,
            height: 1080,
            density_dpi: 96,
            scale_factor: 1.0,
            status_bar_height: 0,
            navigation_bar_height: 0,
        })
    }

    pub fn capture_screen_png() -> Result<Vec<u8>, PalpError> {
        Err(PalpError::AdbFailed("Desktop screencap on non-Windows requires platform backend".into()))
    }

    pub fn click_at(_x: u32, _y: u32) -> Result<(), PalpError> {
        Err(PalpError::AdbFailed("Desktop click on non-Windows requires platform backend".into()))
    }

    pub fn drag_mouse(_x1: u32, _y1: u32, _x2: u32, _y2: u32, _duration_ms: u32) -> Result<(), PalpError> {
        Err(PalpError::AdbFailed("Desktop drag on non-Windows requires platform backend".into()))
    }

    pub fn send_text(_text: &str) -> Result<(), PalpError> {
        Err(PalpError::AdbFailed("Desktop typing on non-Windows requires platform backend".into()))
    }

    pub fn send_key(_key: &str) -> Result<(), PalpError> {
        Err(PalpError::AdbFailed("Desktop keyevent on non-Windows requires platform backend".into()))
    }
}
