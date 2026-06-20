use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};

use crate::domain::button::{
    make_item_id, normalize_path_text, parse_bool, parse_int, truthy_string,
};
use crate::domain::tab::{LauncherTab, MAX_BUTTON_COLS, MAX_BUTTON_ROWS, MAX_TAB_COUNT};

pub const DEFAULT_WINDOW_GEOMETRY: &str = "800x600";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(rename = "Geometry")]
    pub geometry: String,
    #[serde(rename = "DpiScale", skip_serializing_if = "Option::is_none")]
    pub dpi_scale: Option<f64>,
    #[serde(rename = "DarkTheme", default, skip_serializing_if = "is_false")]
    pub dark_theme: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl WindowConfig {
    fn from_value(raw_window: Option<Value>) -> Self {
        let Some(Value::Object(mut data)) = raw_window else {
            return Self::default();
        };

        let geometry = data
            .remove("Geometry")
            .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_WINDOW_GEOMETRY.to_owned());
        let dpi_scale = data.remove("DpiScale").and_then(normalize_dpi_scale);
        let dark_theme = data
            .remove("DarkTheme")
            .is_some_and(|value| parse_bool(Some(&value)));

        Self {
            geometry,
            dpi_scale,
            dark_theme,
            extra: data,
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            geometry: DEFAULT_WINDOW_GEOMETRY.to_owned(),
            dpi_scale: None,
            dark_theme: false,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LauncherConfig {
    #[serde(rename = "Window")]
    pub window: WindowConfig,
    #[serde(rename = "FolderTabs")]
    pub folder_tabs: Vec<LauncherTab>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl LauncherConfig {
    pub fn from_value(value: Value) -> Self {
        let Value::Object(data) = value else {
            return Self::default();
        };

        Self::from_object(data)
    }

    fn from_object(mut data: Map<String, Value>) -> Self {
        let window = WindowConfig::from_value(data.remove("Window"));
        let raw_folder_tabs = data.remove("FolderTabs");
        let (folder_tabs, extra) = match raw_folder_tabs {
            Some(Value::Array(raw_tabs)) => {
                (normalize_folder_tabs(Some(Value::Array(raw_tabs))), data)
            }
            _ => {
                if let Some(Value::Object(legacy_tabs)) = data.remove("Tabs") {
                    let migrated_tabs = migrate_legacy_folder_tabs(&legacy_tabs, &data);
                    (
                        normalize_folder_tabs(Some(Value::Array(migrated_tabs))),
                        Map::new(),
                    )
                } else {
                    (Vec::new(), data)
                }
            }
        };

        Self {
            window,
            folder_tabs,
            extra,
        }
    }

    pub fn from_json_str(payload: &str) -> serde_json::Result<Self> {
        serde_json::from_str(payload)
    }

    pub fn to_value(&self) -> serde_json::Result<Value> {
        serde_json::to_value(self)
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            folder_tabs: Vec::new(),
            extra: Map::new(),
        }
    }
}

impl<'de> Deserialize<'de> for LauncherConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let Value::Object(data) = value else {
            return Err(serde::de::Error::custom(
                "launcher config root must be a JSON object",
            ));
        };
        Ok(Self::from_object(data))
    }
}

fn normalize_folder_tabs(raw_folder_tabs: Option<Value>) -> Vec<LauncherTab> {
    let Some(Value::Array(raw_tabs)) = raw_folder_tabs else {
        return Vec::new();
    };

    let mut seen_tab_ids = HashSet::new();
    raw_tabs
        .iter()
        .take(MAX_TAB_COUNT)
        .enumerate()
        .filter_map(|(tab_index, raw_tab)| {
            LauncherTab::from_value(raw_tab, tab_index, &mut seen_tab_ids)
        })
        .collect()
}

