use serde_json::{Map, Value};

use crate::domain::LauncherConfig;

pub type SettingsDocument = Map<String, Value>;
pub type LauncherSettings = LauncherConfig;
