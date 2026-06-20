use std::num::NonZeroUsize;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(NonZeroUsize);

impl WindowHandle {
    pub fn from_raw_value(value: usize) -> Result<Self> {
        NonZeroUsize::new(value)
            .map(Self)
            .ok_or_else(|| super::platform_error("window handle is null"))
    }

    pub fn from_signed_raw_value(value: isize) -> Result<Self> {
        let value = usize::try_from(value)
            .map_err(|_| super::platform_error("window handle is negative"))?;
        Self::from_raw_value(value)
    }

    pub fn raw_value(self) -> usize {
        self.0.get()
    }

    #[cfg(windows)]
    pub(crate) fn as_hwnd(self) -> windows_sys::Win32::Foundation::HWND {
        self.raw_value() as windows_sys::Win32::Foundation::HWND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_handle_rejects_zero_and_negative_values() {
        assert!(WindowHandle::from_raw_value(0).is_err());
        assert!(WindowHandle::from_signed_raw_value(-1).is_err());
    }

    #[test]
    fn window_handle_preserves_raw_value() {
        let hwnd = WindowHandle::from_raw_value(42).expect("test handle is non-zero");

        assert_eq!(hwnd.raw_value(), 42);
    }
}
