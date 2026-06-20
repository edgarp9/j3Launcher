use std::path::Path;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellExecuteOutcome {
    pub code: isize,
    pub last_error: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminLaunchStatus {
    Started,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminLaunchResult {
    pub status: AdminLaunchStatus,
    pub outcome: ShellExecuteOutcome,
}

pub fn open_path(path: impl AsRef<Path>) -> Result<ShellExecuteOutcome> {
    shell_execute_path("open", path.as_ref(), None, None)
}

pub fn open_file(path: impl AsRef<Path>) -> Result<ShellExecuteOutcome> {
    open_path(path)
}

pub fn open_folder(path: impl AsRef<Path>) -> Result<ShellExecuteOutcome> {
    open_path(path)
}

pub fn launch_program(
    executable: impl AsRef<Path>,
    parameters: Option<&str>,
    working_dir: Option<&Path>,
) -> Result<ShellExecuteOutcome> {
    shell_execute_path("open", executable.as_ref(), parameters, working_dir)
}

pub fn run_as_admin(
    executable: impl AsRef<Path>,
    parameters: Option<&str>,
) -> Result<AdminLaunchResult> {
    let executable = executable.as_ref();
    ensure_path_not_empty(executable)?;
    if executable.is_dir() {
        return Err(super::platform_error(
            "directories cannot be run as administrator",
        ));
    }

    let outcome = shell_execute_raw("runas", executable, parameters, None)?;
    let status = if is_admin_launch_cancelled(outcome) {
        AdminLaunchStatus::Cancelled
    } else {
        AdminLaunchStatus::Started
    };

    if status == AdminLaunchStatus::Cancelled || shell_execute_succeeded(outcome.code) {
        Ok(AdminLaunchResult { status, outcome })
    } else {
        Err(super::platform_error(format!(
            "ShellExecuteW failed for administrator launch ({})",
            shell_execute_outcome_detail(outcome)
        )))
    }
}

pub fn select_in_explorer(path: impl AsRef<Path>) -> Result<ShellExecuteOutcome> {
    ensure_path_not_empty(path.as_ref())?;
    select_in_explorer_path(path.as_ref())
}

fn shell_execute_path(
    operation: &str,
    file: &Path,
    parameters: Option<&str>,
    working_dir: Option<&Path>,
) -> Result<ShellExecuteOutcome> {
    ensure_path_not_empty(file)?;
    let outcome = shell_execute_raw(operation, file, parameters, working_dir)?;
    if shell_execute_succeeded(outcome.code) {
        Ok(outcome)
    } else {
        Err(super::platform_error(format!(
            "ShellExecuteW failed (operation='{operation}', {})",
            shell_execute_outcome_detail(outcome)
        )))
    }
}

#[cfg(windows)]
fn shell_execute_raw(
    operation: &str,
    file: &Path,
    parameters: Option<&str>,
    working_dir: Option<&Path>,
) -> Result<ShellExecuteOutcome> {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation_wide = super::wide::WideString::from_str("operation", operation)?;
    let file_wide = super::wide::path_to_wide_z(file)?;
    let parameters_wide = parameters
        .map(|value| super::wide::WideString::from_str("parameters", value))
        .transpose()?;
    let working_dir_wide = non_empty_optional_path(working_dir)
        .map(super::wide::path_to_wide_z)
        .transpose()?;

    // Safety: SetLastError only mutates this thread's last-error slot.
    unsafe { SetLastError(0) };
    // Safety: all optional strings are either null or validated NUL-terminated
    // UTF-16 buffers that live for this call. ShellExecuteW does not retain the
    // pointers after it returns.
    let instance = unsafe {
        ShellExecuteW(
            null_mut(),
            operation_wide.as_ptr(),
            file_wide.as_ptr(),
            parameters_wide
                .as_ref()
                .map_or(std::ptr::null(), super::wide::WideString::as_ptr),
            working_dir_wide
                .as_ref()
                .map_or(std::ptr::null(), super::wide::WideString::as_ptr),
            SW_SHOWNORMAL,
        )
    };
    // Safety: GetLastError reads this thread's last-error slot and requires no
    // additional invariants.
    let last_error = unsafe { GetLastError() };

    Ok(ShellExecuteOutcome {
        code: instance as isize,
        last_error,
    })
}

#[cfg(not(windows))]
fn shell_execute_raw(
    _operation: &str,
    _file: &Path,
    _parameters: Option<&str>,
    _working_dir: Option<&Path>,
) -> Result<ShellExecuteOutcome> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
fn select_in_explorer_path(path: &Path) -> Result<ShellExecuteOutcome> {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let explorer_wide = super::wide::WideString::from_str("explorer executable", "explorer.exe")?;
    let operation_wide = super::wide::WideString::from_str("operation", "open")?;
    let parameters_wide = explorer_select_parameters_wide(path)?;

    // Safety: SetLastError only mutates this thread's last-error slot.
    unsafe { SetLastError(0) };
    // Safety: strings are validated NUL-terminated UTF-16 buffers and live for
    // this call. ShellExecuteW does not retain the pointers.
    let instance = unsafe {
        ShellExecuteW(
            null_mut(),
            operation_wide.as_ptr(),
            explorer_wide.as_ptr(),
            parameters_wide.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Safety: GetLastError reads this thread's last-error slot and requires no
    // additional invariants.
    let last_error = unsafe { GetLastError() };
    let outcome = ShellExecuteOutcome {
        code: instance as isize,
        last_error,
    };
    if shell_execute_succeeded(outcome.code) {
        Ok(outcome)
    } else {
        Err(super::platform_error(format!(
            "explorer /select failed ({})",
            shell_execute_outcome_detail(outcome)
        )))
    }
}

#[cfg(windows)]
fn explorer_select_parameters_wide(path: &Path) -> Result<super::wide::WideString> {
    use std::os::windows::ffi::OsStrExt;

    let mut units: Vec<u16> = "/select,\"".encode_utf16().collect();
    units.extend(path.as_os_str().encode_wide());
    units.extend("\"".encode_utf16());
    super::wide::WideString::from_units("explorer parameters", units)
}

#[cfg(not(windows))]
fn select_in_explorer_path(_path: &Path) -> Result<ShellExecuteOutcome> {
    Err(super::unsupported_platform())
}

fn shell_execute_succeeded(code: isize) -> bool {
    code > 32
}

fn shell_execute_outcome_detail(outcome: ShellExecuteOutcome) -> String {
    format!(
        "code={} {}, last_error={} {}",
        outcome.code,
        shell_execute_code_name(outcome.code),
        outcome.last_error,
        windows_last_error_name(outcome.last_error)
    )
}

fn shell_execute_code_name(code: isize) -> &'static str {
    match code {
        0 => "SE_ERR_OOM_OR_ZERO",
        2 => "SE_ERR_FNF",
        3 => "SE_ERR_PNF",
        5 => "SE_ERR_ACCESSDENIED",
        8 => "SE_ERR_OOM",
        26 => "SE_ERR_SHARE",
        27 => "SE_ERR_ASSOCINCOMPLETE",
        28 => "SE_ERR_DDETIMEOUT",
        29 => "SE_ERR_DDEFAIL",
        30 => "SE_ERR_DDEBUSY",
        31 => "SE_ERR_NOASSOC",
        32 => "SE_ERR_DLLNOTFOUND",
        _ if shell_execute_succeeded(code) => "SUCCESS",
        _ => "SE_ERR_UNKNOWN",
    }
}

fn windows_last_error_name(error: u32) -> &'static str {
    match error {
        0 => "ERROR_SUCCESS",
        2 => "ERROR_FILE_NOT_FOUND",
        3 => "ERROR_PATH_NOT_FOUND",
        5 => "ERROR_ACCESS_DENIED",
        87 => "ERROR_INVALID_PARAMETER",
        126 => "ERROR_MOD_NOT_FOUND",
        127 => "ERROR_PROC_NOT_FOUND",
        1223 => "ERROR_CANCELLED",
        _ => "ERROR_UNKNOWN",
    }
}

fn ensure_path_not_empty(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        Err(super::platform_error("path is empty"))
    } else {
        Ok(())
    }
}

fn non_empty_optional_path(path: Option<&Path>) -> Option<&Path> {
    path.filter(|value| !value.as_os_str().is_empty())
}

fn is_admin_launch_cancelled(outcome: ShellExecuteOutcome) -> bool {
    const ERROR_CANCELLED: u32 = 1223;
    const SE_ERR_ACCESSDENIED: isize = 5;

    !shell_execute_succeeded(outcome.code)
        && (outcome.code == SE_ERR_ACCESSDENIED || outcome.last_error == ERROR_CANCELLED)
}

#[cfg(test)]
fn build_explorer_select_parameters_for_test(path: &str) -> Result<String> {
    let trimmed = super::wide::non_empty_trimmed("path", path)?;
    Ok(format!("/select,\"{trimmed}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_execute_success_requires_code_above_32() {
        assert!(!shell_execute_succeeded(32));
        assert!(shell_execute_succeeded(33));
    }

    #[test]
    fn runas_cancel_detection_uses_code_or_last_error() {
        assert!(is_admin_launch_cancelled(ShellExecuteOutcome {
            code: 5,
            last_error: 0,
        }));
        assert!(is_admin_launch_cancelled(ShellExecuteOutcome {
            code: 31,
            last_error: 1223,
        }));
        assert!(!is_admin_launch_cancelled(ShellExecuteOutcome {
            code: 33,
            last_error: 1223,
        }));
    }

    #[test]
    fn shell_execute_outcome_detail_maps_known_codes_and_last_errors() {
        let detail = shell_execute_outcome_detail(ShellExecuteOutcome {
            code: 31,
            last_error: 1223,
        });

        assert!(detail.contains("SE_ERR_NOASSOC"));
        assert!(detail.contains("ERROR_CANCELLED"));
    }

    #[test]
    fn shell_execute_outcome_detail_maps_module_lookup_errors() {
        let detail = shell_execute_outcome_detail(ShellExecuteOutcome {
            code: 32,
            last_error: 127,
        });

        assert!(detail.contains("SE_ERR_DLLNOTFOUND"));
        assert!(detail.contains("ERROR_PROC_NOT_FOUND"));
    }

    #[test]
    fn explorer_select_parameters_quote_target() {
        let params = build_explorer_select_parameters_for_test("C:\\Tools\\app.exe")
            .expect("valid parameters");

        assert_eq!(params, "/select,\"C:\\Tools\\app.exe\"");
    }

    #[test]
    fn explorer_select_parameters_reject_nul() {
        let error =
            build_explorer_select_parameters_for_test("C:\\bad\0name").expect_err("NUL fails");

        assert!(error.to_string().contains("interior NUL"));
    }

    #[test]
    fn empty_path_is_rejected_before_win32_call() {
        let error = ensure_path_not_empty(Path::new("")).expect_err("empty path fails");

        assert!(error.to_string().contains("path is empty"));
    }

    #[test]
    fn empty_optional_working_dir_is_treated_as_absent() {
        assert_eq!(non_empty_optional_path(None), None);
        assert_eq!(non_empty_optional_path(Some(Path::new(""))), None);
        assert_eq!(
            non_empty_optional_path(Some(Path::new("C:\\Tools"))),
            Some(Path::new("C:\\Tools"))
        );
    }
}
