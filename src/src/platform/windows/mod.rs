use crate::{LauncherError, Result};

pub mod clipboard;
pub mod dialogs;
pub mod dpi;
pub mod dwm;
pub mod icon;
pub mod input;
pub mod shell;

mod wide;

#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

#[cfg(windows)]
pub fn initialize_process_dpi_awareness() -> Result<()> {
    dpi::configure_process_dpi_awareness().map(|_| ())
}

#[cfg(not(windows))]
pub fn initialize_process_dpi_awareness() -> Result<()> {
    Err(unsupported_platform())
}

#[cfg(windows)]
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    let from_wide = wide::path_to_wide_z(from)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let to_wide = wide::path_to_wide_z(to)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;

    // Safety: both buffers are validated NUL-terminated UTF-16 paths and live
    // for this call. MoveFileExW does not retain either pointer.
    let moved = unsafe { MoveFileExW(from_wide.as_ptr(), to_wide.as_ptr(), flags) };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn platform_error(message: impl Into<String>) -> LauncherError {
    LauncherError::Platform {
        message: message.into(),
    }
}

#[cfg(not(windows))]
fn unsupported_platform() -> LauncherError {
    LauncherError::UnsupportedPlatform {
        platform: std::env::consts::OS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_error_preserves_message() {
        let error = platform_error("win32 failed");

        assert_eq!(error.user_message(), "win32 failed");
    }
}
