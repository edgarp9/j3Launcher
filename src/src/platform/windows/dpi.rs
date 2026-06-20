use std::sync::atomic::{AtomicBool, Ordering};

use crate::Result;

use super::input::WindowHandle;

pub const BASE_DPI: u32 = 96;

static PROCESS_DPI_CONFIGURED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiAwarenessMethod {
    ProcessDpiAwarenessContext,
    ProcessDpiAwareness,
    ProcessDpiAwareFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiAwarenessOutcome {
    Configured(DpiAwarenessMethod),
    AlreadyConfigured,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiMetrics {
    pub dpi: u32,
    pub scale: f64,
}

pub fn configure_process_dpi_awareness() -> Result<DpiAwarenessOutcome> {
    if PROCESS_DPI_CONFIGURED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(DpiAwarenessOutcome::AlreadyConfigured);
    }

    match configure_process_dpi_awareness_once() {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            PROCESS_DPI_CONFIGURED.store(false, Ordering::SeqCst);
            Err(error)
        }
    }
}

pub fn get_dpi_for_window(hwnd: WindowHandle) -> Result<u32> {
    get_dpi_for_window_impl(hwnd)
}

pub fn get_dpi_for_system() -> Result<u32> {
    get_dpi_for_system_impl()
}

pub fn get_window_dpi_metrics(hwnd: WindowHandle) -> Result<DpiMetrics> {
    let dpi = get_dpi_for_window(hwnd)?;
    Ok(DpiMetrics::from_dpi(dpi))
}

impl DpiMetrics {
    pub fn from_dpi(dpi: u32) -> Self {
        let dpi = dpi.max(BASE_DPI);
        Self {
            dpi,
            scale: f64::from(dpi) / f64::from(BASE_DPI),
        }
    }
}

#[cfg(windows)]
fn configure_process_dpi_awareness_once() -> Result<DpiAwarenessOutcome> {
    use windows_sys::Win32::Foundation::{
        E_ACCESSDENIED, ERROR_ACCESS_DENIED, GetLastError, SetLastError,
    };
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        DPI_AWARENESS_CONTEXT_SYSTEM_AWARE, PROCESS_PER_MONITOR_DPI_AWARE,
        PROCESS_SYSTEM_DPI_AWARE, SetProcessDpiAwareness, SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SetProcessDPIAware;

    for context in [
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
    ] {
        // Safety: SetLastError only mutates this thread's last-error slot.
        unsafe { SetLastError(0) };
        // Safety: the DPI awareness context constants are stable sentinel
        // handles defined by Win32 and do not reference Rust memory.
        let configured = unsafe { SetProcessDpiAwarenessContext(context) != 0 };
        // Safety: GetLastError reads this thread's last-error slot.
        let last_error = unsafe { GetLastError() };
        if configured {
            return Ok(DpiAwarenessOutcome::Configured(
                DpiAwarenessMethod::ProcessDpiAwarenessContext,
            ));
        }
        if last_error == ERROR_ACCESS_DENIED {
            return Ok(DpiAwarenessOutcome::AlreadyConfigured);
        }
    }

    for awareness in [PROCESS_PER_MONITOR_DPI_AWARE, PROCESS_SYSTEM_DPI_AWARE] {
        // Safety: SetProcessDpiAwareness takes a value enum and does not use
        // Rust pointers or retained state beyond process DPI configuration.
        let hr = unsafe { SetProcessDpiAwareness(awareness) };
        if hresult_succeeded(hr) {
            return Ok(DpiAwarenessOutcome::Configured(
                DpiAwarenessMethod::ProcessDpiAwareness,
            ));
        }
        if hr == E_ACCESSDENIED {
            return Ok(DpiAwarenessOutcome::AlreadyConfigured);
        }
    }

    // Safety: SetLastError only mutates this thread's last-error slot.
    unsafe { SetLastError(0) };
    // Safety: SetProcessDPIAware configures process-global DPI awareness and
    // takes no Rust pointers.
    let configured = unsafe { SetProcessDPIAware() != 0 };
    // Safety: GetLastError reads this thread's last-error slot.
    let last_error = unsafe { GetLastError() };
    if configured {
        return Ok(DpiAwarenessOutcome::Configured(
            DpiAwarenessMethod::ProcessDpiAwareFallback,
        ));
    }
    if last_error == ERROR_ACCESS_DENIED {
        return Ok(DpiAwarenessOutcome::AlreadyConfigured);
    }

    Err(super::platform_error(format!(
        "failed to configure process DPI awareness (last_error={last_error})"
    )))
}

#[cfg(not(windows))]
fn configure_process_dpi_awareness_once() -> Result<DpiAwarenessOutcome> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
fn get_dpi_for_window_impl(hwnd: WindowHandle) -> Result<u32> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    // Safety: hwnd is a non-null handle value supplied by the caller. The
    // function only queries DPI and does not retain the handle.
    let dpi = unsafe { GetDpiForWindow(hwnd.as_hwnd()) };
    if dpi == 0 {
        Err(super::platform_error(format!(
            "GetDpiForWindow returned 0 for hwnd={}",
            hwnd.raw_value()
        )))
    } else {
        Ok(dpi)
    }
}

#[cfg(not(windows))]
fn get_dpi_for_window_impl(_hwnd: WindowHandle) -> Result<u32> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
fn get_dpi_for_system_impl() -> Result<u32> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;

    // Safety: GetDpiForSystem takes no pointers and returns the process system DPI.
    let dpi = unsafe { GetDpiForSystem() };
    if dpi == 0 {
        Err(super::platform_error("GetDpiForSystem returned 0"))
    } else {
        Ok(dpi)
    }
}

#[cfg(not(windows))]
fn get_dpi_for_system_impl() -> Result<u32> {
    Err(super::unsupported_platform())
}

fn hresult_succeeded(hr: windows_sys::core::HRESULT) -> bool {
    hr >= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_metrics_clamp_scale_to_base_dpi() {
        assert_eq!(
            DpiMetrics::from_dpi(72),
            DpiMetrics {
                dpi: 96,
                scale: 1.0
            }
        );
        assert_eq!(
            DpiMetrics::from_dpi(144),
            DpiMetrics {
                dpi: 144,
                scale: 1.5
            }
        );
    }

    #[test]
    fn hresult_success_matches_signed_hresult_rule() {
        assert!(hresult_succeeded(0));
        assert!(hresult_succeeded(1));
        assert!(!hresult_succeeded(0x80070005_u32 as i32));
    }
}