fn migrate_legacy_folder_tabs(
    legacy_tabs: &Map<String, Value>,
    legacy_sections: &Map<String, Value>,
) -> Vec<Value> {
    let count = parse_legacy_bounded_usize(legacy_tabs.get("Count"), 0, 0, MAX_TAB_COUNT);
    let rows = parse_legacy_bounded_u16(
        legacy_tabs.get("ButtonRows"),
        crate::domain::tab::DEFAULT_BUTTON_ROWS,
        1,
        MAX_BUTTON_ROWS,
    );
    let cols = parse_legacy_bounded_u16(
        legacy_tabs.get("ButtonCols"),
        crate::domain::tab::DEFAULT_BUTTON_COLS,
        1,
        MAX_BUTTON_COLS,
    );

    (0..count)
        .map(|tab_index| {
            let section_key = format!("Tab{tab_index}");
            let legacy_section = legacy_sections.get(&section_key).and_then(Value::as_object);
            let title = legacy_truthy_string(legacy_tabs.get(&section_key))
                .unwrap_or_else(|| format!("Tab {}", tab_index + 1));

            json!({
                "id": format!("legacy-tab-{}", tab_index + 1),
                "tab_type": "folder",
                "title": title,
                "folder_path": "",
                "rows": rows,
                "cols": cols,
                "hidden_item_ids": [],
                "slot_positions": {},
                "buttons": migrate_legacy_buttons(tab_index, legacy_section),
            })
        })
        .collect()
}

fn migrate_legacy_buttons(
    tab_index: usize,
    legacy_section: Option<&Map<String, Value>>,
) -> Vec<Value> {
    let Some(legacy_section) = legacy_section else {
        return Vec::new();
    };

    let mut button_indexes = BTreeSet::new();
    for key in legacy_section.keys() {
        if let Some(button_index) = legacy_button_index(key) {
            button_indexes.insert(button_index);
        }
    }

    button_indexes
        .into_iter()
        .filter_map(|button_index| {
            let base = format!("Button{button_index}");
            let name = legacy_truthy_string(legacy_section.get(&format!("{base}_Name")))
                .unwrap_or_default();
            let path = legacy_truthy_string(legacy_section.get(&format!("{base}_Path")))
                .unwrap_or_default();
            let params = legacy_truthy_string(legacy_section.get(&format!("{base}_Params")))
                .unwrap_or_default();
            let admin = parse_bool(legacy_section.get(&format!("{base}_Admin")));
            let action = if parse_int(legacy_section.get(&format!("{base}_Action"))) == Some(1) {
                1
            } else {
                0
            };
            let auto_enter = parse_bool(legacy_section.get(&format!("{base}_AutoEnter")));
            if !admin
                && action != 1
                && !auto_enter
                && name.is_empty()
                && path.is_empty()
                && params.is_empty()
            {
                return None;
            }

            let source_path = normalize_path_text(&path);
            let item_id = make_item_id(&source_path)
                .unwrap_or_else(|| format!("legacy-tab{tab_index}-btn{button_index}"));
            let source_name = path_basename(&source_path).unwrap_or_else(|| {
                if name.is_empty() {
                    format!("Button {}", button_index + 1)
                } else {
                    name.clone()
                }
            });

            Some(json!({
                "item_id": item_id,
                "source_name": source_name,
                "source_path": source_path,
                "is_dir": false,
                "name": name,
                "path": path,
                "params": params,
                "admin": admin,
                "action": action,
                "auto_enter": auto_enter,
            }))
        })
        .collect()
}

fn legacy_button_index(key: &str) -> Option<usize> {
    let rest = key.strip_prefix("Button")?;
    let digit_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum::<usize>();
    if digit_len == 0 || !rest[digit_len..].starts_with('_') {
        return None;
    }
    rest[..digit_len].parse().ok()
}

fn parse_legacy_bounded_usize(
    value: Option<&Value>,
    default: usize,
    min_value: usize,
    max_value: usize,
) -> usize {
    let default = i64::try_from(default).unwrap_or(i64::MAX);
    let min_value = i64::try_from(min_value).unwrap_or(i64::MAX);
    let max_value = i64::try_from(max_value).unwrap_or(i64::MAX);
    parse_legacy_bounded_i64(value, default, min_value, max_value) as usize
}

