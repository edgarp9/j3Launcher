use std::collections::{BTreeMap, HashSet};

use crate::domain::tab::max_button_slot_index;
use crate::domain::{LauncherTab, TabType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonSlotMove {
    pub tab_idx: usize,
    pub source_button_idx: usize,
    pub source_slot_idx: usize,
    pub target_button_idx: usize,
    pub target_slot_idx: usize,
}

impl ButtonSlotMove {
    pub fn new(
        tab_idx: usize,
        source_button_idx: usize,
        source_slot_idx: usize,
        target_button_idx: usize,
        target_slot_idx: usize,
    ) -> Self {
        Self {
            tab_idx,
            source_button_idx,
            source_slot_idx,
            target_button_idx,
            target_slot_idx,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonSlotMoveOutcome {
    pub focus_tab_idx: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSlotMoveError {
    InvalidTabIndex,
    InvalidButtonIndex,
    InvalidSlotIndex,
}

pub type ButtonSlotMoveResult<T = Option<ButtonSlotMoveOutcome>> =
    std::result::Result<T, ButtonSlotMoveError>;

pub fn move_button_between_slots(
    folder_tabs: &mut [LauncherTab],
    request: ButtonSlotMove,
) -> ButtonSlotMoveResult {
    let tab = folder_tabs
        .get_mut(request.tab_idx)
        .ok_or(ButtonSlotMoveError::InvalidTabIndex)?;

    if !button_index_exists(tab, request.source_button_idx)
        || !button_index_exists(tab, request.target_button_idx)
    {
        return Err(ButtonSlotMoveError::InvalidButtonIndex);
    }

    if request.source_button_idx == request.target_button_idx {
        return Ok(None);
    }

    if tab.tab_type == TabType::Manual {
        tab.buttons
            .swap(request.source_button_idx, request.target_button_idx);
        return Ok(Some(ButtonSlotMoveOutcome {
            focus_tab_idx: request.tab_idx,
        }));
    }

    move_folder_button_slots(tab, request)?;
    Ok(Some(ButtonSlotMoveOutcome {
        focus_tab_idx: request.tab_idx,
    }))
}

fn move_folder_button_slots(
    tab: &mut LauncherTab,
    request: ButtonSlotMove,
) -> ButtonSlotMoveResult<()> {
    let source_slot = valid_button_slot(request.source_slot_idx, tab.cols)?;
    let target_slot = valid_button_slot(request.target_slot_idx, tab.cols)?;
    if source_slot == target_slot {
        return Ok(());
    }

    let source_item_id = button_item_id(tab, request.source_button_idx)?;
    let target_item_id = button_item_id(tab, request.target_button_idx)?;

    tab.slot_positions.insert(source_item_id, target_slot);
    tab.slot_positions.insert(target_item_id, source_slot);
    let valid_button_ids = tab
        .buttons
        .iter()
        .map(|button| button.item_id.clone())
        .collect::<HashSet<_>>();
    tab.slot_positions = sanitize_slot_positions(&tab.slot_positions, &valid_button_ids);
    Ok(())
}

fn button_index_exists(tab: &LauncherTab, button_idx: usize) -> bool {
    button_idx < tab.buttons.len()
}

fn button_item_id(tab: &LauncherTab, button_idx: usize) -> Result<String, ButtonSlotMoveError> {
    tab.buttons
        .get(button_idx)
        .map(|button| button.item_id.trim().to_owned())
        .filter(|item_id| !item_id.is_empty())
        .ok_or(ButtonSlotMoveError::InvalidButtonIndex)
}

fn valid_button_slot(slot_idx: usize, cols: u16) -> Result<u64, ButtonSlotMoveError> {
    let slot_idx = u64::try_from(slot_idx).map_err(|_| ButtonSlotMoveError::InvalidSlotIndex)?;
    if slot_idx > max_button_slot_index(cols) {
        return Err(ButtonSlotMoveError::InvalidSlotIndex);
    }
    Ok(slot_idx)
}

fn sanitize_slot_positions(
    raw_slot_positions: &BTreeMap<String, u64>,
    valid_item_ids: &HashSet<String>,
) -> BTreeMap<String, u64> {
    let mut normalized = BTreeMap::new();
    for (item_id, slot_index) in raw_slot_positions {
        let item_id = item_id.trim();
        if !item_id.is_empty() && valid_item_ids.contains(item_id) {
            normalized.insert(item_id.to_owned(), *slot_index);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::folder_tabs::{add_folder_tab, add_manual_tab};
    use crate::domain::{
        DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, LauncherButton, ScanItem, ScanSignature,
    };

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
    fn manual_slot_move_swaps_button_settings() {
        let mut tabs = Vec::new();
        assert!(add_manual_tab(&mut tabs).is_ok());
        tabs[0].buttons[0].name = String::from("Alpha");
        tabs[0].buttons[0].path = String::from("C:\\Tools\\alpha.exe");
        tabs[0].buttons[1].name = String::from("Beta");
        tabs[0].buttons[1].path = String::from("C:\\Tools\\beta.exe");

        let outcome = move_button_between_slots(&mut tabs, ButtonSlotMove::new(0, 0, 0, 1, 1));

        assert_eq!(
            outcome,
            Ok(Some(ButtonSlotMoveOutcome { focus_tab_idx: 0 }))
        );
        assert_eq!(tabs[0].buttons[0].name, "Beta");
        assert_eq!(tabs[0].buttons[1].name, "Alpha");

        let outcome = move_button_between_slots(&mut tabs, ButtonSlotMove::new(0, 1, 1, 2, 2));

        assert_eq!(
            outcome,
            Ok(Some(ButtonSlotMoveOutcome { focus_tab_idx: 0 }))
        );
        assert_eq!(tabs[0].buttons[1], LauncherButton::manual_default());
        assert_eq!(tabs[0].buttons[2].name, "Alpha");
        assert_eq!(tabs[0].buttons[2].path, "C:\\Tools\\alpha.exe");
    }

    #[test]
    fn folder_slot_move_swaps_visible_positions() {
        let mut tabs = vec![folder_tab()];

        let outcome = move_button_between_slots(&mut tabs, ButtonSlotMove::new(0, 0, 0, 1, 1));

        assert_eq!(
            outcome,
            Ok(Some(ButtonSlotMoveOutcome { focus_tab_idx: 0 }))
        );
        assert_eq!(tabs[0].slot_positions.get("folder-a"), Some(&1));
        assert_eq!(tabs[0].slot_positions.get("file-b"), Some(&0));
        assert_eq!(tabs[0].buttons[0].name, "Alpha");
        assert_eq!(tabs[0].buttons[1].name, "Beta.exe");
    }

    #[test]
    fn moving_same_button_does_not_mutate() {
        let mut tabs = vec![folder_tab()];
        let snapshot = tabs[0].clone();

        let outcome = move_button_between_slots(&mut tabs, ButtonSlotMove::new(0, 0, 0, 0, 0));

        assert_eq!(outcome, Ok(None));
        assert_eq!(tabs[0], snapshot);
    }

    #[test]
    fn folder_slot_move_rejects_invalid_targets() {
        let mut tabs = vec![folder_tab()];

        assert_eq!(
            move_button_between_slots(&mut tabs, ButtonSlotMove::new(1, 0, 0, 1, 1)),
            Err(ButtonSlotMoveError::InvalidTabIndex)
        );
        assert_eq!(
            move_button_between_slots(&mut tabs, ButtonSlotMove::new(0, 0, 0, 9, 1)),
            Err(ButtonSlotMoveError::InvalidButtonIndex)
        );
    }
}
