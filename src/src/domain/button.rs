use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherButton {
    pub item_id: String,
    pub source_name: String,
    pub source_path: String,
    pub is_dir: bool,
    pub name: String,
    pub path: String,
    pub params: String,
    pub admin: bool,
    pub action: u8,
    pub auto_enter: bool,
}

impl LauncherButton {
    pub fn manual_default() -> Self {
        Self {
            item_id: String::new(),
            source_name: String::new(),
            source_path: String::new(),
            is_dir: false,
            name: String::new(),
            path: String::new(),
            params: String::new(),
            admin: false,
            action: 0,
            auto_enter: false,
        }
    }

    pub(crate) fn from_value(
        raw_button: &Value,
        fallback_seed: &str,
        allow_empty_item_id: bool,
    ) -> Self {
        let data = raw_button.as_object().cloned().unwrap_or_default();
        Self::from_map(&data, fallback_seed, allow_empty_item_id)
    }

    fn from_map(data: &Map<String, Value>, fallback_seed: &str, allow_empty_item_id: bool) -> Self {
        let source_path = resolve_button_source_path(data);
        let raw_path = truthy_string(data.get("path"));
        let path = if raw_path.is_empty() {
            source_path.clone()
        } else {
            raw_path
        };

        let item_id = if allow_empty_item_id {
            String::new()
        } else {
            let raw_item_id = truthy_string(data.get("item_id")).trim().to_owned();
            if raw_item_id.is_empty() {
                make_item_id(&source_path)
                    .or_else(|| make_item_id(&path))
                    .unwrap_or_else(|| format!("legacy-item-{fallback_seed}"))
            } else {
                raw_item_id
            }
        };

        let mut source_name = truthy_string(data.get("source_name")).trim().to_owned();
        if source_name.is_empty() {
            source_name = if source_path.is_empty() {
                truthy_string(data.get("name"))
            } else {
                path_basename(&source_path)
            };
        }

        Self {
            item_id,
            source_name,
            source_path,
            is_dir: parse_bool(data.get("is_dir")),
            name: optional_string(data.get("name")),
            path,
            params: truthy_string(data.get("params")),
            admin: parse_bool(data.get("admin")),
            action: parse_action(data.get("action")),
            auto_enter: parse_bool(data.get("auto_enter")),
        }
    }
}

impl Default for LauncherButton {
    fn default() -> Self {
        Self::manual_default()
    }
}

pub(crate) fn parse_bool(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => number_as_i64(value).is_some_and(|number| number != 0),
        Some(Value::String(value)) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        _ => false,
    }
}

pub(crate) fn parse_int(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Some(Value::Number(value)) => number_as_i64(value),
        Some(Value::String(value)) => value.trim().parse::<i64>().ok(),
        _ => None,
    }
}

pub(crate) fn optional_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => String::new(),
    }
}

pub(crate) fn truthy_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        Some(Value::Bool(true)) => "true".to_owned(),
        Some(Value::Number(value)) if number_as_i64(value).is_some_and(|number| number != 0) => {
            value.to_string()
        }
        _ => String::new(),
    }
}

pub(crate) fn make_item_id(path: &str) -> Option<String> {
    let normalized = normalize_path_text(path);
    if normalized.is_empty() {
        None
    } else if cfg!(windows) || is_windows_path_like(&normalized) {
        Some(normalized.to_lowercase())
    } else {
        Some(normalized)
    }
}

pub(crate) fn normalize_path_text(path: &str) -> String {
    let path = path.trim().trim_matches('"');
    if cfg!(windows) || is_windows_path_like(path) {
        path.replace('/', "\\")
    } else {
        path.to_owned()
    }
}

fn parse_action(value: Option<&Value>) -> u8 {
    if parse_int(value) == Some(1) { 1 } else { 0 }
}

fn resolve_button_source_path(data: &Map<String, Value>) -> String {
    let raw_source_path = truthy_string(data.get("source_path"));
    if !raw_source_path.trim().is_empty() {
        return normalize_path_text(&raw_source_path);
    }

    normalize_path_text(&truthy_string(data.get("path")))
}

fn path_basename(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .map(str::to_owned)
        .unwrap_or_default()
}

fn number_as_i64(value: &Number) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| {
            let value = value.as_f64()?;
            if !value.is_finite() {
                return None;
            }
            let truncated = value.trunc();
            if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
                None
            } else {
                Some(truncated as i64)
            }
        })
}

fn is_windows_path_like(path: &str) -> bool {
    let mut chars = path.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(drive), Some(':'), Some('\\' | '/')) if drive.is_ascii_alphabetic()
    ) || path.starts_with("\\\\")
}
