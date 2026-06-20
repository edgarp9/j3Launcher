use crate::app::actions::UserMessage;
use crate::app::folder_tabs::{FolderTabMutationError, can_add_tab};
use crate::app::tab_actions;
use crate::domain::tab::max_button_slot_index;
use crate::domain::tab::path_basename;
use crate::domain::{LauncherButton, LauncherTab, TabType};

const EMPTY_VISIBLE_BUTTON_SLOT: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleButtonSlot {
    pub button_idx: usize,
    pub slot_idx: usize,
}

#[derive(Debug, Default)]
pub struct VisibleButtonSlotScratch {
    assigned_slots: Vec<usize>,
    unassigned_buttons: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuActionAvailability {
    pub add_folder_tab: bool,
    pub add_manual_tab: bool,
    pub set_tab_folder: bool,
    pub tab_layout: bool,
    pub rename_tab: bool,
    pub delete_tab: bool,
    pub move_left: bool,
    pub move_right: bool,
    pub select_prev: bool,
    pub select_next: bool,
    pub sort: bool,
    pub refresh: bool,
    pub reset: bool,
    pub manage_hidden: bool,
}

impl MenuActionAvailability {
    pub fn is_command_enabled(&self, command: MenuCommand) -> bool {
        match command {
            MenuCommand::AddFolderTab => self.add_folder_tab,
            MenuCommand::AddManualTab => self.add_manual_tab,
            MenuCommand::SetTabFolder => self.set_tab_folder,
            MenuCommand::TabLayout => self.tab_layout,
            MenuCommand::RenameTab => self.rename_tab,
            MenuCommand::DeleteTab => self.delete_tab,
            MenuCommand::MoveLeft => self.move_left,
            MenuCommand::MoveRight => self.move_right,
            MenuCommand::SelectPrev => self.select_prev,
            MenuCommand::SelectNext => self.select_next,
            MenuCommand::Sort => self.sort,
            MenuCommand::Refresh => self.refresh,
            MenuCommand::Reset => self.reset,
            MenuCommand::ManageHidden => self.manage_hidden,
            MenuCommand::DarkTheme | MenuCommand::Exit | MenuCommand::About => true,
        }
    }
}

pub trait MenuCommandHandler {
    fn add_folder_tab(&mut self);
    fn add_manual_tab(&mut self);
    fn set_current_tab_folder(&mut self);
    fn edit_current_tab_layout(&mut self);
    fn rename_current_tab(&mut self);
    fn delete_current_tab(&mut self);
    fn move_current_tab_left(&mut self);
    fn move_current_tab_right(&mut self);
    fn select_previous_tab(&mut self);
    fn select_next_tab(&mut self);
    fn sort_current_tab(&mut self);
    fn refresh_current_tab(&mut self);
    fn reset_current_tab(&mut self);
    fn manage_hidden_items(&mut self);
    fn toggle_dark_theme(&mut self);
    fn exit(&mut self);
    fn show_about(&mut self);
}

pub fn dispatch_menu_command(command: MenuCommand, handler: &mut impl MenuCommandHandler) {
    match command {
        MenuCommand::AddFolderTab => handler.add_folder_tab(),
        MenuCommand::AddManualTab => handler.add_manual_tab(),
        MenuCommand::SetTabFolder => handler.set_current_tab_folder(),
        MenuCommand::TabLayout => handler.edit_current_tab_layout(),
        MenuCommand::RenameTab => handler.rename_current_tab(),
        MenuCommand::DeleteTab => handler.delete_current_tab(),
        MenuCommand::MoveLeft => handler.move_current_tab_left(),
        MenuCommand::MoveRight => handler.move_current_tab_right(),
        MenuCommand::SelectPrev => handler.select_previous_tab(),
        MenuCommand::SelectNext => handler.select_next_tab(),
        MenuCommand::Sort => handler.sort_current_tab(),
        MenuCommand::Refresh => handler.refresh_current_tab(),
        MenuCommand::Reset => handler.reset_current_tab(),
        MenuCommand::ManageHidden => handler.manage_hidden_items(),
        MenuCommand::DarkTheme => handler.toggle_dark_theme(),
        MenuCommand::Exit => handler.exit(),
        MenuCommand::About => handler.show_about(),
    }
}

pub fn dispatch_menu_command_if_enabled(
    command: MenuCommand,
    availability: &MenuActionAvailability,
    handler: &mut impl MenuCommandHandler,
) -> bool {
    if !availability.is_command_enabled(command) {
        return false;
    }
    dispatch_menu_command(command, handler);
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    AddFolderTab,
    AddManualTab,
    SetTabFolder,
    TabLayout,
    RenameTab,
    DeleteTab,
    MoveLeft,
    MoveRight,
    SelectPrev,
    SelectNext,
    Sort,
    Refresh,
    Reset,
    ManageHidden,
    DarkTheme,
    Exit,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonContextCommand {
    Edit,
    OpenInExplorer,
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItemSpec {
    pub command: MenuCommand,
    pub label: &'static str,
    pub win32_label: &'static str,
    pub gtk_action: &'static str,
    pub gtk_accels: &'static [&'static str],
}

impl MenuItemSpec {
    pub const fn new(command: MenuCommand, label: &'static str, gtk_action: &'static str) -> Self {
        Self {
            command,
            label,
            win32_label: label,
            gtk_action,
            gtk_accels: &[],
        }
    }

    pub const fn with_accel(
        command: MenuCommand,
        label: &'static str,
        win32_label: &'static str,
        gtk_action: &'static str,
        gtk_accels: &'static [&'static str],
    ) -> Self {
        Self {
            command,
            label,
            win32_label,
            gtk_action,
            gtk_accels,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonContextItemSpec {
    pub command: ButtonContextCommand,
    pub label: &'static str,
}

const ACCEL_MOVE_LEFT: &[&str] = &["<Control><Shift>Left"];
const ACCEL_MOVE_RIGHT: &[&str] = &["<Control><Shift>Right"];
const ACCEL_SELECT_PREV: &[&str] = &["<Control>Page_Up"];
const ACCEL_SELECT_NEXT: &[&str] = &["<Control>Page_Down"];
const ACCEL_SORT: &[&str] = &["F5"];

pub const MAIN_MENU_SECTIONS: &[&[MenuItemSpec]] = &[
    &[
        MenuItemSpec::new(
            MenuCommand::AddFolderTab,
            "Add Folder Tab...",
            "win.add-folder-tab",
        ),
        MenuItemSpec::new(MenuCommand::AddManualTab, "Add Tab", "win.add-manual-tab"),
        MenuItemSpec::new(
            MenuCommand::SetTabFolder,
            "Set Current Tab Folder...",
            "win.set-tab-folder",
        ),
    ],
    &[
        MenuItemSpec::new(
            MenuCommand::TabLayout,
            "Current Tab Layout...",
            "win.tab-layout",
        ),
        MenuItemSpec::new(
            MenuCommand::RenameTab,
            "Rename Current Tab...",
            "win.rename-tab",
        ),
        MenuItemSpec::new(
            MenuCommand::DeleteTab,
            "Delete Current Tab...",
            "win.delete-tab",
        ),
    ],
    &[
        MenuItemSpec::with_accel(
            MenuCommand::MoveLeft,
            "Move Tab Left",
            "Move Tab Left\tCtrl+Shift+Left",
            "win.move-left",
            ACCEL_MOVE_LEFT,
        ),
        MenuItemSpec::with_accel(
            MenuCommand::MoveRight,
            "Move Tab Right",
            "Move Tab Right\tCtrl+Shift+Right",
            "win.move-right",
            ACCEL_MOVE_RIGHT,
        ),
        MenuItemSpec::with_accel(
            MenuCommand::SelectPrev,
            "Select Previous Tab",
            "Select Previous Tab\tCtrl+PageUp",
            "win.select-prev",
            ACCEL_SELECT_PREV,
        ),
        MenuItemSpec::with_accel(
            MenuCommand::SelectNext,
            "Select Next Tab",
            "Select Next Tab\tCtrl+PageDown",
            "win.select-next",
            ACCEL_SELECT_NEXT,
        ),
    ],
    &[
        MenuItemSpec::with_accel(
            MenuCommand::Sort,
            "Sorting Current Tab",
            "Sorting Current Tab\tF5",
            "win.sort",
            ACCEL_SORT,
        ),
        MenuItemSpec::new(MenuCommand::Refresh, "Refresh Current Tab", "win.refresh"),
        MenuItemSpec::new(MenuCommand::Reset, "Reset Current Tab", "win.reset"),
        MenuItemSpec::new(
            MenuCommand::ManageHidden,
            "Manage Hidden Items...",
            "win.manage-hidden",
        ),
    ],
    &[MenuItemSpec::new(
        MenuCommand::DarkTheme,
        "Dark Theme",
        "win.dark-theme",
    )],
    &[MenuItemSpec::new(MenuCommand::Exit, "Exit", "win.exit")],
];

pub const ABOUT_MENU_SECTIONS: &[&[MenuItemSpec]] = &[&[MenuItemSpec::new(
    MenuCommand::About,
    "About j3Launcher...",
    "win.about",
)]];

pub fn main_menu_items() -> impl Iterator<Item = &'static MenuItemSpec> {
    MAIN_MENU_SECTIONS
        .iter()
        .chain(ABOUT_MENU_SECTIONS.iter())
        .flat_map(|section| section.iter())
}

pub const BUTTON_CONTEXT_MENU_ITEMS: &[ButtonContextItemSpec] = &[
    ButtonContextItemSpec {
        command: ButtonContextCommand::Edit,
        label: "Edit",
    },
    ButtonContextItemSpec {
        command: ButtonContextCommand::OpenInExplorer,
        label: "Open in Explorer",
    },
    ButtonContextItemSpec {
        command: ButtonContextCommand::Hide,
        label: "Hide",
    },
];

pub fn menu_action_availability(
    tabs: &[LauncherTab],
    selected_tab_idx: usize,
    scan_idle: bool,
) -> MenuActionAvailability {
    let has_tab = !tabs.is_empty();
    let current_is_folder = tabs
        .get(selected_tab_idx)
        .is_some_and(|tab| tab.tab_type == TabType::Folder);
    let navigation = if has_tab {
        tab_actions::navigation_state(tabs.len(), selected_tab_idx)
    } else {
        tab_actions::navigation_state(0, 0)
    };
    let add_enabled = scan_idle && can_add_tab(tabs);
    let tab_enabled = scan_idle && has_tab;
    let folder_enabled = tab_enabled && current_is_folder;

    MenuActionAvailability {
        add_folder_tab: add_enabled,
        add_manual_tab: add_enabled,
        set_tab_folder: folder_enabled,
        tab_layout: tab_enabled,
        rename_tab: tab_enabled,
        delete_tab: tab_enabled,
        move_left: scan_idle && navigation.can_move_left,
        move_right: scan_idle && navigation.can_move_right,
        select_prev: scan_idle && navigation.can_move_left,
        select_next: scan_idle && navigation.can_move_right,
        sort: folder_enabled,
        refresh: folder_enabled,
        reset: folder_enabled,
        manage_hidden: folder_enabled,
    }
}

pub fn button_context_open_enabled(button: &LauncherButton) -> bool {
    !button.path.trim().is_empty() || !button.source_path.trim().is_empty()
}

pub fn button_open_in_explorer_path(button: &LauncherButton) -> &str {
    if button.path.trim().is_empty() {
        button.source_path.as_str()
    } else {
        button.path.as_str()
    }
}

pub fn button_context_hide_enabled(tab: &LauncherTab, button: &LauncherButton) -> bool {
    tab.tab_type == TabType::Folder && !button.item_id.trim().is_empty()
}

pub fn button_context_command_enabled(
    command: ButtonContextCommand,
    tab: &LauncherTab,
    button: &LauncherButton,
) -> bool {
    match command {
        ButtonContextCommand::Edit => true,
        ButtonContextCommand::OpenInExplorer => button_context_open_enabled(button),
        ButtonContextCommand::Hide => button_context_hide_enabled(tab, button),
    }
}

pub fn folder_tab_mutation_error_message(error: FolderTabMutationError) -> &'static str {
    match error {
        FolderTabMutationError::MaxTabsReached => "더 이상 탭을 추가할 수 없습니다.",
        FolderTabMutationError::TabNotFound | FolderTabMutationError::InvalidTabIndex => {
            "대상 탭을 찾을 수 없습니다."
        }
        FolderTabMutationError::DuplicateFolder { .. } => {
            "이미 같은 폴더를 사용하는 탭이 있습니다."
        }
        FolderTabMutationError::ManualTab => "이 작업은 폴더 탭에서만 사용할 수 있습니다.",
    }
}

pub fn user_message_title(message: &UserMessage) -> &str {
    if message.title.trim().is_empty() {
        "j3Launcher"
    } else {
        message.title.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiddenItem {
    pub item_id: String,
    pub label: String,
}

pub fn hidden_items_for_tab(tab: &LauncherTab) -> Vec<HiddenItem> {
    tab.hidden_item_ids
        .iter()
        .filter_map(|item_id| {
            let item_id = item_id.trim();
            if item_id.is_empty() {
                return None;
            }
            let label = tab
                .buttons
                .iter()
                .find(|button| button.item_id == item_id)
                .map(button_label)
                .filter(|label| !label.trim().is_empty())
                .unwrap_or_else(|| item_id.to_owned());
            Some(HiddenItem {
                item_id: item_id.to_owned(),
                label,
            })
        })
        .collect()
}

pub fn selected_hidden_item_ids_from_indices(
    items: &[HiddenItem],
    indices: impl IntoIterator<Item = usize>,
) -> Vec<String> {
    indices
        .into_iter()
        .filter_map(|index| items.get(index))
        .map(|item| item.item_id.clone())
        .collect()
}

pub fn button_label(button: &LauncherButton) -> String {
    if !button.name.trim().is_empty() {
        button.name.clone()
    } else if !button.source_name.trim().is_empty() {
        button.source_name.clone()
    } else if !button.path.trim().is_empty() {
        path_basename(&button.path).unwrap_or_else(|| button.path.clone())
    } else {
        String::new()
    }
}

pub fn collect_visible_button_slots(
    tab: &LauncherTab,
    slots: &mut Vec<VisibleButtonSlot>,
    scratch: &mut VisibleButtonSlotScratch,
) {
    slots.clear();
    if tab.tab_type == TabType::Manual {
        let visible_slots =
            usize::from(tab.rows.max(1)).saturating_mul(usize::from(tab.cols.max(1)));
        let button_count = tab.buttons.len().min(visible_slots);
        slots.reserve(button_count);
        for button_idx in 0..button_count {
            slots.push(VisibleButtonSlot {
                button_idx,
                slot_idx: button_idx,
            });
        }
        return;
    }

    scratch.assigned_slots.clear();
    scratch.unassigned_buttons.clear();
    let max_slot_index = usize::try_from(max_button_slot_index(tab.cols)).unwrap_or(usize::MAX);
    let mut visible_button_count = 0usize;
    let mut highest_valid_slot: Option<usize> = None;

    for button in &tab.buttons {
        if hidden_item_ids_contains(&tab.hidden_item_ids, button.item_id.as_str()) {
            continue;
        }
        visible_button_count = visible_button_count.saturating_add(1);
        if let Some(slot_idx) =
            folder_button_slot_position(tab, button.item_id.as_str(), max_slot_index)
        {
            highest_valid_slot =
                Some(highest_valid_slot.map_or(slot_idx, |highest| highest.max(slot_idx)));
        }
    }

    scratch.assigned_slots.resize(
        folder_assigned_slot_count(tab, visible_button_count, highest_valid_slot),
        EMPTY_VISIBLE_BUTTON_SLOT,
    );

    for (button_idx, button) in tab.buttons.iter().enumerate() {
        if hidden_item_ids_contains(&tab.hidden_item_ids, button.item_id.as_str()) {
            continue;
        }
        let slot = folder_button_slot_position(tab, button.item_id.as_str(), max_slot_index);
        if let Some(slot_idx) = slot
            && scratch.assigned_slots[slot_idx] == EMPTY_VISIBLE_BUTTON_SLOT
        {
            scratch.assigned_slots[slot_idx] = button_idx;
            continue;
        }
        scratch.unassigned_buttons.push(button_idx);
    }

    let mut next_slot = 0usize;
    for button_idx in scratch.unassigned_buttons.drain(..) {
        while scratch
            .assigned_slots
            .get(next_slot)
            .is_some_and(|button_idx| *button_idx != EMPTY_VISIBLE_BUTTON_SLOT)
        {
            next_slot += 1;
        }
        if next_slot == scratch.assigned_slots.len() {
            scratch.assigned_slots.push(button_idx);
        } else {
            scratch.assigned_slots[next_slot] = button_idx;
        }
    }

    slots.reserve(visible_button_count);
    for (slot_idx, button_idx) in scratch.assigned_slots.iter().copied().enumerate() {
        if button_idx != EMPTY_VISIBLE_BUTTON_SLOT {
            slots.push(VisibleButtonSlot {
                button_idx,
                slot_idx,
            });
        }
    }
}

fn folder_assigned_slot_count(
    tab: &LauncherTab,
    visible_button_count: usize,
    highest_valid_slot: Option<usize>,
) -> usize {
    let cols = usize::from(tab.cols.max(1));
    let rows = usize::from(tab.rows.max(1));
    let configured_slots = rows.saturating_mul(cols);
    visible_button_count
        .max(configured_slots)
        .max(highest_valid_slot.map_or(0, |slot| slot.saturating_add(1)))
}

fn folder_button_slot_position(
    tab: &LauncherTab,
    item_id: &str,
    max_slot_index: usize,
) -> Option<usize> {
    let slot = *tab.slot_positions.get(item_id)?;
    let slot = usize::try_from(slot).ok()?;
    (slot <= max_slot_index).then_some(slot)
}

fn hidden_item_ids_contains(hidden_item_ids: &[String], item_id: &str) -> bool {
    hidden_item_ids
        .iter()
        .any(|hidden| hidden.as_str() == item_id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::{
        DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, MANUAL_DEFAULT_BUTTON_COLS,
        MANUAL_DEFAULT_BUTTON_ROWS, ScanSignature,
    };

    fn button(item_id: &str, name: &str) -> LauncherButton {
        LauncherButton {
            item_id: item_id.to_owned(),
            source_name: name.to_owned(),
            source_path: format!("/tmp/{name}"),
            is_dir: false,
            name: name.to_owned(),
            path: format!("/tmp/{name}"),
            params: String::new(),
            admin: false,
            action: 0,
            auto_enter: false,
        }
    }

    fn folder_tab(id: &str) -> LauncherTab {
        LauncherTab {
            id: id.to_owned(),
            tab_type: TabType::Folder,
            title: id.to_owned(),
            folder_path: format!("/tmp/{id}"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: DEFAULT_BUTTON_COLS,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: vec![button("item-1", "Item")],
            scan_signature: Some(ScanSignature::new(format!("/tmp/{id}"), 1, 2, 3)),
            scan_item_order: Some(vec![String::from("item-1")]),
        }
    }

    fn manual_tab(id: &str) -> LauncherTab {
        LauncherTab {
            id: id.to_owned(),
            tab_type: TabType::Manual,
            title: id.to_owned(),
            folder_path: String::new(),
            rows: MANUAL_DEFAULT_BUTTON_ROWS,
            cols: MANUAL_DEFAULT_BUTTON_COLS,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: vec![LauncherButton::manual_default()],
            scan_signature: None,
            scan_item_order: None,
        }
    }

    #[test]
    fn folder_slots_hide_items_and_preserve_explicit_positions() {
        let mut slot_positions = BTreeMap::new();
        slot_positions.insert(String::from("item-visible"), 3);
        slot_positions.insert(String::from("item-hidden"), max_button_slot_index(32));
        let tab = LauncherTab {
            id: String::from("tab-1"),
            tab_type: TabType::Folder,
            title: String::from("Folder"),
            folder_path: String::from("/tmp"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: DEFAULT_BUTTON_COLS,
            hidden_item_ids: vec![String::from("item-hidden")],
            slot_positions,
            buttons: vec![
                button("item-hidden", "Hidden"),
                button("item-visible", "Visible"),
            ],
            scan_signature: Some(ScanSignature::new("/tmp", 1, 2, 3)),
            scan_item_order: Some(vec![
                String::from("item-hidden"),
                String::from("item-visible"),
            ]),
        };
        let mut slots = Vec::new();
        let mut scratch = VisibleButtonSlotScratch::default();

        collect_visible_button_slots(&tab, &mut slots, &mut scratch);

        assert_eq!(
            slots,
            vec![VisibleButtonSlot {
                button_idx: 1,
                slot_idx: 3
            }]
        );
    }

    #[test]
    fn menu_actions_match_win32_folder_tab_rules() {
        let tabs = vec![folder_tab("a"), folder_tab("b")];

        let availability = menu_action_availability(&tabs, 0, true);

        assert!(availability.add_folder_tab);
        assert!(availability.add_manual_tab);
        assert!(availability.set_tab_folder);
        assert!(availability.tab_layout);
        assert!(availability.rename_tab);
        assert!(availability.delete_tab);
        assert!(!availability.move_left);
        assert!(availability.move_right);
        assert!(!availability.select_prev);
        assert!(availability.select_next);
        assert!(availability.sort);
        assert!(availability.refresh);
        assert!(availability.reset);
        assert!(availability.manage_hidden);
    }

    #[test]
    fn menu_actions_disable_folder_commands_for_manual_tabs() {
        let tabs = vec![folder_tab("a"), manual_tab("manual")];

        let availability = menu_action_availability(&tabs, 1, true);

        assert!(!availability.set_tab_folder);
        assert!(availability.tab_layout);
        assert!(availability.rename_tab);
        assert!(availability.delete_tab);
        assert!(availability.move_left);
        assert!(!availability.move_right);
        assert!(!availability.sort);
        assert!(!availability.refresh);
        assert!(!availability.reset);
        assert!(!availability.manage_hidden);
    }

    #[test]
    fn menu_actions_disable_mutating_commands_during_scan() {
        let tabs = vec![folder_tab("a"), folder_tab("b")];

        let availability = menu_action_availability(&tabs, 1, false);

        assert!(!availability.add_folder_tab);
        assert!(!availability.add_manual_tab);
        assert!(!availability.set_tab_folder);
        assert!(!availability.tab_layout);
        assert!(!availability.rename_tab);
        assert!(!availability.delete_tab);
        assert!(!availability.move_left);
        assert!(!availability.move_right);
        assert!(!availability.select_prev);
        assert!(!availability.select_next);
        assert!(!availability.sort);
        assert!(!availability.refresh);
        assert!(!availability.reset);
        assert!(!availability.manage_hidden);
    }

    #[test]
    fn menu_command_enabled_uses_same_availability_rules() {
        let tabs = vec![folder_tab("a"), manual_tab("manual")];

        let availability = menu_action_availability(&tabs, 1, true);

        assert!(availability.is_command_enabled(MenuCommand::AddFolderTab));
        assert!(availability.is_command_enabled(MenuCommand::AddManualTab));
        assert!(!availability.is_command_enabled(MenuCommand::SetTabFolder));
        assert!(availability.is_command_enabled(MenuCommand::TabLayout));
        assert!(availability.is_command_enabled(MenuCommand::RenameTab));
        assert!(availability.is_command_enabled(MenuCommand::DeleteTab));
        assert!(availability.is_command_enabled(MenuCommand::MoveLeft));
        assert!(!availability.is_command_enabled(MenuCommand::MoveRight));
        assert!(availability.is_command_enabled(MenuCommand::SelectPrev));
        assert!(!availability.is_command_enabled(MenuCommand::SelectNext));
        assert!(!availability.is_command_enabled(MenuCommand::Sort));
        assert!(!availability.is_command_enabled(MenuCommand::Refresh));
        assert!(!availability.is_command_enabled(MenuCommand::Reset));
        assert!(!availability.is_command_enabled(MenuCommand::ManageHidden));
        assert!(availability.is_command_enabled(MenuCommand::DarkTheme));
        assert!(availability.is_command_enabled(MenuCommand::Exit));
        assert!(availability.is_command_enabled(MenuCommand::About));

        let scanning = menu_action_availability(&tabs, 1, false);
        assert!(!scanning.is_command_enabled(MenuCommand::AddManualTab));
        assert!(!scanning.is_command_enabled(MenuCommand::TabLayout));
        assert!(!scanning.is_command_enabled(MenuCommand::RenameTab));
        assert!(!scanning.is_command_enabled(MenuCommand::DeleteTab));
        assert!(!scanning.is_command_enabled(MenuCommand::Sort));
        assert!(scanning.is_command_enabled(MenuCommand::DarkTheme));
        assert!(scanning.is_command_enabled(MenuCommand::Exit));
        assert!(scanning.is_command_enabled(MenuCommand::About));
    }

    #[test]
    fn main_menu_spec_preserves_win32_order_and_accelerators() {
        let sections = MAIN_MENU_SECTIONS
            .iter()
            .map(|section| {
                section
                    .iter()
                    .map(|item| item.win32_label)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sections,
            vec![
                vec!["Add Folder Tab...", "Add Tab", "Set Current Tab Folder..."],
                vec![
                    "Current Tab Layout...",
                    "Rename Current Tab...",
                    "Delete Current Tab...",
                ],
                vec![
                    "Move Tab Left\tCtrl+Shift+Left",
                    "Move Tab Right\tCtrl+Shift+Right",
                    "Select Previous Tab\tCtrl+PageUp",
                    "Select Next Tab\tCtrl+PageDown",
                ],
                vec![
                    "Sorting Current Tab\tF5",
                    "Refresh Current Tab",
                    "Reset Current Tab",
                    "Manage Hidden Items...",
                ],
                vec!["Dark Theme"],
                vec!["Exit"],
            ]
        );

        let labels = MAIN_MENU_SECTIONS
            .iter()
            .flat_map(|section| section.iter().map(|item| item.win32_label))
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "Add Folder Tab...",
                "Add Tab",
                "Set Current Tab Folder...",
                "Current Tab Layout...",
                "Rename Current Tab...",
                "Delete Current Tab...",
                "Move Tab Left\tCtrl+Shift+Left",
                "Move Tab Right\tCtrl+Shift+Right",
                "Select Previous Tab\tCtrl+PageUp",
                "Select Next Tab\tCtrl+PageDown",
                "Sorting Current Tab\tF5",
                "Refresh Current Tab",
                "Reset Current Tab",
                "Manage Hidden Items...",
                "Dark Theme",
                "Exit",
            ]
        );
        assert_eq!(
            MAIN_MENU_SECTIONS
                .iter()
                .flat_map(|section| section.iter())
                .filter(|item| !item.gtk_accels.is_empty())
                .map(|item| (item.gtk_action, item.gtk_accels))
                .collect::<Vec<_>>(),
            vec![
                ("win.move-left", ACCEL_MOVE_LEFT),
                ("win.move-right", ACCEL_MOVE_RIGHT),
                ("win.select-prev", ACCEL_SELECT_PREV),
                ("win.select-next", ACCEL_SELECT_NEXT),
                ("win.sort", ACCEL_SORT),
            ]
        );
        assert_eq!(
            ABOUT_MENU_SECTIONS
                .iter()
                .flat_map(|section| section.iter().map(|item| item.win32_label))
                .collect::<Vec<_>>(),
            vec!["About j3Launcher..."]
        );
    }

    #[derive(Default)]
    struct RecordingMenuCommandHandler {
        calls: Vec<&'static str>,
    }

    impl MenuCommandHandler for RecordingMenuCommandHandler {
        fn add_folder_tab(&mut self) {
            self.calls.push("add-folder-tab");
        }

        fn add_manual_tab(&mut self) {
            self.calls.push("add-manual-tab");
        }

        fn set_current_tab_folder(&mut self) {
            self.calls.push("set-current-tab-folder");
        }

        fn edit_current_tab_layout(&mut self) {
            self.calls.push("edit-current-tab-layout");
        }

        fn rename_current_tab(&mut self) {
            self.calls.push("rename-current-tab");
        }

        fn delete_current_tab(&mut self) {
            self.calls.push("delete-current-tab");
        }

        fn move_current_tab_left(&mut self) {
            self.calls.push("move-current-tab-left");
        }

        fn move_current_tab_right(&mut self) {
            self.calls.push("move-current-tab-right");
        }

        fn select_previous_tab(&mut self) {
            self.calls.push("select-previous-tab");
        }

        fn select_next_tab(&mut self) {
            self.calls.push("select-next-tab");
        }

        fn sort_current_tab(&mut self) {
            self.calls.push("sort-current-tab");
        }

        fn refresh_current_tab(&mut self) {
            self.calls.push("refresh-current-tab");
        }

        fn reset_current_tab(&mut self) {
            self.calls.push("reset-current-tab");
        }

        fn manage_hidden_items(&mut self) {
            self.calls.push("manage-hidden-items");
        }

        fn toggle_dark_theme(&mut self) {
            self.calls.push("toggle-dark-theme");
        }

        fn exit(&mut self) {
            self.calls.push("exit");
        }

        fn show_about(&mut self) {
            self.calls.push("show-about");
        }
    }

    #[test]
    fn main_menu_dispatch_preserves_win32_command_targets() {
        let mut handler = RecordingMenuCommandHandler::default();

        for command in main_menu_items().map(|item| item.command) {
            dispatch_menu_command(command, &mut handler);
        }

        assert_eq!(
            handler.calls,
            vec![
                "add-folder-tab",
                "add-manual-tab",
                "set-current-tab-folder",
                "edit-current-tab-layout",
                "rename-current-tab",
                "delete-current-tab",
                "move-current-tab-left",
                "move-current-tab-right",
                "select-previous-tab",
                "select-next-tab",
                "sort-current-tab",
                "refresh-current-tab",
                "reset-current-tab",
                "manage-hidden-items",
                "toggle-dark-theme",
                "exit",
                "show-about",
            ]
        );
    }

    #[test]
    fn guarded_menu_dispatch_skips_disabled_commands() {
        let tabs = vec![folder_tab("a"), manual_tab("manual")];
        let availability = menu_action_availability(&tabs, 1, true);
        let mut handler = RecordingMenuCommandHandler::default();

        assert!(!dispatch_menu_command_if_enabled(
            MenuCommand::Sort,
            &availability,
            &mut handler
        ));
        assert!(handler.calls.is_empty());

        assert!(dispatch_menu_command_if_enabled(
            MenuCommand::RenameTab,
            &availability,
            &mut handler
        ));
        assert_eq!(handler.calls, vec!["rename-current-tab"]);
    }

    #[test]
    fn guarded_menu_dispatch_skips_every_disabled_command_for_manual_and_scan_states() {
        let tabs = vec![folder_tab("a"), manual_tab("manual")];
        let manual_availability = menu_action_availability(&tabs, 1, true);
        let manual_disabled = [
            MenuCommand::SetTabFolder,
            MenuCommand::MoveRight,
            MenuCommand::SelectNext,
            MenuCommand::Sort,
            MenuCommand::Refresh,
            MenuCommand::Reset,
            MenuCommand::ManageHidden,
        ];
        for command in manual_disabled {
            let mut handler = RecordingMenuCommandHandler::default();
            assert!(!dispatch_menu_command_if_enabled(
                command,
                &manual_availability,
                &mut handler
            ));
            assert!(handler.calls.is_empty(), "{command:?} was dispatched");
        }

        let scan_availability = menu_action_availability(&tabs, 1, false);
        let scan_disabled = [
            MenuCommand::AddFolderTab,
            MenuCommand::AddManualTab,
            MenuCommand::SetTabFolder,
            MenuCommand::TabLayout,
            MenuCommand::RenameTab,
            MenuCommand::DeleteTab,
            MenuCommand::MoveLeft,
            MenuCommand::MoveRight,
            MenuCommand::SelectPrev,
            MenuCommand::SelectNext,
            MenuCommand::Sort,
            MenuCommand::Refresh,
            MenuCommand::Reset,
            MenuCommand::ManageHidden,
        ];
        for command in scan_disabled {
            let mut handler = RecordingMenuCommandHandler::default();
            assert!(!dispatch_menu_command_if_enabled(
                command,
                &scan_availability,
                &mut handler
            ));
            assert!(handler.calls.is_empty(), "{command:?} was dispatched");
        }

        let mut handler = RecordingMenuCommandHandler::default();
        assert!(dispatch_menu_command_if_enabled(
            MenuCommand::DarkTheme,
            &scan_availability,
            &mut handler
        ));
        assert!(dispatch_menu_command_if_enabled(
            MenuCommand::Exit,
            &scan_availability,
            &mut handler
        ));
        assert!(dispatch_menu_command_if_enabled(
            MenuCommand::About,
            &scan_availability,
            &mut handler
        ));
        assert_eq!(
            handler.calls,
            vec!["toggle-dark-theme", "exit", "show-about"]
        );
    }

    #[test]
    fn button_context_menu_spec_preserves_win32_order_and_rules() {
        let labels = BUTTON_CONTEXT_MENU_ITEMS
            .iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["Edit", "Open in Explorer", "Hide"]);

        let folder_tab = folder_tab("folder");
        let manual_tab = manual_tab("manual");
        let empty_button = LauncherButton::default();
        let path_button = LauncherButton {
            path: String::from("tool.exe"),
            ..LauncherButton::default()
        };
        let source_path_button = LauncherButton {
            source_path: String::from("C:/Tools/source.exe"),
            ..LauncherButton::default()
        };
        let folder_item = LauncherButton {
            item_id: String::from("item-1"),
            ..LauncherButton::default()
        };

        assert!(button_context_command_enabled(
            ButtonContextCommand::Edit,
            &folder_tab,
            &empty_button
        ));
        assert!(!button_context_command_enabled(
            ButtonContextCommand::OpenInExplorer,
            &folder_tab,
            &empty_button
        ));
        assert!(button_context_command_enabled(
            ButtonContextCommand::OpenInExplorer,
            &folder_tab,
            &path_button
        ));
        assert!(button_context_command_enabled(
            ButtonContextCommand::Hide,
            &folder_tab,
            &folder_item
        ));
        assert!(!button_context_command_enabled(
            ButtonContextCommand::Hide,
            &manual_tab,
            &folder_item
        ));
        assert!(!button_context_open_enabled(&empty_button));
        assert!(button_context_open_enabled(&path_button));
        assert!(button_context_open_enabled(&source_path_button));
        assert!(button_context_hide_enabled(&folder_tab, &folder_item));
        assert!(!button_context_hide_enabled(&manual_tab, &folder_item));
        assert!(!button_context_hide_enabled(&folder_tab, &empty_button));
    }

    #[test]
    fn folder_tab_mutation_error_messages_match_win32_text() {
        assert_eq!(
            folder_tab_mutation_error_message(FolderTabMutationError::MaxTabsReached),
            "더 이상 탭을 추가할 수 없습니다."
        );
        assert_eq!(
            folder_tab_mutation_error_message(FolderTabMutationError::TabNotFound),
            "대상 탭을 찾을 수 없습니다."
        );
        assert_eq!(
            folder_tab_mutation_error_message(FolderTabMutationError::InvalidTabIndex),
            "대상 탭을 찾을 수 없습니다."
        );
        assert_eq!(
            folder_tab_mutation_error_message(FolderTabMutationError::DuplicateFolder {
                tab_idx: 1
            }),
            "이미 같은 폴더를 사용하는 탭이 있습니다."
        );
        assert_eq!(
            folder_tab_mutation_error_message(FolderTabMutationError::ManualTab),
            "이 작업은 폴더 탭에서만 사용할 수 있습니다."
        );
    }

    #[test]
    fn user_message_title_uses_win32_empty_title_fallback() {
        let empty = UserMessage::new("info", "", "message");
        let whitespace = UserMessage::new("info", "   ", "message");
        let titled = UserMessage::new("info", "Launch", "message");

        assert_eq!(user_message_title(&empty), "j3Launcher");
        assert_eq!(user_message_title(&whitespace), "j3Launcher");
        assert_eq!(user_message_title(&titled), "Launch");
    }

    #[test]
    fn button_open_in_explorer_path_matches_win32_preference_order() {
        let custom_path = LauncherButton {
            path: String::from(" C:/Tools/custom.exe "),
            source_path: String::from("C:/Tools/source.exe"),
            ..LauncherButton::default()
        };
        let source_fallback = LauncherButton {
            path: String::from("   "),
            source_path: String::from("C:/Tools/source.exe"),
            ..LauncherButton::default()
        };
        let empty = LauncherButton::default();

        assert_eq!(
            button_open_in_explorer_path(&custom_path),
            " C:/Tools/custom.exe "
        );
        assert_eq!(
            button_open_in_explorer_path(&source_fallback),
            "C:/Tools/source.exe"
        );
        assert_eq!(button_open_in_explorer_path(&empty), "");
    }

    #[test]
    fn hidden_items_for_tab_preserves_win32_order_and_label_policy() {
        let mut tab = folder_tab("folder");
        tab.buttons = vec![
            LauncherButton {
                item_id: String::from("item-a"),
                name: String::new(),
                source_name: String::from("Alpha"),
                ..LauncherButton::default()
            },
            LauncherButton {
                item_id: String::from("item-b"),
                name: String::from("  "),
                source_name: String::new(),
                path: String::from("/opt/tools/beta.exe"),
                ..LauncherButton::default()
            },
            LauncherButton {
                item_id: String::from("duplicate"),
                name: String::from("First duplicate"),
                ..LauncherButton::default()
            },
            LauncherButton {
                item_id: String::from("duplicate"),
                name: String::from("Second duplicate"),
                ..LauncherButton::default()
            },
        ];
        tab.hidden_item_ids = vec![
            String::from(" item-a "),
            String::from("missing"),
            String::from("item-b"),
            String::new(),
            String::from("duplicate"),
        ];

        assert_eq!(
            hidden_items_for_tab(&tab),
            vec![
                HiddenItem {
                    item_id: String::from("item-a"),
                    label: String::from("Alpha"),
                },
                HiddenItem {
                    item_id: String::from("missing"),
                    label: String::from("missing"),
                },
                HiddenItem {
                    item_id: String::from("item-b"),
                    label: String::from("beta.exe"),
                },
                HiddenItem {
                    item_id: String::from("duplicate"),
                    label: String::from("First duplicate"),
                },
            ]
        );
    }

    #[test]
    fn selected_hidden_item_ids_from_indices_ignores_out_of_range_indices() {
        let items = vec![
            HiddenItem {
                item_id: String::from("one"),
                label: String::from("One"),
            },
            HiddenItem {
                item_id: String::from("two"),
                label: String::from("Two"),
            },
            HiddenItem {
                item_id: String::from("three"),
                label: String::from("Three"),
            },
        ];

        assert_eq!(
            selected_hidden_item_ids_from_indices(&items, [2, 99, 0]),
            vec![String::from("three"), String::from("one")]
        );
    }
}
