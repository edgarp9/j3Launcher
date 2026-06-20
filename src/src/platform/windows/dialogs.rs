use std::path::PathBuf;

use crate::Result;

use super::input::WindowHandle;

pub fn pick_folder(owner: Option<WindowHandle>, title: impl AsRef<str>) -> Result<Option<PathBuf>> {
    if let Some(result) = debug_folder_picker_override() {
        return result;
    }
    pick_folder_impl(owner, title.as_ref())
}

#[cfg(debug_assertions)]
fn debug_folder_picker_override() -> Option<Result<Option<PathBuf>>> {
    debug_folder_picker_override_from_env(
        std::env::var_os("J3LAUNCHER_TEST_PICK_FOLDER"),
        std::env::var_os("J3LAUNCHER_TEST_PICK_FOLDER_ERROR"),
    )
}

#[cfg(not(debug_assertions))]
fn debug_folder_picker_override() -> Option<Result<Option<PathBuf>>> {
    None
}

#[cfg(debug_assertions)]
fn debug_folder_picker_override_from_env(
    selected_path: Option<std::ffi::OsString>,
    error_message: Option<std::ffi::OsString>,
) -> Option<Result<Option<PathBuf>>> {
    if let Some(message) = error_message {
        return Some(Err(super::platform_error(
            message.to_string_lossy().into_owned(),
        )));
    }
    let path = selected_path?;
    if path.as_os_str() == std::ffi::OsStr::new("__CANCEL__") {
        Some(Ok(None))
    } else {
        Some(Ok(Some(PathBuf::from(path))))
    }
}

#[cfg(windows)]
fn pick_folder_impl(owner: Option<WindowHandle>, title: &str) -> Result<Option<PathBuf>> {
    use std::ptr::null_mut;

    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW,
        SHGetPathFromIDListW,
    };

    let _com = FolderDialogComGuard::initialize()?;
    let title_wide = super::wide::WideString::from_str("folder dialog title", title)?;
    let mut display_name = vec![0u16; 260];
    let browse_info = BROWSEINFOW {
        hwndOwner: owner.map_or(null_mut(), WindowHandle::as_hwnd),
        pidlRoot: null_mut(),
        pszDisplayName: display_name.as_mut_ptr(),
        lpszTitle: title_wide.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        lpfn: None,
        lParam: 0,
        iImage: 0,
    };

    // Safety: browse_info points to initialized data that lives for this call.
    // SHBrowseForFolderW returns a PIDL allocated by the shell allocator.
    let pidl = unsafe { SHBrowseForFolderW(&browse_info) };
    if pidl.is_null() {
        return Ok(None);
    }

    let mut buffer = vec![0u16; 32768];
    // Safety: pidl is the non-null PIDL returned by SHBrowseForFolderW and
    // buffer is a writable NUL-terminated path buffer.
    let ok = unsafe { SHGetPathFromIDListW(pidl, buffer.as_mut_ptr()) };
    // Safety: pidl is released exactly once with the shell allocator.
    unsafe { CoTaskMemFree(pidl.cast()) };
    if ok == 0 {
        return Err(super::platform_error(
            "선택한 폴더 경로를 확인할 수 없습니다.",
        ));
    }

    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    let path = String::from_utf16_lossy(&buffer[..end]);
    if path.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}

#[cfg(not(windows))]
fn pick_folder_impl(_owner: Option<WindowHandle>, _title: &str) -> Result<Option<PathBuf>> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
struct FolderDialogComGuard {
    initialized: bool,
}

#[cfg(windows)]
impl FolderDialogComGuard {
    fn initialize() -> Result<Self> {
        use std::ptr::null;

        use windows_sys::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
        use windows_sys::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
        };

        let flags = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32;
        // Safety: CoInitializeEx receives a null reserved pointer as required by
        // Win32 and initializes COM for the current thread only.
        let hr = unsafe { CoInitializeEx(null(), flags) };
        if hr == S_OK || hr == S_FALSE {
            Ok(Self { initialized: true })
        } else if hr == RPC_E_CHANGED_MODE {
            Err(super::platform_error(
                "폴더 선택 대화상자는 STA COM 초기화가 필요하지만 현재 스레드가 다른 COM 모델로 이미 초기화되어 있습니다.",
            ))
        } else {
            Err(super::platform_error(format!(
                "폴더 선택 대화상자 COM 초기화에 실패했습니다 (hr=0x{:08X})",
                hr as u32
            )))
        }
    }
}

#[cfg(windows)]
impl Drop for FolderDialogComGuard {
    fn drop(&mut self) {
        if self.initialized {
            use windows_sys::Win32::System::Com::CoUninitialize;

            // Safety: this guard calls CoUninitialize only for a successful
            // CoInitializeEx on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[test]
    fn debug_folder_picker_override_supports_path_cancel_and_error() {
        assert_eq!(
            debug_folder_picker_override_from_env(Some("C:\\Tools".into()), None)
                .expect("override should be present")
                .expect("path override should succeed"),
            Some(PathBuf::from("C:\\Tools"))
        );
        assert_eq!(
            debug_folder_picker_override_from_env(Some("__CANCEL__".into()), None)
                .expect("override should be present")
                .expect("cancel override should succeed"),
            None
        );

        let error = debug_folder_picker_override_from_env(None, Some("no local path".into()))
            .expect("override should be present")
            .expect_err("error override should fail");
        assert_eq!(error.user_message(), "no local path");

        let error = debug_folder_picker_override_from_env(
            Some("C:\\Tools".into()),
            Some("no local path".into()),
        )
        .expect("override should be present")
        .expect_err("error override should take precedence");
        assert_eq!(error.user_message(), "no local path");

        assert!(debug_folder_picker_override_from_env(None, None).is_none());
    }
}
