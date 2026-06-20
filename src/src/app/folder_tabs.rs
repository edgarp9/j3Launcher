use std::collections::{BTreeMap, HashMap, HashSet};

use crate::domain::button::{make_item_id, normalize_path_text};
use crate::domain::scan::scan_item_order_from_items;
use crate::domain::tab::{build_tab_id, normalize_hidden_item_ids, path_basename};
use crate::domain::{
    DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, FolderScanResult, LauncherButton, LauncherTab,
    MANUAL_DEFAULT_BUTTON_COLS, MANUAL_DEFAULT_BUTTON_ROWS, MAX_BUTTON_COLS, MAX_BUTTON_ROWS,
    MAX_TAB_COUNT, ScanItem, ScanSignature, TabType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTabMutationOutcome {
    pub focus_tab_idx: Option<usize>,
    pub reload_tab_indices: Vec<usize>,
    pub copy_tab_indices: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderTabMutationError {
    MaxTabsReached,
    TabNotFound,
    InvalidTabIndex,
    DuplicateFolder { tab_idx: usize },
    ManualTab,
}

pub type FolderTabMutationResult<T = FolderTabMutationOutcome> =
    std::result::Result<T, FolderTabMutationError>;

pub fn can_add_tab(folder_tabs: &[LauncherTab]) -> bool {
    folder_tabs.len() < MAX_TAB_COUNT
}

pub fn find_tab_index_by_id(folder_tabs: &[LauncherTab], tab_id: &str) -> Option<usize> {
    folder_tabs.iter().position(|tab| tab.id == tab_id)
}

pub fn find_tab_index_by_folder(folder_tabs: &[LauncherTab], folder_path: &str) -> Option<usize> {
    let target = make_item_id(folder_path)?;
    folder_tabs
        .iter()
        .position(|tab| make_item_id(&tab.folder_path).as_deref() == Some(target.as_str()))
}

pub fn add_folder_tab(
    folder_tabs: &mut Vec<LauncherTab>,
    folder_path: &str,
    scanned_items: &[ScanItem],
    scan_signature: Option<ScanSignature>,
) -> FolderTabMutationResult {
    if !can_add_tab(folder_tabs) {
        return Err(FolderTabMutationError::MaxTabsReached);
    }
    if let Some(tab_idx) = find_tab_index_by_folder(folder_tabs, folder_path) {
        return Err(FolderTabMutationError::DuplicateFolder { tab_idx });
    }

    let tab_idx = folder_tabs.len();
    let normalized_folder = normalize_path_text(folder_path);
    let existing_ids = folder_tabs
        .iter()
        .map(|tab| tab.id.as_str())
        .collect::<HashSet<_>>();
    let mut tab = LauncherTab {
        id: build_tab_id(&existing_ids),
        tab_type: TabType::Folder,
        title: path_basename(&normalized_folder).unwrap_or_else(|| normalized_folder.clone()),
        folder_path: normalized_folder,
        rows: DEFAULT_BUTTON_ROWS,
        cols: DEFAULT_BUTTON_COLS,
        hidden_item_ids: Vec::new(),
        slot_positions: BTreeMap::new(),
        buttons: scanned_items
            .iter()
            .map(default_button_from_scan_item)
            .collect(),
        scan_signature: None,
        scan_item_order: None,
    };
    set_tab_scan_signature(&mut tab, scan_signature, Some(scanned_items));
    folder_tabs.push(tab);

    Ok(FolderTabMutationOutcome {
        focus_tab_idx: Some(tab_idx),
        reload_tab_indices: Vec::new(),
        copy_tab_indices: Some(vec![tab_idx]),
    })
}

pub fn add_manual_tab(folder_tabs: &mut Vec<LauncherTab>) -> FolderTabMutationResult {
    if !can_add_tab(folder_tabs) {
        return Err(FolderTabMutationError::MaxTabsReached);
    }

    let tab_idx = folder_tabs.len();
    let existing_ids = folder_tabs
        .iter()
        .map(|tab| tab.id.as_str())
        .collect::<HashSet<_>>();
    let button_count =
        usize::from(MANUAL_DEFAULT_BUTTON_ROWS) * usize::from(MANUAL_DEFAULT_BUTTON_COLS);
    folder_tabs.push(LauncherTab {
        id: build_tab_id(&existing_ids),
        tab_type: TabType::Manual,
        title: format!("Tab {}", tab_idx + 1),
        folder_path: String::new(),
        rows: MANUAL_DEFAULT_BUTTON_ROWS,
        cols: MANUAL_DEFAULT_BUTTON_COLS,
        hidden_item_ids: Vec::new(),
        slot_positions: BTreeMap::new(),
        buttons: (0..button_count)
            .map(|_| LauncherButton::manual_default())
            .collect(),
        scan_signature: None,
        scan_item_order: None,
    });

    Ok(FolderTabMutationOutcome {
        focus_tab_idx: Some(tab_idx),
        reload_tab_indices: Vec::new(),
        copy_tab_indices: Some(vec![tab_idx]),
    })
}

pub fn set_tab_folder(
    folder_tabs: &mut [LauncherTab],
    tab_id: &str,
    folder_path: &str,
    scanned_items: &[ScanItem],
    scan_signature: Option<ScanSignature>,
) -> FolderTabMutationResult {
    let current_idx =
        find_tab_index_by_id(folder_tabs, tab_id).ok_or(FolderTabMutationError::TabNotFound)?;
    if let Some(duplicate_idx) = find_tab_index_by_folder(folder_tabs, folder_path)
        && duplicate_idx != current_idx
    {
        return Err(FolderTabMutationError::DuplicateFolder {
            tab_idx: duplicate_idx,
        });
    }

    let tab = folder_tabs
        .get_mut(current_idx)
        .ok_or(FolderTabMutationError::TabNotFound)?;
    tab.tab_type = TabType::Folder;
    tab.folder_path = normalize_path_text(folder_path);
    rebuild_buttons_from_scan(tab, scanned_items, scan_signature);

    Ok(single_tab_outcome(current_idx))
}

pub fn rename_tab(
    folder_tabs: &mut [LauncherTab],
    tab_idx: usize,
    title: &str,
) -> FolderTabMutationResult {
    let tab = folder_tabs
        .get_mut(tab_idx)
        .ok_or(FolderTabMutationError::InvalidTabIndex)?;
    let fallback_title = if tab.title.trim().is_empty() {
        format!("Tab {}", tab_idx + 1)
    } else {
        tab.title.clone()
    };
    let trimmed = title.trim();
    tab.title = if trimmed.is_empty() {
        fallback_title
    } else {
        trimmed.to_owned()
    };
    Ok(single_tab_outcome(tab_idx))
}

pub fn delete_tab(folder_tabs: &mut Vec<LauncherTab>, tab_idx: usize) -> FolderTabMutationResult {
    if tab_idx >= folder_tabs.len() {
        return Err(FolderTabMutationError::InvalidTabIndex);
    }
    folder_tabs.remove(tab_idx);
    let focus_tab_idx = if folder_tabs.is_empty() {
        None
    } else {
        Some(tab_idx.min(folder_tabs.len() - 1))
    };
    Ok(FolderTabMutationOutcome {
        focus_tab_idx,
        reload_tab_indices: Vec::new(),
        copy_tab_indices: Some(Vec::new()),
    })
}

pub fn update_tab_layout(
    folder_tabs: &mut [LauncherTab],
    tab_idx: usize,
    rows: u16,
    cols: u16,
) -> FolderTabMutationResult {
    let tab = folder_tabs
        .get_mut(tab_idx)
        .ok_or(FolderTabMutationError::InvalidTabIndex)?;
    tab.rows = rows.clamp(1, MAX_BUTTON_ROWS);
    tab.cols = cols.clamp(1, MAX_BUTTON_COLS);
    if tab.tab_type.is_manual() {
        let required_slots = usize::from(tab.rows).saturating_mul(usize::from(tab.cols));
        tab.buttons.truncate(required_slots);
        while tab.buttons.len() < required_slots {
            tab.buttons.push(LauncherButton::manual_default());
        }
    }
    Ok(single_tab_outcome(tab_idx))
}

pub fn refresh_tab_from_scan_result(
    folder_tabs: &mut [LauncherTab],
    tab_id: &str,
    scan_result: &FolderScanResult,
) -> FolderTabMutationResult<Option<FolderTabMutationOutcome>> {
    if scan_result.cancelled || scan_result.unchanged {
        return Ok(None);
    }

    let scan_signature = if scan_result.is_complete() {
        scan_result.signature.clone()
    } else {
        None
    };
    refresh_tab(
        folder_tabs,
        tab_id,
        &scan_result.items,
        !scan_result.is_complete(),
        scan_signature,
    )
    .map(Some)
}

pub fn refresh_tab(
    folder_tabs: &mut [LauncherTab],
    tab_id: &str,
    scanned_items: &[ScanItem],
    preserve_missing: bool,
    scan_signature: Option<ScanSignature>,
) -> FolderTabMutationResult {
    let current_idx =
        find_tab_index_by_id(folder_tabs, tab_id).ok_or(FolderTabMutationError::TabNotFound)?;
    let tab = folder_tabs
        .get_mut(current_idx)
        .ok_or(FolderTabMutationError::TabNotFound)?;
    merge_buttons_on_refresh(tab, scanned_items, preserve_missing, scan_signature);
    Ok(single_tab_outcome(current_idx))
}

pub fn reset_tab(
    folder_tabs: &mut [LauncherTab],
    tab_id: &str,
    scanned_items: &[ScanItem],
    scan_signature: Option<ScanSignature>,
) -> FolderTabMutationResult {
    let current_idx =
        find_tab_index_by_id(folder_tabs, tab_id).ok_or(FolderTabMutationError::TabNotFound)?;
    let tab = folder_tabs
        .get_mut(current_idx)
        .ok_or(FolderTabMutationError::TabNotFound)?;
    rebuild_buttons_from_scan(tab, scanned_items, scan_signature);
    Ok(single_tab_outcome(current_idx))
}

pub fn sort_tab(
    folder_tabs: &mut [LauncherTab],
    tab_id: &str,
) -> FolderTabMutationResult<Option<FolderTabMutationOutcome>> {
    let current_idx =
        find_tab_index_by_id(folder_tabs, tab_id).ok_or(FolderTabMutationError::TabNotFound)?;
    let tab = folder_tabs
        .get_mut(current_idx)
        .ok_or(FolderTabMutationError::TabNotFound)?;
    if tab.tab_type.is_manual() {
        return Err(FolderTabMutationError::ManualTab);
    }

    let mut sorted_buttons = tab.buttons.clone();
    sorted_buttons.sort_by_cached_key(button_sort_key);
    let current_slot_positions = sanitize_slot_positions(&tab.slot_positions, None);
    if sorted_buttons == tab.buttons && current_slot_positions.is_empty() {
        return Ok(None);
    }

    tab.buttons = sorted_buttons;
    tab.slot_positions.clear();
    Ok(Some(single_tab_outcome(current_idx)))
}

pub fn hide_item(
    folder_tabs: &mut [LauncherTab],
    tab_idx: usize,
    item_id: &str,
) -> FolderTabMutationResult<bool> {
    if item_id.trim().is_empty() {
        return Ok(false);
    }
    let tab = folder_tabs
        .get_mut(tab_idx)
        .ok_or(FolderTabMutationError::InvalidTabIndex)?;
    if tab.tab_type.is_manual() {
        return Err(FolderTabMutationError::ManualTab);
    }
    if tab.hidden_item_ids.iter().any(|hidden| hidden == item_id) {
        return Ok(false);
    }
    tab.hidden_item_ids.push(item_id.to_owned());
    Ok(true)
}

pub fn unhide_items(
    folder_tabs: &mut [LauncherTab],
    tab_idx: usize,
    item_ids: &[String],
) -> FolderTabMutationResult<bool> {
    let tab = folder_tabs
        .get_mut(tab_idx)
        .ok_or(FolderTabMutationError::InvalidTabIndex)?;
    if tab.tab_type.is_manual() {
        return Err(FolderTabMutationError::ManualTab);
    }
    let unhide = item_ids
        .iter()
        .map(|item_id| item_id.as_str())
        .collect::<HashSet<_>>();
    if unhide.is_empty() {
        return Ok(false);
    }
    let original_len = tab.hidden_item_ids.len();
    tab.hidden_item_ids
        .retain(|item_id| !unhide.contains(item_id.as_str()));
    Ok(tab.hidden_item_ids.len() != original_len)
}

pub fn build_known_scan_items_from_tab(tab: &LauncherTab) -> Option<Vec<ScanItem>> {
    tab.scan_signature.as_ref()?;
    let raw_order = tab.scan_item_order.as_ref()?;
    let mut buttons_by_id = HashMap::with_capacity(tab.buttons.len());
    for button in &tab.buttons {
        let item_id = button.item_id.trim();
        if item_id.is_empty() || buttons_by_id.contains_key(item_id) {
            return None;
        }
        buttons_by_id.insert(item_id, button);
    }

    let mut known_items = Vec::with_capacity(raw_order.len());
    let mut seen = HashSet::with_capacity(raw_order.len());
    for raw_item_id in raw_order {
        let item_id = raw_item_id.trim();
        if item_id.is_empty() || !seen.insert(item_id) {
            return None;
        }
        let button = buttons_by_id.get(item_id)?;
        let source_name = button.source_name.trim();
        let source_path = button.source_path.trim();
        if source_name.is_empty() || source_path.is_empty() {
            return None;
        }
        known_items.push(ScanItem::new(
            item_id,
            source_name,
            source_path,
            button.is_dir,
        ));
    }

    if seen.len() == buttons_by_id.len() {
        Some(known_items)
    } else {
        None
    }
}

fn default_button_from_scan_item(item: &ScanItem) -> LauncherButton {
    LauncherButton {
        item_id: item.item_id.clone(),
        source_name: item.name.clone(),
        source_path: item.path.clone(),
        is_dir: item.is_dir,
        name: item.name.clone(),
        path: item.path.clone(),
        params: String::new(),
        admin: false,
        action: 0,
        auto_enter: false,
    }
}

fn merge_buttons_on_refresh(
    tab: &mut LauncherTab,
    scanned_items: &[ScanItem],
    preserve_missing: bool,
    scan_signature: Option<ScanSignature>,
) {
    let scanned_by_id = scanned_items
        .iter()
        .map(|item| (item.item_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut merged = Vec::new();
    let mut seen = HashSet::new();

    for (index, button) in tab.buttons.iter().enumerate() {
        let mut current = normalize_folder_button(button, index);
        let Some(item) = scanned_by_id.get(current.item_id.as_str()) else {
            if preserve_missing {
                if !current.item_id.is_empty() {
                    seen.insert(current.item_id.clone());
                }
                merged.push(current);
            }
            continue;
        };

        let old_source_path = current.source_path.clone();
        current.source_name = item.name.clone();
        current.source_path = item.path.clone();
        current.is_dir = item.is_dir;
        if current.name.is_empty() {
            current.name = item.name.clone();
        }
        let current_path_item_id = if !current.path.is_empty() && current.path == old_source_path {
            Some(current.item_id.clone())
        } else {
            make_item_id(&current.path)
        };
        if !old_source_path.is_empty()
            && current_path_item_id.as_deref() == Some(current.item_id.as_str())
        {
            current.path = item.path.clone();
        }

        seen.insert(current.item_id.clone());
        merged.push(current);
    }

    for item in scanned_items {
        if !seen.contains(&item.item_id) {
            seen.insert(item.item_id.clone());
            merged.push(default_button_from_scan_item(item));
        }
    }

    let valid_hidden_ids = if preserve_missing {
        tab.hidden_item_ids
            .iter()
            .filter_map(|item_id| {
                let item_id = item_id.trim();
                (!item_id.is_empty()).then_some(item_id.to_owned())
            })
            .chain(merged.iter().map(|button| button.item_id.clone()))
            .collect::<HashSet<_>>()
    } else {
        scanned_items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<HashSet<_>>()
    };
    tab.hidden_item_ids = sanitize_hidden_item_ids(&tab.hidden_item_ids, &valid_hidden_ids);
    tab.buttons = merged;
    let valid_button_ids = tab
        .buttons
        .iter()
        .map(|button| button.item_id.clone())
        .collect::<HashSet<_>>();
    tab.slot_positions = sanitize_slot_positions(&tab.slot_positions, Some(&valid_button_ids));
    if preserve_missing {
        set_tab_scan_signature(tab, None, None);
    } else {
        set_tab_scan_signature(tab, scan_signature, Some(scanned_items));
    }
}

fn rebuild_buttons_from_scan(
    tab: &mut LauncherTab,
    scanned_items: &[ScanItem],
    scan_signature: Option<ScanSignature>,
) {
    let scanned_ids = scanned_items
        .iter()
        .map(|item| item.item_id.clone())
        .collect::<HashSet<_>>();
    tab.hidden_item_ids = sanitize_hidden_item_ids(&tab.hidden_item_ids, &scanned_ids);
    tab.buttons = scanned_items
        .iter()
        .map(default_button_from_scan_item)
        .collect();
    tab.slot_positions.clear();
    set_tab_scan_signature(tab, scan_signature, Some(scanned_items));
}

fn normalize_folder_button(button: &LauncherButton, fallback_index: usize) -> LauncherButton {
    let mut normalized = button.clone();
    normalized.source_path = normalize_path_text(&normalized.source_path);
    if normalized.path.is_empty() {
        normalized.path = normalized.source_path.clone();
    }
    if normalized.item_id.trim().is_empty() {
        normalized.item_id = make_item_id(&normalized.source_path)
            .or_else(|| make_item_id(&normalized.path))
            .unwrap_or_else(|| format!("legacy-item-{fallback_index}"));
    }
    if normalized.source_name.trim().is_empty() {
        normalized.source_name =
            path_basename(&normalized.source_path).unwrap_or_else(|| normalized.name.clone());
    }
    normalized.action = if normalized.action == 1 { 1 } else { 0 };
    normalized
}

fn set_tab_scan_signature(
    tab: &mut LauncherTab,
    scan_signature: Option<ScanSignature>,
    scanned_items: Option<&[ScanItem]>,
) {
    let Some(signature) = scan_signature else {
        tab.scan_signature = None;
        tab.scan_item_order = None;
        return;
    };

    tab.scan_signature = Some(signature);
    if let Some(scanned_items) = scanned_items {
        tab.scan_item_order = scan_item_order_from_items(scanned_items);
    }
}

fn sanitize_hidden_item_ids(raw_hidden_ids: &[String], valid_ids: &HashSet<String>) -> Vec<String> {
    normalize_hidden_item_ids(raw_hidden_ids.iter().map(|item_id| item_id.as_str()))
        .into_iter()
        .filter(|item_id| valid_ids.contains(item_id.as_str()))
        .collect()
}

fn sanitize_slot_positions(
    raw_slot_positions: &BTreeMap<String, u64>,
    valid_item_ids: Option<&HashSet<String>>,
) -> BTreeMap<String, u64> {
    let mut normalized = BTreeMap::new();
    for (item_id, slot_index) in raw_slot_positions {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            continue;
        }
        if valid_item_ids.is_some_and(|valid_item_ids| !valid_item_ids.contains(item_id)) {
            continue;
        }
        normalized.insert(item_id.to_owned(), *slot_index);
    }
    normalized
}

fn button_sort_key(button: &LauncherButton) -> (u8, String, String, String) {
    let source_path = normalize_path_text(&button.source_path);
    let sort_name = if !button.source_name.trim().is_empty() {
        button.source_name.trim().to_owned()
    } else if !button.name.trim().is_empty() {
        button.name.trim().to_owned()
    } else {
        path_basename(&source_path).unwrap_or_else(|| button.item_id.clone())
    };
    (
        if button.is_dir { 0 } else { 1 },
        sort_name.to_lowercase(),
        source_path.to_lowercase(),
        button.item_id.to_lowercase(),
    )
}

fn single_tab_outcome(tab_idx: usize) -> FolderTabMutationOutcome {
    FolderTabMutationOutcome {
        focus_tab_idx: Some(tab_idx),
        reload_tab_indices: vec![tab_idx],
        copy_tab_indices: Some(vec![tab_idx]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(path: &str) -> ScanSignature {
        ScanSignature::new(path, 1, 2, 3)
    }

    fn item(id: &str, name: &str, is_dir: bool) -> ScanItem {
        ScanItem::new(id, name, format!("C:\\Tools\\{name}"), is_dir)
    }

    fn folder_tab() -> LauncherTab {
        let items = vec![
            item("folder-a", "Alpha", true),
            item("file-b", "Beta.exe", false),
        ];
        let mut tabs = Vec::new();
        let added = add_folder_tab(&mut tabs, "C:\\Tools", &items, Some(signature("C:\\Tools")));
        assert!(added.is_ok());
        match tabs.into_iter().next() {
            Some(tab) => tab,
            None => LauncherTab {
                id: String::new(),
                tab_type: TabType::Folder,
                title: String::new(),
                folder_path: String::new(),
                rows: DEFAULT_BUTTON_ROWS,
                cols: DEFAULT_BUTTON_COLS,
                hidden_item_ids: Vec::new(),
                slot_positions: BTreeMap::new(),
                buttons: Vec::new(),
                scan_signature: None,
                scan_item_order: None,
            },
        }
    }

    #[test]
    fn add_folder_tab_refresh_reset_and_sort_keep_scan_metadata_rules() {
        let mut tabs = Vec::new();
        let initial_items = vec![
            item("folder-z", "Zoo", true),
            item("file-a", "Alpha.exe", false),
        ];

        let outcome = add_folder_tab(
            &mut tabs,
            "C:\\Tools",
            &initial_items,
            Some(signature("C:\\Tools")),
        );

        assert!(outcome.is_ok());
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].title, "Tools");
        assert_eq!(
            tabs[0].scan_item_order,
            Some(vec![String::from("folder-z"), String::from("file-a")])
        );

        tabs[0].buttons[1].name.clear();
        tabs[0].slot_positions.insert(String::from("file-a"), 4);
        tabs[0]
            .hidden_item_ids
            .extend([String::from("missing"), String::from("file-a")]);
        let refreshed_items = vec![
            item("folder-z", "Zoo", true),
            item("file-b", "Beta.exe", false),
        ];
        let refreshed = refresh_tab(
            &mut tabs,
            "tab-1",
            &refreshed_items,
            false,
            Some(signature("C:\\Tools")),
        );

        assert!(refreshed.is_ok());
        assert_eq!(tabs[0].buttons.len(), 2);
        assert_eq!(tabs[0].buttons[1].item_id, "file-b");
        assert!(tabs[0].slot_positions.is_empty());
        assert!(tabs[0].hidden_item_ids.is_empty());
        assert_eq!(
            tabs[0].scan_item_order,
            Some(vec![String::from("folder-z"), String::from("file-b")])
        );

        tabs[0].slot_positions.insert(String::from("folder-z"), 2);
        let sorted = sort_tab(&mut tabs, "tab-1");
        assert!(matches!(sorted, Ok(Some(_))));
        assert!(tabs[0].slot_positions.is_empty());
        assert_eq!(tabs[0].buttons[0].item_id, "folder-z");
        assert!(tabs[0].scan_signature.is_some());

        let reset_items = vec![item("file-c", "Gamma.exe", false)];
        let reset = reset_tab(
            &mut tabs,
            "tab-1",
            &reset_items,
            Some(signature("C:\\Tools")),
        );
        assert!(reset.is_ok());
        assert_eq!(tabs[0].buttons.len(), 1);
        assert_eq!(tabs[0].buttons[0].item_id, "file-c");
        assert_eq!(tabs[0].scan_item_order, Some(vec![String::from("file-c")]));
    }

    #[test]
    fn unchanged_refresh_result_does_not_mutate_tab() {
        let mut tabs = vec![folder_tab()];
        let snapshot = tabs[0].clone();
        let result = FolderScanResult {
            items: vec![item("ignored", "Ignored.exe", false)],
            failures: Vec::new(),
            cancelled: false,
            signature: Some(signature("C:\\Tools")),
            unchanged: true,
        };

        let outcome = refresh_tab_from_scan_result(&mut tabs, "tab-1", &result);

        assert_eq!(outcome, Ok(None));
        assert_eq!(tabs[0], snapshot);
    }

    #[test]
    fn cancelled_refresh_result_does_not_mutate_tab() {
        let mut tabs = vec![folder_tab()];
        let snapshot = tabs[0].clone();

        let outcome =
            refresh_tab_from_scan_result(&mut tabs, "tab-1", &FolderScanResult::cancelled());

        assert_eq!(outcome, Ok(None));
        assert_eq!(tabs[0], snapshot);
    }

    #[test]
    fn stale_refresh_result_for_missing_tab_is_rejected_without_mutation() {
        let mut tabs = vec![folder_tab()];
        let snapshot = tabs[0].clone();
        let result = FolderScanResult {
            items: vec![item("file-new", "New.exe", false)],
            failures: Vec::new(),
            cancelled: false,
            signature: Some(signature("C:\\Tools")),
            unchanged: false,
        };

        let outcome = refresh_tab_from_scan_result(&mut tabs, "stale-tab", &result);

        assert_eq!(outcome, Err(FolderTabMutationError::TabNotFound));
        assert_eq!(tabs[0], snapshot);
    }

    #[test]
    fn add_manual_tab_creates_default_slots() {
        let mut tabs = Vec::new();

        let outcome = add_manual_tab(&mut tabs);

        assert!(outcome.is_ok());
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].tab_type, TabType::Manual);
        assert_eq!(tabs[0].folder_path, "");
        assert_eq!(
            tabs[0].buttons.len(),
            usize::from(MANUAL_DEFAULT_BUTTON_ROWS) * usize::from(MANUAL_DEFAULT_BUTTON_COLS)
        );
        assert!(tabs[0].scan_signature.is_none());
    }

    #[test]
    fn add_folder_tab_reports_duplicate_folder_index_for_existing_tab_focus() {
        let mut tabs = vec![folder_tab()];
        let items = vec![item("file-new", "New.exe", false)];

        let outcome = add_folder_tab(&mut tabs, "C:\\Tools", &items, Some(signature("C:\\Tools")));

        assert_eq!(
            outcome,
            Err(FolderTabMutationError::DuplicateFolder { tab_idx: 0 })
        );
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].buttons[0].item_id, "folder-a");
    }

    #[test]
    fn set_tab_folder_rebuilds_and_cleans_hidden_items() {
        let mut tabs = vec![folder_tab()];
        tabs[0]
            .hidden_item_ids
            .extend([String::from("folder-a"), String::from("missing")]);
        tabs[0].slot_positions.insert(String::from("folder-a"), 2);
        let new_items = vec![item("file-new", "New.exe", false)];

        let outcome = set_tab_folder(
            &mut tabs,
            "tab-1",
            "C:\\NewTools",
            &new_items,
            Some(signature("C:\\NewTools")),
        );

        assert!(outcome.is_ok());
        assert_eq!(tabs[0].folder_path, "C:\\NewTools");
        assert_eq!(tabs[0].buttons[0].item_id, "file-new");
        assert!(tabs[0].hidden_item_ids.is_empty());
        assert!(tabs[0].slot_positions.is_empty());
    }

    #[test]
    fn set_tab_folder_reports_other_duplicate_folder_index_for_focus() {
        let mut tabs = vec![folder_tab()];
        let items = vec![item("file-new", "New.exe", false)];
        assert!(
            add_folder_tab(
                &mut tabs,
                "C:\\OtherTools",
                &items,
                Some(signature("C:\\OtherTools")),
            )
            .is_ok()
        );

        let outcome = set_tab_folder(
            &mut tabs,
            "tab-2",
            "C:\\Tools",
            &items,
            Some(signature("C:\\Tools")),
        );

        assert_eq!(
            outcome,
            Err(FolderTabMutationError::DuplicateFolder { tab_idx: 0 })
        );
        assert_eq!(tabs[1].folder_path, "C:\\OtherTools");
        assert_eq!(tabs[1].buttons[0].item_id, "file-new");
    }

    #[test]
    fn set_tab_folder_allows_current_folder_reselection_for_rescan() {
        let mut tabs = vec![folder_tab()];
        let items = vec![item("file-new", "New.exe", false)];

        let outcome = set_tab_folder(
            &mut tabs,
            "tab-1",
            "C:\\Tools",
            &items,
            Some(signature("C:\\Tools")),
        );

        assert!(matches!(
            outcome,
            Ok(FolderTabMutationOutcome {
                focus_tab_idx: Some(0),
                ..
            })
        ));
        assert_eq!(tabs[0].folder_path, "C:\\Tools");
        assert_eq!(tabs[0].buttons.len(), 1);
        assert_eq!(tabs[0].buttons[0].item_id, "file-new");
    }

    #[test]
    fn rename_delete_and_layout_update_tabs_without_io() {
        let mut tabs = vec![folder_tab()];
        assert!(add_manual_tab(&mut tabs).is_ok());

        let renamed = rename_tab(&mut tabs, 0, "  Tools  ");
        assert!(renamed.is_ok());
        assert_eq!(tabs[0].title, "Tools");

        let layout = update_tab_layout(&mut tabs, 1, 2, 2);
        assert!(layout.is_ok());
        assert_eq!(tabs[1].rows, 2);
        assert_eq!(tabs[1].cols, 2);
        assert_eq!(tabs[1].buttons.len(), 4);

        let layout = update_tab_layout(&mut tabs, 1, 4, 8);
        assert!(layout.is_ok());
        assert_eq!(tabs[1].buttons.len(), 32);

        let deleted = delete_tab(&mut tabs, 0);
        assert!(matches!(
            deleted,
            Ok(FolderTabMutationOutcome {
                focus_tab_idx: Some(0),
                ..
            })
        ));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].tab_type, TabType::Manual);
    }

    #[test]
    fn hide_and_unhide_items_respect_folder_only_rule() {
        let mut tabs = vec![folder_tab()];

        assert_eq!(hide_item(&mut tabs, 0, "folder-a"), Ok(true));
        assert_eq!(hide_item(&mut tabs, 0, "folder-a"), Ok(false));
        assert_eq!(tabs[0].hidden_item_ids, vec![String::from("folder-a")]);
        assert_eq!(
            unhide_items(&mut tabs, 0, &[String::from("folder-a")]),
            Ok(true)
        );
        assert!(tabs[0].hidden_item_ids.is_empty());

        let manual = add_manual_tab(&mut tabs);
        assert!(manual.is_ok());
        assert_eq!(
            hide_item(&mut tabs, 1, "ignored"),
            Err(FolderTabMutationError::ManualTab)
        );
    }

    #[test]
    fn known_scan_items_follow_stored_scan_order() {
        let mut tab = folder_tab();
        tab.buttons.swap(0, 1);

        let known = build_known_scan_items_from_tab(&tab);

        assert_eq!(
            known,
            Some(vec![
                item("folder-a", "Alpha", true),
                item("file-b", "Beta.exe", false)
            ])
        );
    }
}
