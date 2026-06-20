use std::borrow::Borrow;
use std::collections::{BTreeMap, HashSet};
use std::hash::Hash;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::button::{
    LauncherButton, normalize_path_text, optional_string, parse_int, truthy_string,
};
use crate::domain::scan::ScanSignature;

pub const TAB_TYPE_FOLDER: &str = "folder";
pub const TAB_TYPE_MANUAL: &str = "manual";
pub const DEFAULT_BUTTON_ROWS: u16 = 3;
pub const DEFAULT_BUTTON_COLS: u16 = 8;
pub const MANUAL_DEFAULT_BUTTON_ROWS: u16 = 3;
pub const MANUAL_DEFAULT_BUTTON_COLS: u16 = 8;
pub const MAX_TAB_COUNT: usize = 50;
pub const MAX_BUTTON_ROWS: u16 = 500;
pub const MAX_BUTTON_COLS: u16 = 32;

pub(crate) fn max_button_slot_index(cols: u16) -> u64 {
    let cols = cols.clamp(1, MAX_BUTTON_COLS);
    u64::from(MAX_BUTTON_ROWS) * u64::from(cols) - 1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabType {
    #[default]
    Folder,
    Manual,
}

impl TabType {
    pub(crate) fn from_value(value: Option<&Value>) -> Self {
        if truthy_string(value)
            .trim()
            .eq_ignore_ascii_case(TAB_TYPE_MANUAL)
        {
            Self::Manual
        } else {
            Self::Folder
        }
    }

    pub(crate) fn is_manual(self) -> bool {
        self == Self::Manual
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherTab {
    pub id: String,
    pub tab_type: TabType,
    pub title: String,
    pub folder_path: String,
    pub rows: u16,
    pub cols: u16,
    pub hidden_item_ids: Vec<String>,
    pub slot_positions: BTreeMap<String, u64>,
    pub buttons: Vec<LauncherButton>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_signature: Option<ScanSignature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_item_order: Option<Vec<String>>,
}

impl LauncherTab {
    pub(crate) fn from_value(
        raw_tab: &Value,
        tab_index: usize,
        seen_tab_ids: &mut HashSet<String>,
    ) -> Option<Self> {
        let data = raw_tab.as_object()?;
        let mut id = truthy_string(data.get("id")).trim().to_owned();
        if id.is_empty() || seen_tab_ids.contains(&id) {
            id = build_tab_id(seen_tab_ids);
        }
        seen_tab_ids.insert(id.clone());

        let tab_type = TabType::from_value(data.get("tab_type"));
        let rows_default = if tab_type.is_manual() {
            MANUAL_DEFAULT_BUTTON_ROWS
        } else {
            DEFAULT_BUTTON_ROWS
        };
        let cols_default = if tab_type.is_manual() {
            MANUAL_DEFAULT_BUTTON_COLS
        } else {
            DEFAULT_BUTTON_COLS
        };
        let rows = parse_bounded_int(data.get("rows"), rows_default, 1, MAX_BUTTON_ROWS);
        let cols = parse_bounded_int(data.get("cols"), cols_default, 1, MAX_BUTTON_COLS);

        let buttons_raw = data
            .get("buttons")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let mut buttons = Vec::new();
        let mut seen_button_ids = HashSet::new();
        for (button_index, raw_button) in buttons_raw.iter().enumerate() {
            if !raw_button.is_object() {
                if tab_type.is_manual() {
                    buttons.push(LauncherButton::manual_default());
                }
                continue;
            }

            let fallback_seed = format!("{tab_index}-{button_index}");
            let button =
                LauncherButton::from_value(raw_button, &fallback_seed, tab_type.is_manual());
            if tab_type.is_manual() {
                buttons.push(button);
                continue;
            }

            if button.item_id.is_empty() || !seen_button_ids.insert(button.item_id.clone()) {
                continue;
            }
            buttons.push(button);
        }

        let folder_path;
        let hidden_item_ids;
        let slot_positions;
        let scan_signature;
        let scan_item_order;

        if tab_type.is_manual() {
            let required_slots = usize::from(rows) * usize::from(cols);
            buttons.truncate(required_slots);
            while buttons.len() < required_slots {
                buttons.push(LauncherButton::manual_default());
            }

            folder_path = String::new();
            hidden_item_ids = Vec::new();
            slot_positions = BTreeMap::new();
            scan_signature = None;
            scan_item_order = None;
        } else {
            folder_path = normalize_path_text(&truthy_string(data.get("folder_path")));
            hidden_item_ids = normalize_hidden_item_ids_value(data.get("hidden_item_ids"));
            let valid_button_ids = buttons
                .iter()
                .map(|button| button.item_id.as_str())
                .collect::<HashSet<_>>();
            slot_positions =
                normalize_slot_positions_value(data.get("slot_positions"), &valid_button_ids, cols);
            scan_signature = ScanSignature::from_value(data.get("scan_signature"), &folder_path);
            scan_item_order = scan_signature.as_ref().and_then(|_| {
                normalize_scan_item_order_value(data.get("scan_item_order"), &valid_button_ids)
            });
        }

        Some(Self {
            id,
            tab_type,
            title: title_or_default(data.get("title"), tab_index),
            folder_path,
            rows,
            cols,
            hidden_item_ids,
            slot_positions,
            buttons,
            scan_signature,
            scan_item_order,
        })
    }
}

fn parse_bounded_int(value: Option<&Value>, default: u16, min_value: u16, max_value: u16) -> u16 {
    let parsed = parse_int(value)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value >= 1)
        .unwrap_or(default);

    parsed.clamp(min_value, max_value)
}

fn normalize_hidden_item_ids_value(raw_hidden: Option<&Value>) -> Vec<String> {
    let Some(raw_hidden) = raw_hidden.and_then(Value::as_array) else {
        return Vec::new();
    };

    normalize_hidden_item_ids(
        raw_hidden
            .iter()
            .map(|raw_item_id| truthy_string(Some(raw_item_id))),
    )
}

pub(crate) fn normalize_hidden_item_ids<I, S>(hidden_item_ids: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for raw_item_id in hidden_item_ids {
        let item_id = raw_item_id.as_ref().trim().to_owned();
        if !item_id.is_empty() && seen.insert(item_id.clone()) {
            normalized.push(item_id);
        }
    }
    normalized
}

fn normalize_slot_positions_value(
    raw_slot_positions: Option<&Value>,
    valid_item_ids: &HashSet<&str>,
    cols: u16,
) -> BTreeMap<String, u64> {
    let Some(raw_slot_positions) = raw_slot_positions.and_then(Value::as_object) else {
        return BTreeMap::new();
    };

    let parsed_slot_positions = raw_slot_positions
        .iter()
        .filter_map(|(raw_item_id, raw_slot)| {
            let slot_index = parse_int(Some(raw_slot))?;
            if slot_index < 0 {
                return None;
            }
            Some((raw_item_id.as_str(), slot_index as u64))
        });

    normalize_slot_positions(parsed_slot_positions, valid_item_ids, cols)
}

pub(crate) fn normalize_slot_positions<I, S>(
    slot_positions: I,
    valid_item_ids: &HashSet<&str>,
    cols: u16,
) -> BTreeMap<String, u64>
where
    I: IntoIterator<Item = (S, u64)>,
    S: AsRef<str>,
{
    let mut normalized = BTreeMap::new();
    let max_slot_index = max_button_slot_index(cols);
    for (raw_item_id, slot_index) in slot_positions {
        let item_id = raw_item_id.as_ref().trim();
        if item_id.is_empty() || !valid_item_ids.contains(item_id) {
            continue;
        }
        if slot_index > max_slot_index {
            continue;
        }
        normalized.insert(item_id.to_owned(), slot_index);
    }
    normalized
}

fn normalize_scan_item_order_value(
    raw_order: Option<&Value>,
    valid_item_ids: &HashSet<&str>,
) -> Option<Vec<String>> {
    let raw_order = raw_order?.as_array()?;
    normalize_scan_item_order(
        Some(
            raw_order
                .iter()
                .map(|raw_item_id| truthy_string(Some(raw_item_id))),
        ),
        valid_item_ids,
    )
}

pub(crate) fn normalize_scan_item_order<I, S>(
    scan_item_order: Option<I>,
    valid_item_ids: &HashSet<&str>,
) -> Option<Vec<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let scan_item_order = scan_item_order?;
    if valid_item_ids.is_empty() {
        let mut iter = scan_item_order.into_iter();
        return if iter.next().is_none() {
            Some(Vec::new())
        } else {
            None
        };
    }

    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for raw_item_id in scan_item_order {
        let item_id = raw_item_id.as_ref().trim().to_owned();
        if item_id.is_empty()
            || !valid_item_ids.contains(item_id.as_str())
            || !seen.insert(item_id.clone())
        {
            return None;
        }
        normalized.push(item_id);
    }

    if seen.len() == valid_item_ids.len() {
        Some(normalized)
    } else {
        None
    }
}

fn title_or_default(raw_title: Option<&Value>, tab_index: usize) -> String {
    let title = optional_string(raw_title);
    if title.is_empty() {
        format!("Tab {}", tab_index + 1)
    } else {
        title
    }
}

pub(crate) fn build_tab_id<T>(existing_ids: &HashSet<T>) -> String
where
    T: Borrow<str> + Eq + Hash,
{
    for index in 1.. {
        let candidate = format!("tab-{index}");
        if !existing_ids.contains(candidate.as_str()) {
            return candidate;
        }
    }
    String::from("tab")
}

pub(crate) fn path_basename(path: &str) -> Option<String> {
    path.rsplit(['\\', '/'])
        .find(|part| !part.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn folder_tab_rejects_slot_positions_above_supported_grid() {
        let raw = json!({
            "id": "folder",
            "tab_type": "folder",
            "title": "Folder",
            "folder_path": "C:/tools",
            "rows": 3,
            "cols": 1,
            "slot_positions": {
                "item-a": 4294967295u64,
                "item-b": max_button_slot_index(1)
            },
            "buttons": [
                {"item_id": "item-a", "name": "A", "path": "C:/tools/a.exe"},
                {"item_id": "item-b", "name": "B", "path": "C:/tools/b.exe"}
            ]
        });
        let mut seen_tab_ids = HashSet::new();

        let Some(tab) = LauncherTab::from_value(&raw, 0, &mut seen_tab_ids) else {
            panic!("folder tab should parse");
        };

        assert!(!tab.slot_positions.contains_key("item-a"));
        assert_eq!(
            tab.slot_positions.get("item-b"),
            Some(&max_button_slot_index(1))
        );
    }

    #[test]
    fn manual_tab_truncates_buttons_to_layout_slots_on_load() {
        let raw = json!({
            "id": "manual",
            "tab_type": "manual",
            "title": "Manual",
            "rows": 1,
            "cols": 2,
            "buttons": [
                {"name": "One"},
                {"name": "Two"},
                {"name": "Hidden"}
            ]
        });
        let mut seen_tab_ids = HashSet::new();

        let Some(tab) = LauncherTab::from_value(&raw, 0, &mut seen_tab_ids) else {
            panic!("manual tab should parse");
        };

        assert_eq!(tab.buttons.len(), 2);
        assert_eq!(tab.buttons[0].name, "One");
        assert_eq!(tab.buttons[1].name, "Two");
    }

    #[test]
    fn shared_typed_tab_normalizers_preserve_existing_rules() {
        let hidden = normalize_hidden_item_ids([" item-a ", "", "item-a", "item-b"]);
        assert_eq!(hidden, vec![String::from("item-a"), String::from("item-b")]);

        let valid_item_ids = HashSet::from(["item-a", "item-b"]);
        let slots = normalize_slot_positions(
            [
                (" item-a ", 2),
                ("missing", 1),
                ("item-b", max_button_slot_index(1) + 1),
            ],
            &valid_item_ids,
            1,
        );
        assert_eq!(slots, BTreeMap::from([(String::from("item-a"), 2)]));

        assert_eq!(
            normalize_scan_item_order(Some([" item-a ", "item-b"]), &valid_item_ids),
            Some(vec![String::from("item-a"), String::from("item-b")])
        );
        assert_eq!(
            normalize_scan_item_order(Some(["item-a"]), &valid_item_ids),
            None
        );

        let owned_tab_ids = HashSet::from([String::from("tab-1")]);
        let borrowed_tab_ids = HashSet::from(["tab-1", "tab-2"]);
        assert_eq!(build_tab_id(&owned_tab_ids), "tab-2");
        assert_eq!(build_tab_id(&borrowed_tab_ids), "tab-3");
        assert_eq!(
            path_basename("C:/Tools/app.exe"),
            Some(String::from("app.exe"))
        );
    }
}
