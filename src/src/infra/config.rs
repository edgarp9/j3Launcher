use std::fs;
use std::path::Path;

use crate::domain::LauncherSettings;
use crate::{LauncherError, Result};

pub fn load_settings(path: impl AsRef<Path>) -> Result<LauncherSettings> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|source| LauncherError::ConfigRead {
        path: path.to_path_buf(),
        source,
    })?;

    serde_json::from_str(&contents).map_err(|source| LauncherError::ConfigParse {
        path: path.to_path_buf(),
        source,
    })
}
