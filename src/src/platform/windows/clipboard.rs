use crate::Result;

#[cfg(windows)]
pub fn set_clipboard_text(text: &str) -> Result<()> {
    use std::mem::size_of;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::SetLastError;
    use windows_sys::Win32::System::DataExchange::{
        EmptyClipboard, OpenClipboard, SetClipboardData,
    };

    const CF_UNICODETEXT: u32 = 13;

    let wide = clipboard_wide_text(text);
    let byte_len = wide
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| super::platform_error("clipboard text is too large"))?;

    let mut memory = GlobalMemory::new(byte_len)?;
    memory.write_utf16(&wide)?;

    unsafe { SetLastError(0) };
    let opened = unsafe {
        // Safety: null owner is allowed by OpenClipboard and transfers no ownership.
        OpenClipboard(null_mut())
    };
    if opened == 0 {
        return Err(last_error("OpenClipboard"));
    }
    let _clipboard = ClipboardGuard;

    unsafe { SetLastError(0) };
    let emptied = unsafe {
        // Safety: the clipboard is open on this thread while ClipboardGuard is alive.
        EmptyClipboard()
    };
    if emptied == 0 {
        return Err(last_error("EmptyClipboard"));
    }

    unsafe { SetLastError(0) };
    let stored = unsafe {
        // Safety: memory contains a NUL-terminated UTF-16 buffer allocated with
        // GlobalAlloc(GMEM_MOVEABLE), as required for CF_UNICODETEXT.
        SetClipboardData(CF_UNICODETEXT, memory.handle())
    };
    if stored.is_null() {
        return Err(last_error("SetClipboardData"));
    }
    memory.release();
    Ok(())
}

#[cfg(not(windows))]
pub fn set_clipboard_text(_text: &str) -> Result<()> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
struct ClipboardGuard;

#[cfg(windows)]
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::System::DataExchange::CloseClipboard;

        unsafe {
            // Safety: this guard is only constructed after OpenClipboard succeeds.
            CloseClipboard();
        }
    }
}

#[cfg(windows)]
struct GlobalMemory {
    handle: windows_sys::Win32::Foundation::HGLOBAL,
}

#[cfg(windows)]
impl GlobalMemory {
    fn new(byte_len: usize) -> Result<Self> {
        use windows_sys::Win32::Foundation::SetLastError;
        use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc};

        unsafe { SetLastError(0) };
        let handle = unsafe {
            // Safety: GlobalAlloc is called with documented flags and a checked size.
            GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, byte_len)
        };
        if handle.is_null() {
            Err(last_error("GlobalAlloc"))
        } else {
            Ok(Self { handle })
        }
    }

    fn handle(&self) -> windows_sys::Win32::Foundation::HGLOBAL {
        self.handle
    }

    fn write_utf16(&mut self, wide: &[u16]) -> Result<()> {
        use std::ptr::copy_nonoverlapping;

        use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
        use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};

        unsafe { SetLastError(0) };
        let locked = unsafe {
            // Safety: handle is owned by this wrapper and has not been released.
            GlobalLock(self.handle)
        };
        if locked.is_null() {
            return Err(last_error("GlobalLock"));
        }

        unsafe {
            // Safety: the allocation size was computed from wide.len(), and the
            // locked pointer remains valid until GlobalUnlock below.
            copy_nonoverlapping(wide.as_ptr(), locked.cast::<u16>(), wide.len());
        }

        unsafe { SetLastError(0) };
        let unlocked = unsafe {
            // Safety: locked was returned by GlobalLock for this handle.
            GlobalUnlock(self.handle)
        };
        if unlocked == 0 {
            let last_error = unsafe {
                // Safety: GetLastError reads this thread's last-error slot.
                GetLastError()
            };
            if last_error != 0 {
                return Err(super::platform_error(format!(
                    "GlobalUnlock failed (last_error={last_error})"
                )));
            }
        }
        Ok(())
    }

    fn release(&mut self) {
        self.handle = std::ptr::null_mut();
    }
}

#[cfg(windows)]
impl Drop for GlobalMemory {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            use windows_sys::Win32::Foundation::GlobalFree;

            unsafe {
                // Safety: this wrapper owns the handle until SetClipboardData succeeds.
                GlobalFree(self.handle);
            }
        }
    }
}

#[cfg(windows)]
fn clipboard_wide_text(text: &str) -> Vec<u16> {
    text.encode_utf16()
        .map(|unit| if unit == 0 { 0xFFFD } else { unit })
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn last_error(operation: &str) -> crate::LauncherError {
    let last_error = unsafe {
        // Safety: GetLastError reads this thread's last-error slot.
        windows_sys::Win32::Foundation::GetLastError()
    };
    super::platform_error(format!("{operation} failed (last_error={last_error})"))
}
