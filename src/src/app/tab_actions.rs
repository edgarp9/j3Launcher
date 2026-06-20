use crate::domain::LauncherTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabMoveDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabMoveOutcome {
    pub moved: bool,
    pub blocked: bool,
    pub focus_tab_idx: Option<usize>,
    pub source_tab_idx: Option<usize>,
    pub target_tab_idx: Option<usize>,
    pub moved_tab_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabNavigationState {
    pub selected_tab_idx: Option<usize>,
    pub can_move_left: bool,
    pub can_move_right: bool,
}

pub fn calculate_tab_move_target(
    tab_count: usize,
    current_idx: usize,
    direction: TabMoveDirection,
) -> Option<usize> {
    if current_idx >= tab_count {
        return None;
    }
    match direction {
        TabMoveDirection::Left => current_idx.checked_sub(1),
        TabMoveDirection::Right => {
            let target = current_idx.checked_add(1)?;
            (target < tab_count).then_some(target)
        }
    }
}

pub fn move_tab(
    folder_tabs: &mut [LauncherTab],
    current_idx: usize,
    direction: TabMoveDirection,
) -> TabMoveOutcome {
    let Some(target_idx) = calculate_tab_move_target(folder_tabs.len(), current_idx, direction)
    else {
        return blocked_move();
    };
    let moved_tab_id = folder_tabs.get(current_idx).map(|tab| tab.id.clone());
    folder_tabs.swap(current_idx, target_idx);

    TabMoveOutcome {
        moved: true,
        blocked: false,
        focus_tab_idx: Some(target_idx),
        source_tab_idx: Some(current_idx),
        target_tab_idx: Some(target_idx),
        moved_tab_id,
    }
}

pub fn select_tab(folder_tabs: &[LauncherTab], requested_idx: usize) -> TabNavigationState {
    if requested_idx >= folder_tabs.len() {
        return TabNavigationState {
            selected_tab_idx: None,
            can_move_left: false,
            can_move_right: false,
        };
    }
    navigation_state(folder_tabs.len(), requested_idx)
}

pub fn navigation_state(tab_count: usize, selected_idx: usize) -> TabNavigationState {
    if selected_idx >= tab_count {
        return TabNavigationState {
            selected_tab_idx: None,
            can_move_left: false,
            can_move_right: false,
        };
    }

    TabNavigationState {
        selected_tab_idx: Some(selected_idx),
        can_move_left: selected_idx > 0,
        can_move_right: selected_idx + 1 < tab_count,
    }
}

fn blocked_move() -> TabMoveOutcome {
    TabMoveOutcome {
        moved: false,
        blocked: true,
        focus_tab_idx: None,
        source_tab_idx: None,
        target_tab_idx: None,
        moved_tab_id: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::{
        DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, LauncherButton, ScanSignature, TabType,
    };

    fn tab(id: &str) -> LauncherTab {
        LauncherTab {
            id: id.to_owned(),
            tab_type: TabType::Folder,
            title: id.to_owned(),
            folder_path: format!("C:\\{id}"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: DEFAULT_BUTTON_COLS,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: vec![LauncherButton::manual_default()],
            scan_signature: Some(ScanSignature::new(format!("C:\\{id}"), 1, 2, 3)),
            scan_item_order: Some(Vec::new()),
        }
    }

    #[test]
    fn tab_move_respects_boundaries() {
        let mut tabs = vec![tab("a"), tab("b"), tab("c")];

        let blocked_left = move_tab(&mut tabs, 0, TabMoveDirection::Left);
        assert!(blocked_left.blocked);
        assert_eq!(tabs[0].id, "a");

        let moved_right = move_tab(&mut tabs, 0, TabMoveDirection::Right);
        assert!(moved_right.moved);
        assert_eq!(moved_right.focus_tab_idx, Some(1));
        assert_eq!(moved_right.moved_tab_id, Some(String::from("a")));
        assert_eq!(
            tabs.iter().map(|tab| tab.id.as_str()).collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );

        let blocked_out_of_range = move_tab(&mut tabs, 10, TabMoveDirection::Right);
        assert!(blocked_out_of_range.blocked);
    }

    #[test]
    fn tab_selection_reports_navigation_boundaries() {
        let tabs = vec![tab("a"), tab("b")];

        assert_eq!(
            select_tab(&tabs, 0),
            TabNavigationState {
                selected_tab_idx: Some(0),
                can_move_left: false,
                can_move_right: true,
            }
        );
        assert_eq!(
            select_tab(&tabs, 1),
            TabNavigationState {
                selected_tab_idx: Some(1),
                can_move_left: true,
                can_move_right: false,
            }
        );
        assert_eq!(
            select_tab(&tabs, 2),
            TabNavigationState {
                selected_tab_idx: None,
                can_move_left: false,
                can_move_right: false,
            }
        );
    }
}