fn parse_legacy_bounded_u16(
    value: Option<&Value>,
    default: u16,
    min_value: u16,
    max_value: u16,
) -> u16 {
    parse_legacy_bounded_i64(
        value,
        i64::from(default),
        i64::from(min_value),
        i64::from(max_value),
    ) as u16
}

fn parse_legacy_bounded_i64(
    value: Option<&Value>,
    default: i64,
    min_value: i64,
    max_value: i64,
) -> i64 {
    let parsed = parse_int(value)
        .filter(|value| *value >= 1)
        .unwrap_or(default);
    parsed.clamp(min_value, max_value)
}

fn legacy_truthy_string(value: Option<&Value>) -> Option<String> {
    let value = truthy_string(value);
    (!value.is_empty()).then_some(value)
}

fn path_basename(path: &str) -> Option<String> {
    path.rsplit(['\\', '/'])
        .find(|part| !part.is_empty())
        .map(str::to_owned)
}

fn normalize_dpi_scale(value: Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|value| *value > 0.0),
        Value::String(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| *value > 0.0),
        other => parse_int(Some(&other))
            .map(|value| value as f64)
            .filter(|value| *value > 0.0),
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use serde_json::json;

    use super::*;
    use crate::domain::tab::TabType;

    const WINDOWS_FIXTURE: &str = include_str!("../../tests/fixtures/j3Launcher_win.json");

    #[test]
    fn empty_config_uses_defaults() -> Result<(), Box<dyn Error>> {
        let config = LauncherConfig::from_value(json!({}));

        assert_eq!(config.window.geometry, DEFAULT_WINDOW_GEOMETRY);
        assert!(config.window.dpi_scale.is_none());
        assert!(!config.window.dark_theme);
        assert!(config.folder_tabs.is_empty());

        let value = config.to_value()?;
        assert_eq!(value["Window"]["Geometry"], DEFAULT_WINDOW_GEOMETRY);
        assert!(value["Window"].get("DarkTheme").is_none());
        assert_eq!(value["FolderTabs"], json!([]));
        Ok(())
    }

    #[test]
    fn invalid_window_and_folder_tabs_are_recovered() {
        let config = LauncherConfig::from_value(json!({
            "Window": "broken",
            "FolderTabs": {"not": "a-list"},
        }));

        assert_eq!(config.window, WindowConfig::default());
        assert!(config.folder_tabs.is_empty());
    }

    #[test]
    fn non_object_config_roots_are_rejected_by_deserialize() {
        for payload in ["null", "[]", "\"plain\"", "true", "42"] {
            let result = LauncherConfig::from_json_str(payload);

            assert!(
                result.is_err(),
                "non-object payload should be rejected: {payload}"
            );
        }
    }

    #[test]
    fn manual_tab_button_count_is_extended_to_slot_count() {
        let config = LauncherConfig::from_value(json!({
            "FolderTabs": [{
                "id": "manual",
                "tab_type": "manual",
                "title": "Manual",
                "folder_path": "C:/ignored",
                "rows": "2",
                "cols": "3",
                "hidden_item_ids": ["hidden"],
                "slot_positions": {"hidden": 1},
                "scan_signature": {"version": 1},
                "scan_item_order": [],
                "buttons": [{
                    "item_id": "ignored",
                    "name": "Tool",
                    "path": "tool.exe",
                    "admin": "yes",
                    "action": "1",
                    "auto_enter": 1
                }]
            }]
        }));

        let tab = &config.folder_tabs[0];
        assert_eq!(tab.tab_type, TabType::Manual);
        assert_eq!(tab.folder_path, "");
        assert_eq!(tab.buttons.len(), 6);
        assert_eq!(tab.buttons[0].item_id, "");
        assert!(tab.buttons[0].admin);
        assert_eq!(tab.buttons[0].action, 1);
        assert!(tab.buttons[0].auto_enter);
        assert!(tab.hidden_item_ids.is_empty());
        assert!(tab.slot_positions.is_empty());
        assert!(tab.scan_signature.is_none());
        assert!(tab.scan_item_order.is_none());
    }

    #[test]
    fn folder_tab_removes_duplicate_button_ids() {
        let config = LauncherConfig::from_value(json!({
            "FolderTabs": [{
                "id": "folder",
                "tab_type": "folder",
                "title": "Folder",
                "folder_path": "C:/tools",
                "rows": 1,
                "cols": 2,
                "hidden_item_ids": ["item-a", "item-a", "item-b"],
                "slot_positions": {
                    "item-a": 2,
                    "item-b": "3",
                    "missing": 4
                },
                "buttons": [
                    {"item_id": "item-a", "name": "A", "path": "C:/tools/a.exe"},
                    {"item_id": "item-a", "name": "A duplicated", "path": "C:/tools/a2.exe"},
                    {"item_id": "item-b", "name": "B", "path": "C:/tools/b.exe"}
                ]
            }]
        }));

        let tab = &config.folder_tabs[0];
        assert_eq!(tab.buttons.len(), 2);
        assert_eq!(tab.buttons[0].name, "A");
        assert_eq!(tab.buttons[1].item_id, "item-b");
        assert_eq!(tab.hidden_item_ids, vec!["item-a", "item-b"]);
        assert_eq!(tab.slot_positions.get("item-a"), Some(&2));
        assert_eq!(tab.slot_positions.get("item-b"), Some(&3));
        assert!(!tab.slot_positions.contains_key("missing"));
    }

    #[test]
    fn existing_windows_fixture_can_be_normalized() -> Result<(), Box<dyn Error>> {
        let config = LauncherConfig::from_json_str(WINDOWS_FIXTURE)?;

        assert!(!config.folder_tabs.is_empty());
        assert_eq!(config.window.geometry, "631x324+943+1873");
        assert!(
            config
                .folder_tabs
                .iter()
                .any(|tab| tab.tab_type == TabType::Manual)
        );
        assert!(
            config
                .folder_tabs
                .iter()
                .filter(|tab| tab.tab_type == TabType::Folder)
                .all(|tab| {
                    let mut seen = HashSet::new();
                    tab.buttons
                        .iter()
                        .all(|button| !button.item_id.is_empty() && seen.insert(&button.item_id))
                })
        );

        let _ = config.to_value()?;
        Ok(())
    }

    #[test]
    fn legacy_tabs_config_is_migrated_to_folder_tabs() {
        let config = LauncherConfig::from_value(json!({
            "Window": {"Geometry": "640x480"},
            "Tabs": {
                "Count": 1,
                "ButtonRows": 2,
                "ButtonCols": 3,
                "Tab0": "Legacy"
            },
            "Tab0": {
                "Button0_Name": "Tool",
                "Button0_Path": "C:/Tools/app.exe",
                "Button0_Params": "--fast",
                "Button0_Admin": "yes",
                "Button0_Action": "1",
                "Button0_AutoEnter": true
            }
        }));

        assert_eq!(config.folder_tabs.len(), 1);
        let tab = &config.folder_tabs[0];
        assert_eq!(tab.id, "legacy-tab-1");
        assert_eq!(tab.tab_type, TabType::Folder);
        assert_eq!(tab.title, "Legacy");
        assert_eq!(tab.rows, 2);
        assert_eq!(tab.cols, 3);
        assert_eq!(tab.buttons.len(), 1);
        let button = &tab.buttons[0];
        assert_eq!(button.item_id, "c:\\tools\\app.exe");
        assert_eq!(button.source_name, "app.exe");
        assert_eq!(button.source_path, "C:\\Tools\\app.exe");
        assert_eq!(button.name, "Tool");
        assert_eq!(button.params, "--fast");
        assert!(button.admin);
        assert_eq!(button.action, 1);
        assert!(button.auto_enter);
        assert!(config.extra.is_empty());
    }

    #[test]
    fn dark_theme_accepts_legacy_truthy_values() -> Result<(), Box<dyn Error>> {
        let config = LauncherConfig::from_value(json!({
            "Window": {
                "Geometry": "640x480",
                "DarkTheme": "yes"
            }
        }));

        assert!(config.window.dark_theme);

        let value = config.to_value()?;
        assert_eq!(value["Window"]["DarkTheme"], true);
        Ok(())
    }
}
