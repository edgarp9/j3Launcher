use crate::Result;

use super::input::WindowHandle;

pub const DARK_TITLEBAR_ATTRIBUTE_IDS: [u32; 2] = [20, 19];
pub const CAPTION_COLOR_ATTRIBUTE_ID: u32 = 35;
pub const TEXT_COLOR_ATTRIBUTE_ID: u32 = 36;
pub const DARK_CAPTION_COLORREF: u32 = 0x00252525;
pub const LIGHT_TEXT_COLORREF: u32 = 0x00FFFFFF;
pub const DEFAULT_COLORREF: u32 = 0xFFFFFFFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DarkTitlebarColors {
    pub caption: u32,
    pub text: u32,
}

impl Default for DarkTitlebarColors {
    fn default() -> Self {
        Self {
            caption: DARK_CAPTION_COLORREF,
            text: LIGHT_TEXT_COLORREF,
        }
    }
}

pub fn apply_dark_titlebar(hwnd: WindowHandle) -> Result<bool> {
    apply_dark_titlebar_with_colors(hwnd, DarkTitlebarColors::default())
}

pub fn apply_light_titlebar(hwnd: WindowHandle) -> Result<bool> {
    apply_light_titlebar_impl(hwnd)
}

pub fn apply_titlebar_theme(hwnd: WindowHandle, dark: bool) -> Result<bool> {
    if dark {
        apply_dark_titlebar(hwnd)
    } else {
        apply_light_titlebar(hwnd)
    }
}

pub fn apply_dark_titlebar_with_colors(
    hwnd: WindowHandle,
    colors: DarkTitlebarColors,
) -> Result<bool> {
    apply_dark_titlebar_impl(hwnd, colors)
}

#[cfg(windows)]
fn apply_dark_titlebar_impl(hwnd: WindowHandle, colors: DarkTitlebarColors) -> Result<bool> {
    ensure_valid_window(hwnd)?;

    let mut immersive_applied = false;
    for attribute in DARK_TITLEBAR_ATTRIBUTE_IDS {
        if set_dwm_i32_attribute(hwnd, attribute, 1).is_ok() {
            immersive_applied = true;
            break;
        }
    }

    if !immersive_applied {
        return Ok(false);
    }

    set_dwm_i32_attribute(hwnd, CAPTION_COLOR_ATTRIBUTE_ID, colors.caption as i32)
        .map_err(dwm_attribute_error)?;
    set_dwm_i32_attribute(hwnd, TEXT_COLOR_ATTRIBUTE_ID, colors.text as i32)
        .map_err(dwm_attribute_error)?;
    Ok(true)
}

#[cfg(windows)]
fn apply_light_titlebar_impl(hwnd: WindowHandle) -> Result<bool> {
    ensure_valid_window(hwnd)?;

    let mut immersive_applied = false;
    for attribute in DARK_TITLEBAR_ATTRIBUTE_IDS {
        if set_dwm_i32_attribute(hwnd, attribute, 0).is_ok() {
            immersive_applied = true;
            break;
        }
    }

    if !immersive_applied {
        return Ok(false);
    }

    let _ = set_dwm_i32_attribute(hwnd, CAPTION_COLOR_ATTRIBUTE_ID, DEFAULT_COLORREF as i32);
    let _ = set_dwm_i32_attribute(hwnd, TEXT_COLOR_ATTRIBUTE_ID, DEFAULT_COLORREF as i32);
    Ok(true)
}

#[cfg(not(windows))]
fn apply_dark_titlebar_impl(_hwnd: WindowHandle, _colors: DarkTitlebarColors) -> Result<bool> {
    Err(super::unsupported_platform())
}

#[cfg(not(windows))]
fn apply_light_titlebar_impl(_hwnd: WindowHandle) -> Result<bool> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DwmAttributeFailure {
    attribute: u32,
    hr: windows_sys::core::HRESULT,
}

#[cfg(windows)]
fn ensure_valid_window(hwnd: WindowHandle) -> Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

    // Safety: hwnd is a non-null handle value supplied by the caller. IsWindow
    // queries the window manager and does not dereference Rust memory.
    if unsafe { IsWindow(hwnd.as_hwnd()) != 0 } {
        Ok(())
    } else {
        Err(super::platform_error(format!(
            "DWM dark titlebar target hwnd is not valid: {}",
            hwnd.raw_value()
        )))
    }
}

#[cfg(windows)]
fn set_dwm_i32_attribute(
    hwnd: WindowHandle,
    attribute: u32,
    value: i32,
) -> std::result::Result<(), DwmAttributeFailure> {
    use std::ffi::c_void;

    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

    // Safety: value points to a valid i32 for the duration of the call, and
    // cbAttribute matches its size. DwmSetWindowAttribute does not retain it.
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd.as_hwnd(),
            attribute,
            (&value as *const i32).cast::<c_void>(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hresult_succeeded(hr) {
        Ok(())
    } else {
        Err(DwmAttributeFailure { attribute, hr })
    }
}

#[cfg(windows)]
fn dwm_attribute_error(failure: DwmAttributeFailure) -> crate::LauncherError {
    super::platform_error(format!(
        "DwmSetWindowAttribute failed for attribute {} ({})",
        failure.attribute,
        format_hresult(failure.hr)
    ))
}

fn format_hresult(hr: windows_sys::core::HRESULT) -> String {
    format!("hr=0x{:08X}", hr as u32)
}

fn hresult_succeeded(hr: windows_sys::core::HRESULT) -> bool {
    hr >= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dark_titlebar_colors_match_original_constants() {
        assert_eq!(
            DarkTitlebarColors::default(),
            DarkTitlebarColors {
                caption: 0x00252525,
                text: 0x00FFFFFF,
            }
        );
        assert_eq!(DEFAULT_COLORREF, 0xFFFFFFFF);
    }

    #[test]
    fn hresult_success_uses_signed_rule() {
        assert!(hresult_succeeded(0));
        assert!(!hresult_succeeded(0x80070057_u32 as i32));
    }

    #[test]
    fn hresult_format_uses_hex_code() {
        assert_eq!(format_hresult(0x80070057_u32 as i32), "hr=0x80070057");
    }
}
