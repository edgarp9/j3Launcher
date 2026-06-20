use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WideString {
    inner: Vec<u16>,
}

impl WideString {
    pub(crate) fn from_str(label: &str, value: &str) -> Result<Self> {
        Self::from_units(label, value.encode_utf16().collect())
    }

    pub(crate) fn from_units(label: &str, mut units: Vec<u16>) -> Result<Self> {
        validate_no_nul_units(label, &units)?;
        units.push(0);
        Ok(Self { inner: units })
    }

    pub(crate) fn as_ptr(&self) -> *const u16 {
        self.inner.as_ptr()
    }

    #[cfg(test)]
    pub(crate) fn as_slice(&self) -> &[u16] {
        &self.inner
    }
}

#[cfg(test)]
pub(crate) fn validate_no_nul_str(label: &str, value: &str) -> Result<()> {
    if value.encode_utf16().any(|unit| unit == 0) {
        return Err(super::platform_error(format!(
            "{label} contains an interior NUL"
        )));
    }
    Ok(())
}

pub(crate) fn validate_no_nul_units(label: &str, units: &[u16]) -> Result<()> {
    if units.contains(&0) {
        return Err(super::platform_error(format!(
            "{label} contains an interior NUL"
        )));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn non_empty_trimmed<'a>(label: &str, value: &'a str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(super::platform_error(format!("{label} is empty")));
    }
    validate_no_nul_str(label, trimmed)?;
    Ok(trimmed)
}

#[cfg(windows)]
pub(crate) fn path_to_wide_z(path: &std::path::Path) -> Result<WideString> {
    use std::os::windows::ffi::OsStrExt;

    let units = path.as_os_str().encode_wide().collect();
    WideString::from_units("path", units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_string_appends_terminating_nul() {
        let wide = WideString::from_str("value", "abc").expect("valid UTF-16");

        assert_eq!(wide.as_slice(), &[97, 98, 99, 0]);
    }

    #[test]
    fn wide_string_rejects_interior_nul() {
        let error = WideString::from_str("value", "a\0b").expect_err("NUL must fail");

        assert!(error.to_string().contains("interior NUL"));
    }

    #[test]
    fn non_empty_trimmed_rejects_blank_input() {
        let error = non_empty_trimmed("path", "  ").expect_err("blank must fail");

        assert!(error.to_string().contains("path is empty"));
    }
}
