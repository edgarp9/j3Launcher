use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use gtk::gio;
use gtk::glib::variant::ToVariant;
use gtk::glib::{self, ControlFlow};
use gtk::prelude::*;

use crate::Result;
use crate::app::actions::{
    ActionFailure, ActionResult, AdminLaunchResult, ExplorerOpenFeedback, LauncherActionService,
    LauncherPlatform, UserMessage, admin_result_user_message, expand_environment_variables,
    has_unresolved_windows_env_reference, has_windows_path_syntax, resolve_runtime_path,
    runtime_uri_scheme,
};
use crate::app::button_layout::{ButtonSlotMove, ButtonSlotMoveError, move_button_between_slots};
use crate::app::config_service::ConfigService;
use crate::app::folder_tabs::{
    FolderTabMutationError, add_folder_tab, add_manual_tab, build_known_scan_items_from_tab,
    delete_tab, find_tab_index_by_folder, hide_item, refresh_tab_from_scan_result, rename_tab,
    reset_tab, set_tab_folder, sort_tab, unhide_items, update_tab_layout,
};
use crate::app::tab_actions::{self, TabMoveDirection};
use crate::domain::{
    APP_ABOUT_TEXT, APP_AUTHOR_URL, APP_DISPLAY_NAME, APP_LINUX_APPLICATION_ID, APP_VERSION,
    DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, FolderScanResult, LauncherButton, LauncherTab,
    MANUAL_DEFAULT_BUTTON_COLS, MANUAL_DEFAULT_BUTTON_ROWS, MAX_BUTTON_COLS, MAX_BUTTON_ROWS,
    ScanItem, ScanSignature, TabType,
};
use crate::infra::config_store::ButtonInfo;
use crate::infra::folder_scan::{
    FolderScanOptions, ScanCancelToken, cancel_scan, new_scan_cancel_token,
    scan_folder_items_with_options,
};
use crate::ui::WindowSpec;
use crate::ui::common::{
    ABOUT_MENU_SECTIONS, BUTTON_CONTEXT_MENU_ITEMS, ButtonContextCommand, HiddenItem,
    MAIN_MENU_SECTIONS, MenuCommand, MenuCommandHandler, MenuItemSpec, VisibleButtonSlot,
    VisibleButtonSlotScratch, button_context_command_enabled, button_label,
    button_open_in_explorer_path, collect_visible_button_slots, dispatch_menu_command_if_enabled,
    folder_tab_mutation_error_message, hidden_items_for_tab, main_menu_items,
    menu_action_availability, selected_hidden_item_ids_from_indices, user_message_title,
};

const BUTTON_MIN_WIDTH: i32 = 48;
const BUTTON_MIN_HEIGHT: i32 = 36;
const BUTTON_ICON_SIZE: i32 = 20;
const EDIT_DIALOG_TEXT_LIMIT: i32 = 32_767;
const EDIT_DIALOG_WIDTH: i32 = 460;
const EDIT_DIALOG_HEIGHT: i32 = 280;
const TEXT_DIALOG_WIDTH: i32 = 420;
const TEXT_DIALOG_HEIGHT: i32 = 150;
const LAYOUT_DIALOG_WIDTH: i32 = 260;
const LAYOUT_DIALOG_HEIGHT: i32 = 160;
const HIDDEN_DIALOG_WIDTH: i32 = 360;
const HIDDEN_DIALOG_HEIGHT: i32 = 280;
const CONFIG_SAVE_POLL_MS: u64 = 150;
const SCAN_POLL_MS: u64 = 50;
const ADMIN_LAUNCH_POLL_MS: u64 = 250;
const DRAG_CLICK_SUPPRESS_RESET_MS: u64 = 150;
const LIGHT_GRID_SPACING: i32 = 6;
const DARK_GRID_SPACING: i32 = 0;
const TITLEBAR_ICON_SIZE: i32 = 12;
const FILE_MANAGER_DBUS_TIMEOUT_MS: i32 = 1_500;
const CONFIG_SAVE_COMPLETE_ACTION: &str = "config-save-complete";

pub fn run_window(spec: WindowSpec) -> Result<()> {
    let app = gtk::Application::builder()
        .application_id(APP_LINUX_APPLICATION_ID)
        .flags(gio::ApplicationFlags::empty())
        .build();
    let startup_error = Rc::new(RefCell::new(None));
    let spec_slot = Rc::new(RefCell::new(Some(spec)));
    // GTK callbacks keep weak references to the launcher state. Hold one strong
    // reference for the application run so user events can reach that state.
    let active_launcher = Rc::new(RefCell::new(None));

    {
        let startup_error = Rc::clone(&startup_error);
        let spec_slot = Rc::clone(&spec_slot);
        let active_launcher = Rc::clone(&active_launcher);
        app.connect_activate(move |application| {
            let Some(spec) = spec_slot.borrow_mut().take() else {
                return;
            };
            match GtkLauncher::open(application, spec) {
                Ok(launcher) => {
                    launcher.borrow().window.present();
                    *active_launcher.borrow_mut() = Some(launcher);
                }
                Err(error) => {
                    *startup_error.borrow_mut() = Some(error);
                    application.quit();
                }
            }
        });
    }

    app.run_with_args::<&str>(&[]);
    if let Some(error) = startup_error.borrow_mut().take() {
        Err(error)
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct GtkLauncherPlatform {
    base_dir: PathBuf,
    clipboard: Option<gtk::gdk::Clipboard>,
    admin_launch_monitor: Rc<RefCell<GtkAdminLaunchMonitor>>,
}

impl GtkLauncherPlatform {
    fn new(base_dir: PathBuf, admin_launch_monitor: Rc<RefCell<GtkAdminLaunchMonitor>>) -> Self {
        let clipboard = gtk::gdk::Display::default().map(|display| display.clipboard());
        Self {
            base_dir,
            clipboard,
            admin_launch_monitor,
        }
    }

    fn runtime_path(&self, value: &str) -> String {
        normalize_linux_runtime_path(resolve_runtime_path(value, Some(&self.base_dir)))
    }
}

impl LauncherPlatform for GtkLauncherPlatform {
    fn supports_native_admin(&self) -> bool {
        true
    }

    fn expand_path(&self, value: &str) -> String {
        expand_environment_variables(value)
    }

    fn normalize_path(&self, value: &str) -> String {
        PathBuf::from(value).to_string_lossy().into_owned()
    }

    fn is_linux(&self) -> bool {
        true
    }

    fn has_windows_path_syntax(&self, value: &str) -> bool {
        has_windows_path_syntax(value)
    }

    fn launch(&self, path: &str, params: &str) -> ActionResult<()> {
        let path = self.runtime_path(path);
        if path.trim().is_empty() {
            return Err(ActionFailure::invalid_input("program path is empty"));
        }
        let params = params.trim();
        let target = PathBuf::from(&path);

        match gtk_launch_decision(&path, &target, params)? {
            GtkLaunchDecision::OpenPathUri => return open_uri_for_path(&target),
            GtkLaunchDecision::OpenRawUri => return open_uri(&path),
            GtkLaunchDecision::SpawnCommand => {}
        }

        let args = split_command_params(params)?;
        let mut command = Command::new(&path);
        command.args(args);
        if let Some(parent) = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            command.current_dir(parent);
        }
        command
            .spawn()
            .map(|_| ())
            .map_err(|source| action_failure_from_io(source, "launch failed"))
    }

    fn run_as_admin(&self, path: &str, params: &str) -> ActionResult<AdminLaunchResult> {
        let path = self.runtime_path(path);
        let spec = linux_admin_command_spec(&path, params)?;
        let mut command = Command::new(&spec.launcher);
        command.arg(&spec.target);
        command.args(&spec.args);
        if let Some(parent) = &spec.current_dir {
            command.current_dir(parent);
        }
        let mut child = command
            .spawn()
            .map_err(|source| action_failure_from_io(source, "administrator launch failed"))?;
        let child_id = child.id() as isize;
        let executable = spec.target.to_string_lossy().into_owned();
        {
            let Ok(mut monitor) = self.admin_launch_monitor.try_borrow_mut() else {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ActionFailure::platform(
                    "administrator launch monitor is busy",
                ));
            };
            monitor.track(child, executable.clone());
        }
        Ok(AdminLaunchResult::success(child_id, executable))
    }

    fn open_in_explorer(&self, raw_path: &str) -> ActionResult<Option<ExplorerOpenFeedback>> {
        let target_path = self.runtime_path(raw_path);
        open_in_file_manager_with(&target_path, show_item_in_file_manager, open_uri_for_path)
    }

    fn copy_to_clipboard(&self, text: &str) -> ActionResult<()> {
        if text.is_empty() {
            return Err(ActionFailure::invalid_input("clipboard text is empty"));
        }
        let Some(clipboard) = &self.clipboard else {
            return Err(ActionFailure::runtime_unavailable(
                "GTK clipboard unavailable",
            ));
        };
        clipboard.set_text(&windows_compatible_clipboard_text(text));
        Ok(())
    }
}

struct GtkLauncher {
    self_ref: Weak<RefCell<GtkLauncher>>,
    application: gtk::Application,
    window: gtk::ApplicationWindow,
    notebook: gtk::Notebook,
    actions: MenuActions,
    config: ConfigService,
    launcher_actions: LauncherActionService<GtkLauncherPlatform>,
    folder_tabs: Vec<LauncherTab>,
    selected_tab_idx: usize,
    dark_theme: bool,
    suppress_switch_page: Rc<Cell<bool>>,
    visible_button_slots: Vec<VisibleButtonSlot>,
    visible_button_slot_scratch: VisibleButtonSlotScratch,
    active_scan: Option<ActiveScan>,
    admin_launch_monitor: Rc<RefCell<GtkAdminLaunchMonitor>>,
    active_context_popover: Option<gtk::Popover>,
    close_in_progress: bool,
}

impl GtkLauncher {
    fn open(application: &gtk::Application, spec: WindowSpec) -> Result<Rc<RefCell<Self>>> {
        install_css();
        let config = match spec.config_path.as_deref() {
            Some(config_path) => {
                ConfigService::open_path_from_executable_or_current_dir(config_path)?
            }
            None => ConfigService::open_from_executable_or_current_dir()?,
        };
        let geometry = parse_initial_geometry(&config.get_window_geometry_for_dpi(Some(1.0)));
        let admin_launch_monitor = Rc::new(RefCell::new(GtkAdminLaunchMonitor::default()));
        let action_platform = GtkLauncherPlatform::new(
            config.base_dir().to_path_buf(),
            Rc::clone(&admin_launch_monitor),
        );
        let folder_tabs = config.get_folder_tabs();
        let dark_theme = config.dark_theme();
        apply_gtk_theme(dark_theme);
        let window_icon_path =
            install_window_icon(spec.icon_svg_file_name, spec.icon_png_file_name);

        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title(spec.title)
            .icon_name(gtk_window_icon_name())
            .default_width(geometry.0)
            .default_height(geometry.1)
            .build();
        install_compact_titlebar(&window, spec.title, window_icon_path.as_deref());
        if let Some(icon_path) = window_icon_path {
            install_toplevel_window_icon_on_realize(&window, icon_path);
        }
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let menubar = gtk::PopoverMenuBar::from_model(Some(&build_menu_model()));
        let notebook = gtk::Notebook::new();
        notebook.set_hexpand(true);
        notebook.set_vexpand(true);
        root.append(&menubar);
        root.append(&notebook);
        window.set_child(Some(&root));

        let launcher = Rc::new(RefCell::new(Self {
            self_ref: Weak::new(),
            application: application.clone(),
            window,
            notebook,
            actions: MenuActions::empty(),
            config,
            launcher_actions: LauncherActionService::new(action_platform),
            folder_tabs,
            selected_tab_idx: 0,
            dark_theme,
            suppress_switch_page: Rc::new(Cell::new(false)),
            visible_button_slots: Vec::new(),
            visible_button_slot_scratch: VisibleButtonSlotScratch::default(),
            active_scan: None,
            admin_launch_monitor,
            active_context_popover: None,
            close_in_progress: false,
        }));

        launcher.borrow_mut().self_ref = Rc::downgrade(&launcher);
        let actions = install_actions(&launcher);
        launcher.borrow_mut().actions = actions;
        connect_window_events(&launcher);
        launcher.borrow_mut().rebuild_tabs();
        launcher.borrow_mut().update_action_state();
        start_config_save_poll(&launcher);
        start_admin_launch_poll(&launcher);
        Ok(launcher)
    }

    fn rebuild_tabs(&mut self) {
        self.close_context_popover();
        self.suppress_switch_page.set(true);
        while self.notebook.n_pages() > 0 {
            self.notebook.remove_page(Some(0));
        }

        for tab_idx in 0..self.folder_tabs.len() {
            let page = self.build_tab_page(tab_idx);
            let title = self
                .folder_tabs
                .get(tab_idx)
                .map(|tab| tab_title(tab, tab_idx))
                .unwrap_or_default();
            let label = gtk::Label::new(Some(&title));
            self.notebook.append_page(&page, Some(&label));
        }

        if self.folder_tabs.is_empty() {
            self.selected_tab_idx = 0;
        } else {
            self.selected_tab_idx = self.selected_tab_idx.min(self.folder_tabs.len() - 1);
            self.notebook
                .set_current_page(Some(self.selected_tab_idx as u32));
        }
        self.suppress_switch_page.set(false);
        self.update_action_state();
        self.focus_current_page();
    }

    fn focus_current_page(&self) {
        if let Some(page) = self.notebook.nth_page(Some(self.selected_tab_idx as u32)) {
            page.grab_focus();
        } else {
            self.notebook.grab_focus();
        }
    }

    fn build_tab_page(&mut self, tab_idx: usize) -> gtk::Widget {
        let Some(tab) = self.folder_tabs.get(tab_idx).cloned() else {
            return gtk::Box::new(gtk::Orientation::Vertical, 0).upcast();
        };
        collect_visible_button_slots(
            &tab,
            &mut self.visible_button_slots,
            &mut self.visible_button_slot_scratch,
        );

        let grid = gtk::Grid::builder()
            .column_homogeneous(true)
            .row_homogeneous(true)
            .column_spacing(launcher_grid_spacing(self.dark_theme))
            .row_spacing(launcher_grid_spacing(self.dark_theme))
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        grid.add_css_class("launcher-grid");
        let cols = i32::from(tab.cols.max(1));
        let slots = self.visible_button_slots.clone();
        let required_rows = required_grid_rows(&tab, &slots);
        let slot_count = required_rows.saturating_mul(usize::from(tab.cols.max(1)));
        let mut buttons_by_slot = vec![None; slot_count];
        for slot in slots {
            if slot.slot_idx < buttons_by_slot.len() {
                buttons_by_slot[slot.slot_idx] = Some(slot.button_idx);
            }
        }

        for (slot_idx, button_idx) in buttons_by_slot.into_iter().enumerate() {
            let row = i32::try_from(slot_idx / usize::try_from(cols).unwrap_or(1)).unwrap_or(0);
            let col = i32::try_from(slot_idx % usize::try_from(cols).unwrap_or(1)).unwrap_or(0);
            if let Some(button_idx) = button_idx
                && let Some(button) = tab.buttons.get(button_idx).cloned()
            {
                let launcher_button = build_launcher_button(
                    &button,
                    Some(self.launcher_actions.platform().base_dir.as_path()),
                );
                connect_button_actions(self, &launcher_button, tab_idx, button_idx, slot_idx);
                grid.attach(&launcher_button, col, row, 1, 1);
            } else {
                grid.attach(&build_empty_slot(), col, row, 1, 1);
            }
        }

        let viewport = gtk::Viewport::builder()
            .scroll_to_focus(launcher_grid_scroll_to_focus())
            .overflow(launcher_grid_overflow())
            .child(&grid)
            .build();
        viewport.upcast()
    }

    fn add_folder_tab(&mut self) {
        if self.active_scan.is_some() {
            self.show_info("Scan", "이미 폴더 스캔이 진행 중입니다.");
            return;
        }
        let folder_path = match self.pick_folder("Add Folder Tab") {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(message) => {
                self.show_error("Add Folder Tab", &message);
                return;
            }
        };
        if let Some(tab_idx) = find_tab_index_by_folder(&self.folder_tabs, &folder_path) {
            self.selected_tab_idx = tab_idx;
            self.rebuild_tabs();
            return;
        }
        self.start_scan(GtkScanRequest::AddFolder { folder_path });
    }

    fn add_manual_tab(&mut self) {
        if let Some((next_tabs, outcome)) =
            self.mutate_tabs_for_save(add_manual_tab, |window, error| {
                window.show_folder_mutation_error("Add Manual Tab", error);
            })
        {
            self.persist_tabs(next_tabs, outcome.focus_tab_idx);
        }
    }

    fn set_current_tab_folder(&mut self) {
        if self.active_scan.is_some() {
            self.show_info("Set Tab Folder", "이미 폴더 스캔이 진행 중입니다.");
            return;
        }
        let Some(tab) = self.current_tab() else {
            return;
        };
        if tab.tab_type == TabType::Manual {
            self.show_info("Set Tab Folder", "수동 탭은 폴더에 연결할 수 없습니다.");
            return;
        }
        let tab_id = tab.id.clone();
        let folder_path = match self.pick_folder("Set Tab Folder") {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(message) => {
                self.show_error("Set Tab Folder", &message);
                return;
            }
        };
        let duplicate_idx = find_tab_index_by_folder(&self.folder_tabs, &folder_path);
        if let Some(duplicate_idx) = duplicate_idx
            && duplicate_idx != self.selected_tab_idx
        {
            self.selected_tab_idx = duplicate_idx;
            self.rebuild_tabs();
            return;
        }
        let (known_signature, known_items) = if duplicate_idx == Some(self.selected_tab_idx) {
            self.current_tab()
                .map(|tab| known_scan_options_from_tab(tab, true))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        self.start_scan(GtkScanRequest::SetFolder {
            tab_id,
            folder_path,
            known_signature,
            known_items,
        });
    }

    fn edit_current_tab_layout(&mut self) {
        let Some(tab) = self.current_tab().cloned() else {
            return;
        };
        let defaults = if tab.tab_type == TabType::Manual {
            (MANUAL_DEFAULT_BUTTON_ROWS, MANUAL_DEFAULT_BUTTON_COLS)
        } else {
            (DEFAULT_BUTTON_ROWS, DEFAULT_BUTTON_COLS)
        };
        let Some((rows, cols)) = tab_layout_dialog(&self.window, tab.rows, tab.cols, defaults)
        else {
            return;
        };
        let selected_tab_idx = self.selected_tab_idx;
        if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
            |tabs| update_tab_layout(tabs, selected_tab_idx, rows, cols),
            |window, error| {
                window.show_folder_mutation_error("Tab Layout", error);
            },
        ) {
            self.persist_tabs(next_tabs, outcome.focus_tab_idx);
        }
    }

    fn rename_current_tab(&mut self) {
        let Some(tab) = self.current_tab().cloned() else {
            return;
        };
        let Some(title) =
            text_input_dialog(&self.window, "Rename Tab", "New tab title:", &tab.title)
        else {
            return;
        };
        let selected_tab_idx = self.selected_tab_idx;
        if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
            |tabs| rename_tab(tabs, selected_tab_idx, &title),
            |window, error| {
                window.show_folder_mutation_error("Rename Tab", error);
            },
        ) {
            self.persist_tabs(next_tabs, outcome.focus_tab_idx);
        }
    }

    fn delete_current_tab(&mut self) {
        let Some(tab) = self.current_tab() else {
            return;
        };
        let title = tab_title(tab, self.selected_tab_idx);
        if !confirm_message(
            &self.window,
            "Delete Tab",
            &format!("Delete current tab '{title}'?"),
        ) {
            return;
        }
        let selected_tab_idx = self.selected_tab_idx;
        if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
            |tabs| delete_tab(tabs, selected_tab_idx),
            |window, error| {
                window.show_folder_mutation_error("Delete Tab", error);
            },
        ) {
            self.persist_tabs(next_tabs, outcome.focus_tab_idx);
        }
    }

    fn refresh_current_tab(&mut self) {
        if self.active_scan.is_some() {
            self.show_info("Refresh", "이미 폴더 스캔이 진행 중입니다.");
            return;
        }
        let Some(tab) = self.current_tab() else {
            return;
        };
        if tab.tab_type != TabType::Folder {
            self.show_info("Refresh", "수동 탭은 폴더 refresh 대상이 아닙니다.");
            return;
        }
        if tab.folder_path.trim().is_empty() {
            self.show_info("Refresh", "현재 탭에 설정된 폴더가 없습니다.");
            return;
        }
        if let Some(message) = linux_folder_scan_guard_message(&tab.folder_path) {
            self.show_info("Refresh", &message);
            return;
        }
        self.start_scan(GtkScanRequest::Refresh {
            tab_id: tab.id.clone(),
            folder_path: tab.folder_path.clone(),
            known_signature: tab.scan_signature.clone(),
        });
    }

    fn sort_current_tab(&mut self) {
        if self.active_scan.is_some() {
            self.show_info("Sort", "이미 폴더 스캔이 진행 중입니다.");
            return;
        }
        let Some(tab) = self.current_tab() else {
            return;
        };
        let tab_id = tab.id.clone();
        if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
            |tabs| sort_tab(tabs, &tab_id),
            |window, error| {
                window.show_folder_mutation_error("Sort", error);
            },
        ) {
            match outcome {
                Some(outcome) => self.persist_tabs(next_tabs, outcome.focus_tab_idx),
                None => self.folder_tabs = next_tabs,
            }
        }
    }

    fn reset_current_tab(&mut self) {
        if self.active_scan.is_some() {
            self.show_info("Reset Tab", "이미 폴더 스캔이 진행 중입니다.");
            return;
        }
        let Some(tab) = self.current_tab() else {
            return;
        };
        if tab.tab_type != TabType::Folder {
            return;
        }
        if tab.folder_path.trim().is_empty() {
            self.show_warning("Reset Tab", "현재 탭에 설정된 폴더가 없습니다.");
            return;
        }
        if let Some(message) = linux_folder_scan_guard_message(&tab.folder_path) {
            self.show_info("Reset Tab", &message);
            return;
        }
        if !confirm_message(
            &self.window,
            "Reset Tab",
            "Reset this tab? Existing button settings will be rebuilt from folder scan.",
        ) {
            return;
        }
        let (known_signature, known_items) = known_scan_options_from_tab(tab, true);
        self.start_scan(GtkScanRequest::Reset {
            tab_id: tab.id.clone(),
            folder_path: tab.folder_path.clone(),
            known_signature,
            known_items,
        });
    }

    fn manage_hidden_items(&mut self) {
        let Some(tab) = self.current_tab().cloned() else {
            return;
        };
        if tab.tab_type != TabType::Folder {
            return;
        }
        let Some(item_ids) = hidden_items_dialog(&self.window, &tab) else {
            return;
        };
        let selected_tab_idx = self.selected_tab_idx;
        if let Some((next_tabs, changed)) = self.mutate_tabs_for_save(
            |tabs| unhide_items(tabs, selected_tab_idx, &item_ids),
            |window, error| {
                window.show_folder_mutation_error("Manage Hidden Items", error);
            },
        ) {
            if changed {
                self.persist_tabs(next_tabs, Some(selected_tab_idx));
            } else {
                self.folder_tabs = next_tabs;
            }
        }
    }

    fn move_current_tab(&mut self, direction: TabMoveDirection) {
        if self.active_scan.is_some() {
            return;
        }
        let mut next_tabs = std::mem::take(&mut self.folder_tabs);
        let outcome = tab_actions::move_tab(&mut next_tabs, self.selected_tab_idx, direction);
        if !outcome.moved {
            self.folder_tabs = next_tabs;
            return;
        }
        self.persist_tabs(next_tabs, outcome.focus_tab_idx);
    }

    fn select_relative_tab(&mut self, delta: isize) {
        if self.active_scan.is_some() || self.folder_tabs.is_empty() {
            return;
        }
        let requested = self.selected_tab_idx as isize + delta;
        if requested < 0 {
            return;
        }
        let requested = requested as usize;
        if requested >= self.folder_tabs.len() {
            return;
        }
        self.close_context_popover();
        self.selected_tab_idx = requested;
        self.notebook.set_current_page(Some(requested as u32));
        self.update_action_state();
        self.focus_current_page();
    }

    fn toggle_dark_theme(&mut self) {
        if !self.finish_config_saves_before_sync_change() {
            return;
        }
        let enabled = !self.dark_theme;
        match self.config.set_dark_theme(enabled) {
            Ok(()) => {
                self.dark_theme = enabled;
                apply_gtk_theme(enabled);
                self.rebuild_tabs();
                self.actions.dark_theme.set_state(&enabled.to_variant());
            }
            Err(error) => self.show_error("Dark Theme", &error.user_message()),
        }
    }

    fn click_button(&mut self, tab_idx: usize, button_idx: usize) {
        let Some(button) = self
            .folder_tabs
            .get(tab_idx)
            .and_then(|tab| tab.buttons.get(button_idx))
        else {
            return;
        };
        let request = self.launcher_actions.prepare_button_action(button);
        for message in &request.pre_messages {
            self.show_user_message(message);
        }
        let messages = self.launcher_actions.execute_button_action(&request);
        self.show_user_messages(&messages);
    }

    fn edit_button(&mut self, tab_idx: usize, button_idx: usize) {
        if !self.finish_config_saves_before_sync_change() {
            return;
        }
        let initial = self.config.get_button_info(tab_idx, button_idx);
        let Some(updated) = edit_button_dialog(&self.window, initial) else {
            return;
        };
        match self.config.set_button_info(tab_idx, button_idx, updated) {
            Ok(()) => {
                self.folder_tabs = self.config.get_folder_tabs();
                self.rebuild_tabs();
            }
            Err(error) => self.show_error("Edit", &error.user_message()),
        }
    }

    fn open_button_in_explorer(&self, tab_idx: usize, button_idx: usize) {
        let Some(tab) = self.folder_tabs.get(tab_idx) else {
            return;
        };
        let Some(button) = tab.buttons.get(button_idx) else {
            return;
        };
        let raw_path = button_open_in_explorer_path(button);
        let messages = self
            .launcher_actions
            .open_in_explorer(raw_path, tab.tab_type == TabType::Manual);
        self.show_user_messages(&messages);
    }

    fn hide_button(&mut self, tab_idx: usize, button_idx: usize) {
        let Some(tab) = self.folder_tabs.get(tab_idx) else {
            return;
        };
        if tab.tab_type == TabType::Manual {
            return;
        }
        let Some(button) = tab.buttons.get(button_idx) else {
            return;
        };
        let item_id = button.item_id.clone();
        if let Some((next_tabs, changed)) = self.mutate_tabs_for_save(
            |tabs| hide_item(tabs, tab_idx, &item_id),
            |window, error| {
                window.show_folder_mutation_error("Hide", error);
            },
        ) {
            if changed {
                self.persist_tabs(next_tabs, Some(tab_idx));
            } else {
                self.folder_tabs = next_tabs;
            }
        }
    }

    fn drop_button_on_button(
        &mut self,
        source_tab_idx: usize,
        source_button_idx: usize,
        source_slot_idx: usize,
        target_tab_idx: usize,
        target_button_idx: usize,
        target_slot_idx: usize,
    ) {
        if source_tab_idx != target_tab_idx || source_tab_idx != self.selected_tab_idx {
            return;
        }
        if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
            |tabs| {
                move_button_between_slots(
                    tabs,
                    ButtonSlotMove::new(
                        source_tab_idx,
                        source_button_idx,
                        source_slot_idx,
                        target_button_idx,
                        target_slot_idx,
                    ),
                )
            },
            |window, error| {
                window.show_button_move_error("Move Button", error);
            },
        ) {
            match outcome {
                Some(outcome) => self.persist_tabs(next_tabs, Some(outcome.focus_tab_idx)),
                None => self.folder_tabs = next_tabs,
            }
        }
    }

    fn start_scan(&mut self, mut request: GtkScanRequest) {
        let token = new_scan_cancel_token();
        let folder_path = request.folder_path();
        let options = request.take_options(token.clone());
        let (tx, rx) = mpsc::channel();
        let worker_request = request;
        let join = match thread::Builder::new()
            .name(String::from("folder-scan-worker"))
            .spawn(move || {
                apply_debug_scan_delay();
                let result = scan_folder_items_with_options(folder_path, options);
                let _ = tx.send(GtkScanCompleteMessage {
                    request: worker_request,
                    result,
                });
            }) {
            Ok(join) => join,
            Err(error) => {
                self.show_error(
                    "Scan",
                    &format!("폴더 스캔 worker를 시작할 수 없습니다: {error}"),
                );
                return;
            }
        };
        self.active_scan = Some(ActiveScan {
            cancel_token: token,
            receiver: rx,
            join: Some(join),
        });
        self.update_action_state();
        start_scan_poll(self);
    }

    fn poll_scan(&mut self) -> bool {
        let Some(active_scan) = self.active_scan.as_mut() else {
            return false;
        };
        let message = match active_scan.receiver.try_recv() {
            Ok(message) => message,
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.active_scan = None;
                self.update_action_state();
                self.show_error("Scan", "폴더 스캔 worker가 예기치 않게 종료되었습니다.");
                return false;
            }
        };
        if let Some(mut active_scan) = self.active_scan.take()
            && let Some(join) = active_scan.join.take()
        {
            let _ = join.join();
        }
        self.update_action_state();
        match message.result {
            Ok(result) => self.apply_scan_result(message.request, result),
            Err(error) => self.show_error("Scan", &error.user_message()),
        }
        false
    }

    fn apply_scan_result(&mut self, request: GtkScanRequest, result: FolderScanResult) {
        if result.cancelled {
            return;
        }
        match request {
            GtkScanRequest::AddFolder { folder_path } => {
                let mut next_tabs = std::mem::take(&mut self.folder_tabs);
                let signature = complete_scan_signature(&result);
                match add_folder_tab(&mut next_tabs, &folder_path, &result.items, signature) {
                    Ok(outcome) => {
                        self.persist_tabs(next_tabs, outcome.focus_tab_idx);
                        self.show_scan_warnings(&result);
                    }
                    Err(FolderTabMutationError::DuplicateFolder { tab_idx }) => {
                        self.folder_tabs = self.config.get_folder_tabs();
                        self.selected_tab_idx = tab_idx;
                        self.rebuild_tabs();
                    }
                    Err(error) => {
                        self.folder_tabs = self.config.get_folder_tabs();
                        self.show_folder_mutation_error("Add Folder Tab", error);
                    }
                }
            }
            GtkScanRequest::SetFolder {
                tab_id,
                folder_path,
                ..
            } => {
                let mut next_tabs = std::mem::take(&mut self.folder_tabs);
                let signature = complete_scan_signature(&result);
                match set_tab_folder(
                    &mut next_tabs,
                    &tab_id,
                    &folder_path,
                    &result.items,
                    signature,
                ) {
                    Ok(outcome) => {
                        self.persist_tabs(next_tabs, outcome.focus_tab_idx);
                        self.show_scan_warnings(&result);
                    }
                    Err(FolderTabMutationError::DuplicateFolder { tab_idx }) => {
                        self.folder_tabs = self.config.get_folder_tabs();
                        self.selected_tab_idx = tab_idx;
                        self.rebuild_tabs();
                    }
                    Err(error) => {
                        self.folder_tabs = self.config.get_folder_tabs();
                        self.show_folder_mutation_error("Set Tab Folder", error);
                    }
                }
            }
            GtkScanRequest::Refresh { tab_id, .. } => {
                if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
                    |tabs| refresh_tab_from_scan_result(tabs, &tab_id, &result),
                    |window, error| {
                        window.show_folder_mutation_error("Refresh", error);
                    },
                ) {
                    match outcome {
                        Some(outcome) => {
                            self.persist_tabs(next_tabs, outcome.focus_tab_idx);
                            self.show_scan_warnings(&result);
                        }
                        None => self.folder_tabs = next_tabs,
                    }
                }
            }
            GtkScanRequest::Reset { tab_id, .. } => {
                let signature = complete_scan_signature(&result);
                if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
                    |tabs| reset_tab(tabs, &tab_id, &result.items, signature),
                    |window, error| {
                        window.show_folder_mutation_error("Reset Tab", error);
                    },
                ) {
                    self.persist_tabs(next_tabs, outcome.focus_tab_idx);
                    self.show_scan_warnings(&result);
                }
            }
        }
    }

    fn show_scan_warnings(&self, result: &FolderScanResult) {
        if result.failure_count() > 0 {
            self.show_warning(
                "Scan",
                &format!(
                    "일부 항목을 읽지 못했습니다. 실패 항목 수: {}",
                    result.failure_count()
                ),
            );
        }
    }

    fn mutate_tabs_for_save<T, E>(
        &mut self,
        mutate: impl FnOnce(&mut Vec<LauncherTab>) -> std::result::Result<T, E>,
        handle_error: impl FnOnce(&mut Self, E),
    ) -> Option<(Vec<LauncherTab>, T)> {
        let mut next_tabs = std::mem::take(&mut self.folder_tabs);
        match mutate(&mut next_tabs) {
            Ok(outcome) => Some((next_tabs, outcome)),
            Err(error) => {
                self.folder_tabs = self.config.get_folder_tabs();
                handle_error(self, error);
                None
            }
        }
    }

    fn persist_tabs(&mut self, next_tabs: Vec<LauncherTab>, focus_tab_idx: Option<usize>) {
        let save_complete_window: glib::SendWeakRef<gtk::ApplicationWindow> =
            self.window.downgrade().into();
        match self.config.set_folder_tabs_deferred(next_tabs, move || {
            let save_complete_window = save_complete_window.clone();
            glib::idle_add_once(move || {
                if let Some(window) = save_complete_window.into_weak_ref().upgrade() {
                    gtk::prelude::ActionGroupExt::activate_action(
                        &window,
                        CONFIG_SAVE_COMPLETE_ACTION,
                        None::<&glib::Variant>,
                    );
                }
            });
        }) {
            Ok(_) => {
                self.folder_tabs = self.config.get_folder_tabs();
                if let Some(index) = focus_tab_idx.filter(|index| *index < self.folder_tabs.len()) {
                    self.selected_tab_idx = index;
                }
                self.rebuild_tabs();
            }
            Err(error) => {
                self.folder_tabs = self.config.get_folder_tabs();
                self.rebuild_tabs();
                self.show_error("Configuration Save", &error.user_message());
            }
        }
    }

    fn process_config_save_results(&mut self) {
        let statuses = self.config.drain_deferred_save_results();
        self.handle_config_save_statuses(statuses);
    }

    fn process_admin_launch_results(&mut self) {
        let results = {
            let Ok(mut monitor) = self.admin_launch_monitor.try_borrow_mut() else {
                return;
            };
            monitor.poll()
        };
        for result in results {
            self.show_user_message(&admin_result_user_message(&result));
        }
    }

    fn handle_config_save_statuses(
        &mut self,
        statuses: Vec<crate::app::config_service::DeferredConfigSaveStatus>,
    ) {
        for status in statuses {
            if status.success || status.superseded {
                continue;
            }
            if status.rolled_back {
                self.folder_tabs = self.config.get_folder_tabs();
                self.rebuild_tabs();
            }
            let message = status
                .user_message
                .unwrap_or_else(|| String::from("설정 저장에 실패했습니다."));
            self.show_error("Configuration Save", &message);
        }
    }

    fn finish_config_saves_before_sync_change(&mut self) -> bool {
        let statuses = self.config.finish_deferred_save_work();
        let success = statuses
            .iter()
            .all(|status| status.success || status.superseded);
        self.handle_config_save_statuses(statuses);
        success
    }

    fn close(&mut self) {
        if self.close_in_progress {
            return;
        }
        self.close_in_progress = true;
        self.close_context_popover();
        if let Some(active_scan) = self.active_scan.take() {
            cancel_scan(&active_scan.cancel_token);
        }
        self.finish_config_saves_before_sync_change();
        self.save_window_geometry_for_close();
    }

    fn save_window_geometry_for_close(&mut self) {
        let width = self.window.allocated_width().max(1);
        let height = self.window.allocated_height().max(1);
        let previous_geometry = self.config.get_window_geometry_for_dpi(Some(1.0));
        let geometry =
            gtk_close_geometry_with_preserved_position(width, height, &previous_geometry);
        if let Err(error) = self
            .config
            .save_window_geometry_with_dpi(geometry, Some(1.0))
        {
            self.show_user_message(&UserMessage::new(
                close_geometry_save_failure_level(),
                "Configuration Save",
                error.user_message(),
            ));
        }
    }

    fn close_context_popover(&mut self) {
        if let Some(popover) = self.active_context_popover.take() {
            popover.popdown();
            if popover.parent().is_some() {
                popover.unparent();
            }
        }
    }

    fn clear_active_context_popover_if_matches(&mut self, popover: &gtk::Popover) {
        let is_active = self
            .active_context_popover
            .as_ref()
            .is_some_and(|active| active.as_ptr() == popover.as_ptr());
        if is_active {
            self.active_context_popover = None;
        }
    }

    fn update_action_state(&self) {
        let availability = self.current_menu_availability();
        for item in MAIN_MENU_SECTIONS.iter().flat_map(|section| section.iter()) {
            self.actions
                .action_for_command(item.command)
                .set_enabled(availability.is_command_enabled(item.command));
        }
        self.actions
            .dark_theme
            .set_state(&self.dark_theme.to_variant());
    }

    fn current_menu_availability(&self) -> crate::ui::common::MenuActionAvailability {
        menu_action_availability(
            &self.folder_tabs,
            self.selected_tab_idx,
            self.active_scan.is_none(),
        )
    }

    fn pick_folder(&self, title: &str) -> std::result::Result<Option<String>, String> {
        if let Some(result) = debug_folder_picker_override() {
            return result;
        }
        let dialog = gtk::FileChooserNative::new(
            Some(title),
            Some(&self.window),
            gtk::FileChooserAction::SelectFolder,
            Some("Select"),
            Some("Cancel"),
        );
        dialog.set_modal(true);
        let response = glib::MainContext::default().block_on(dialog.run_future());
        let selected_path = dialog.file().and_then(|file| file.path());
        dialog.destroy();
        self.window.present();
        selected_folder_from_chooser_response(response, selected_path)
    }

    fn current_tab(&self) -> Option<&LauncherTab> {
        self.folder_tabs.get(self.selected_tab_idx)
    }

    fn show_user_messages(&self, messages: &[UserMessage]) {
        for message in messages {
            self.show_user_message(message);
        }
    }

    fn show_user_message(&self, message: &UserMessage) {
        let title = user_message_title(message);
        match message.level.as_str() {
            "error" => self.show_error(title, &message.message),
            "warning" => self.show_warning(title, &message.message),
            _ => self.show_info(title, &message.message),
        }
    }

    fn show_info(&self, title: &str, message: &str) {
        show_message(&self.window, gtk::MessageType::Info, title, message);
    }

    fn show_warning(&self, title: &str, message: &str) {
        show_message(&self.window, gtk::MessageType::Warning, title, message);
    }

    fn show_error(&self, title: &str, message: &str) {
        show_message(&self.window, gtk::MessageType::Error, title, message);
    }

    fn show_about(&self) {
        about_dialog(&self.window);
    }

    fn show_folder_mutation_error(&self, title: &str, error: FolderTabMutationError) {
        self.show_warning(title, folder_tab_mutation_error_message(error));
    }

    fn show_button_move_error(&self, title: &str, error: ButtonSlotMoveError) {
        let message = match error {
            ButtonSlotMoveError::InvalidTabIndex => "대상 탭을 찾을 수 없습니다.",
            ButtonSlotMoveError::InvalidButtonIndex => "대상 버튼을 찾을 수 없습니다.",
            ButtonSlotMoveError::InvalidSlotIndex => "대상 버튼 위치가 올바르지 않습니다.",
        };
        self.show_warning(title, message);
    }
}

fn selected_folder_from_chooser_response(
    response: gtk::ResponseType,
    selected_path: Option<PathBuf>,
) -> std::result::Result<Option<String>, String> {
    if response != gtk::ResponseType::Accept {
        return Ok(None);
    }
    selected_path
        .map(|path| path.to_string_lossy().into_owned())
        .map(Some)
        .ok_or_else(|| String::from("선택한 폴더의 로컬 경로를 확인할 수 없습니다."))
}

fn selected_file_from_chooser_response(
    response: gtk::ResponseType,
    selected_path: Option<PathBuf>,
) -> std::result::Result<Option<String>, String> {
    if response != gtk::ResponseType::Accept {
        return Ok(None);
    }
    selected_path
        .map(|path| path.to_string_lossy().into_owned())
        .map(Some)
        .ok_or_else(|| String::from("선택한 파일의 로컬 경로를 확인할 수 없습니다."))
}

#[cfg(debug_assertions)]
fn debug_folder_picker_override() -> Option<std::result::Result<Option<String>, String>> {
    debug_path_picker_override_from_env(
        std::env::var_os("J3LAUNCHER_TEST_PICK_FOLDER"),
        std::env::var_os("J3LAUNCHER_TEST_PICK_FOLDER_ERROR"),
    )
}

#[cfg(not(debug_assertions))]
fn debug_folder_picker_override() -> Option<std::result::Result<Option<String>, String>> {
    None
}

#[cfg(test)]
fn debug_folder_picker_override_from_env(
    selected_path: Option<std::ffi::OsString>,
    error_message: Option<std::ffi::OsString>,
) -> Option<std::result::Result<Option<String>, String>> {
    debug_path_picker_override_from_env(selected_path, error_message)
}

#[cfg(debug_assertions)]
fn debug_file_picker_override() -> Option<std::result::Result<Option<String>, String>> {
    debug_path_picker_override_from_env(
        std::env::var_os("J3LAUNCHER_TEST_PICK_FILE"),
        std::env::var_os("J3LAUNCHER_TEST_PICK_FILE_ERROR"),
    )
}

#[cfg(not(debug_assertions))]
fn debug_file_picker_override() -> Option<std::result::Result<Option<String>, String>> {
    None
}

#[cfg(test)]
fn debug_file_picker_override_from_env(
    selected_path: Option<std::ffi::OsString>,
    error_message: Option<std::ffi::OsString>,
) -> Option<std::result::Result<Option<String>, String>> {
    debug_path_picker_override_from_env(selected_path, error_message)
}

#[cfg(any(debug_assertions, test))]
fn debug_path_picker_override_from_env(
    selected_path: Option<std::ffi::OsString>,
    error_message: Option<std::ffi::OsString>,
) -> Option<std::result::Result<Option<String>, String>> {
    if let Some(message) = error_message {
        return Some(Err(message.to_string_lossy().into_owned()));
    }
    let path = selected_path?;
    let path = path.to_string_lossy().into_owned();
    if path == "__CANCEL__" {
        Some(Ok(None))
    } else {
        Some(Ok(Some(path)))
    }
}

impl MenuCommandHandler for GtkLauncher {
    fn add_folder_tab(&mut self) {
        GtkLauncher::add_folder_tab(self);
    }

    fn add_manual_tab(&mut self) {
        GtkLauncher::add_manual_tab(self);
    }

    fn set_current_tab_folder(&mut self) {
        GtkLauncher::set_current_tab_folder(self);
    }

    fn edit_current_tab_layout(&mut self) {
        GtkLauncher::edit_current_tab_layout(self);
    }

    fn rename_current_tab(&mut self) {
        GtkLauncher::rename_current_tab(self);
    }

    fn delete_current_tab(&mut self) {
        GtkLauncher::delete_current_tab(self);
    }

    fn move_current_tab_left(&mut self) {
        self.move_current_tab(TabMoveDirection::Left);
    }

    fn move_current_tab_right(&mut self) {
        self.move_current_tab(TabMoveDirection::Right);
    }

    fn select_previous_tab(&mut self) {
        self.select_relative_tab(-1);
    }

    fn select_next_tab(&mut self) {
        self.select_relative_tab(1);
    }

    fn sort_current_tab(&mut self) {
        GtkLauncher::sort_current_tab(self);
    }

    fn refresh_current_tab(&mut self) {
        GtkLauncher::refresh_current_tab(self);
    }

    fn reset_current_tab(&mut self) {
        GtkLauncher::reset_current_tab(self);
    }

    fn manage_hidden_items(&mut self) {
        GtkLauncher::manage_hidden_items(self);
    }

    fn toggle_dark_theme(&mut self) {
        GtkLauncher::toggle_dark_theme(self);
    }

    fn exit(&mut self) {
        request_gtk_window_close(&self.window);
    }

    fn show_about(&mut self) {
        GtkLauncher::show_about(self);
    }
}

#[derive(Clone)]
struct MenuActions {
    add_folder_tab: gio::SimpleAction,
    add_manual_tab: gio::SimpleAction,
    set_tab_folder: gio::SimpleAction,
    tab_layout: gio::SimpleAction,
    rename_tab: gio::SimpleAction,
    delete_tab: gio::SimpleAction,
    move_left: gio::SimpleAction,
    move_right: gio::SimpleAction,
    select_prev: gio::SimpleAction,
    select_next: gio::SimpleAction,
    sort: gio::SimpleAction,
    refresh: gio::SimpleAction,
    reset: gio::SimpleAction,
    manage_hidden: gio::SimpleAction,
    dark_theme: gio::SimpleAction,
    exit: gio::SimpleAction,
    about: gio::SimpleAction,
}

impl MenuActions {
    fn empty() -> Self {
        fn action(name: &str) -> gio::SimpleAction {
            gio::SimpleAction::new(name, None)
        }
        Self {
            add_folder_tab: action("add-folder-tab"),
            add_manual_tab: action("add-manual-tab"),
            set_tab_folder: action("set-tab-folder"),
            tab_layout: action("tab-layout"),
            rename_tab: action("rename-tab"),
            delete_tab: action("delete-tab"),
            move_left: action("move-left"),
            move_right: action("move-right"),
            select_prev: action("select-prev"),
            select_next: action("select-next"),
            sort: action("sort"),
            refresh: action("refresh"),
            reset: action("reset"),
            manage_hidden: action("manage-hidden"),
            dark_theme: gio::SimpleAction::new_stateful("dark-theme", None, &false.to_variant()),
            exit: action("exit"),
            about: action("about"),
        }
    }

    fn action_for_command(&self, command: MenuCommand) -> &gio::SimpleAction {
        match command {
            MenuCommand::AddFolderTab => &self.add_folder_tab,
            MenuCommand::AddManualTab => &self.add_manual_tab,
            MenuCommand::SetTabFolder => &self.set_tab_folder,
            MenuCommand::TabLayout => &self.tab_layout,
            MenuCommand::RenameTab => &self.rename_tab,
            MenuCommand::DeleteTab => &self.delete_tab,
            MenuCommand::MoveLeft => &self.move_left,
            MenuCommand::MoveRight => &self.move_right,
            MenuCommand::SelectPrev => &self.select_prev,
            MenuCommand::SelectNext => &self.select_next,
            MenuCommand::Sort => &self.sort,
            MenuCommand::Refresh => &self.refresh,
            MenuCommand::Reset => &self.reset,
            MenuCommand::ManageHidden => &self.manage_hidden,
            MenuCommand::DarkTheme => &self.dark_theme,
            MenuCommand::Exit => &self.exit,
            MenuCommand::About => &self.about,
        }
    }
}

struct ActiveScan {
    cancel_token: ScanCancelToken,
    receiver: mpsc::Receiver<GtkScanCompleteMessage>,
    join: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct GtkAdminLaunchMonitor {
    processes: Vec<GtkAdminLaunchProcess>,
}

impl GtkAdminLaunchMonitor {
    fn track(&mut self, child: Child, executable: String) {
        self.processes
            .push(GtkAdminLaunchProcess { child, executable });
    }

    fn poll(&mut self) -> Vec<AdminLaunchResult> {
        let mut results = Vec::new();
        let mut index = 0;
        while index < self.processes.len() {
            match self.processes[index].child.try_wait() {
                Ok(Some(status)) => {
                    let process = self.processes.swap_remove(index);
                    if let Some(result) =
                        linux_admin_result_from_exit_status(status, &process.executable)
                    {
                        results.push(result);
                    }
                }
                Ok(None) => index += 1,
                Err(error) => {
                    let process = self.processes.swap_remove(index);
                    results.push(AdminLaunchResult::exception(
                        format!("pkexec wait failed: {error}"),
                        process.executable,
                    ));
                }
            }
        }
        results
    }
}

struct GtkAdminLaunchProcess {
    child: Child,
    executable: String,
}

#[derive(Debug)]
enum GtkScanRequest {
    AddFolder {
        folder_path: String,
    },
    SetFolder {
        tab_id: String,
        folder_path: String,
        known_signature: Option<ScanSignature>,
        known_items: Option<Vec<ScanItem>>,
    },
    Refresh {
        tab_id: String,
        folder_path: String,
        known_signature: Option<ScanSignature>,
    },
    Reset {
        tab_id: String,
        folder_path: String,
        known_signature: Option<ScanSignature>,
        known_items: Option<Vec<ScanItem>>,
    },
}

impl GtkScanRequest {
    fn folder_path(&self) -> PathBuf {
        match self {
            Self::AddFolder { folder_path }
            | Self::SetFolder { folder_path, .. }
            | Self::Refresh { folder_path, .. }
            | Self::Reset { folder_path, .. } => PathBuf::from(normalize_linux_runtime_path(
                expand_environment_variables(folder_path),
            )),
        }
    }

    fn take_options(&mut self, cancel_token: ScanCancelToken) -> FolderScanOptions {
        match self {
            Self::AddFolder { .. } => FolderScanOptions {
                cancel_token: Some(cancel_token),
                known_signature: None,
                known_items: None,
                allow_signature_only_unchanged: false,
            },
            Self::SetFolder {
                known_signature,
                known_items,
                ..
            }
            | Self::Reset {
                known_signature,
                known_items,
                ..
            } => FolderScanOptions {
                cancel_token: Some(cancel_token),
                known_signature: known_signature.take(),
                known_items: known_items.take(),
                allow_signature_only_unchanged: false,
            },
            Self::Refresh {
                known_signature, ..
            } => FolderScanOptions {
                cancel_token: Some(cancel_token),
                known_signature: known_signature.take(),
                known_items: None,
                allow_signature_only_unchanged: true,
            },
        }
    }
}

struct GtkScanCompleteMessage {
    request: GtkScanRequest,
    result: Result<FolderScanResult>,
}

fn install_actions(launcher: &Rc<RefCell<GtkLauncher>>) -> MenuActions {
    let actions = MenuActions::empty();
    let window = launcher.borrow().window.clone();

    for item in main_menu_items() {
        let action = actions.action_for_command(item.command);
        connect_action(action, item.command, launcher);
        window.add_action(action);
    }
    install_internal_actions(launcher);

    let app = launcher.borrow().application.clone();
    for (action, accels) in gtk_accelerator_bindings_from_menu_spec() {
        app.set_accels_for_action(action, accels);
    }
    actions
}

fn gtk_accelerator_bindings_from_menu_spec() -> Vec<(&'static str, &'static [&'static str])> {
    MAIN_MENU_SECTIONS
        .iter()
        .flat_map(|section| section.iter())
        .filter(|item| !item.gtk_accels.is_empty())
        .map(|item| (item.gtk_action, item.gtk_accels))
        .collect()
}

fn install_internal_actions(launcher: &Rc<RefCell<GtkLauncher>>) {
    let save_complete = gio::SimpleAction::new(CONFIG_SAVE_COMPLETE_ACTION, None);
    let weak = Rc::downgrade(launcher);
    save_complete.connect_activate(move |_, _| {
        if let Some(launcher) = weak.upgrade() {
            let _ = try_with_launcher_mut(&launcher, GtkLauncher::process_config_save_results);
        }
    });
    launcher.borrow().window.add_action(&save_complete);
}

fn try_with_launcher_mut<T>(
    launcher: &Rc<RefCell<GtkLauncher>>,
    action: impl FnOnce(&mut GtkLauncher) -> T,
) -> Option<T> {
    launcher
        .try_borrow_mut()
        .ok()
        .map(|mut launcher| action(&mut launcher))
}

fn connect_action(
    action: &gio::SimpleAction,
    command: MenuCommand,
    launcher: &Rc<RefCell<GtkLauncher>>,
) {
    let weak = Rc::downgrade(launcher);
    action.connect_activate(move |_, _| {
        dispatch_gtk_menu_command_when_available(weak.clone(), command);
    });
}

fn dispatch_gtk_menu_command_when_available(
    weak: Weak<RefCell<GtkLauncher>>,
    command: MenuCommand,
) {
    let Some(launcher) = weak.upgrade() else {
        return;
    };
    if try_with_launcher_mut(&launcher, |launcher| {
        launcher.close_context_popover();
        let availability = launcher.current_menu_availability();
        dispatch_menu_command_if_enabled(command, &availability, launcher);
    })
    .is_none()
    {
        glib::idle_add_local_once(move || {
            dispatch_gtk_menu_command_when_available(weak, command);
        });
    }
}

fn connect_window_events(launcher: &Rc<RefCell<GtkLauncher>>) {
    let notebook = launcher.borrow().notebook.clone();
    let suppress_switch_page = launcher.borrow().suppress_switch_page.clone();
    let weak = Rc::downgrade(launcher);
    notebook.connect_switch_page(move |_, _, page_num| {
        if suppress_switch_page.get() {
            return;
        }
        if let Some(launcher) = weak.upgrade() {
            let _ = try_with_launcher_mut(&launcher, |launcher| {
                launcher.close_context_popover();
                launcher.selected_tab_idx = page_num as usize;
                launcher.update_action_state();
                launcher.focus_current_page();
            });
        }
    });

    let window = launcher.borrow().window.clone();
    let weak = Rc::downgrade(launcher);
    window.connect_close_request(move |_| {
        if let Some(launcher) = weak.upgrade()
            && try_with_launcher_mut(&launcher, GtkLauncher::close).is_none()
        {
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });

    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(launcher);
    key_controller.connect_key_pressed(move |_, key, _, state| {
        let command = gtk_accelerator_command_for_key(key, state);
        let Some(command) = command else {
            return glib::Propagation::Proceed;
        };
        dispatch_gtk_menu_command_when_available(weak.clone(), command);
        glib::Propagation::Stop
    });
    window.add_controller(key_controller);
}

fn gtk_accelerator_command_for_key(
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> Option<MenuCommand> {
    let control = gtk::gdk::ModifierType::CONTROL_MASK;
    let shift = gtk::gdk::ModifierType::SHIFT_MASK;
    let alt = gtk::gdk::ModifierType::ALT_MASK;
    let relevant = state & (control | shift | alt);
    let key_name = key.name();
    match key_name.as_deref() {
        Some("Left") if relevant == (control | shift) => Some(MenuCommand::MoveLeft),
        Some("Right") if relevant == (control | shift) => Some(MenuCommand::MoveRight),
        Some("Page_Up" | "Prior") if relevant == control => Some(MenuCommand::SelectPrev),
        Some("Page_Down" | "Next") if relevant == control => Some(MenuCommand::SelectNext),
        Some("F5") if relevant.is_empty() => Some(MenuCommand::Sort),
        _ => None,
    }
}

fn build_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    let file = gio::Menu::new();
    let about = gio::Menu::new();

    for section in MAIN_MENU_SECTIONS {
        append_menu_section(&file, section);
    }
    for section in ABOUT_MENU_SECTIONS {
        append_menu_section(&about, section);
    }

    menu.append_submenu(Some("File"), &file);
    menu.append_submenu(Some("About"), &about);
    menu
}

fn append_menu_section(parent: &gio::Menu, items: &[MenuItemSpec]) {
    let section = gio::Menu::new();
    for item in items {
        section.append_item(&gtk_menu_item(item));
    }
    parent.append_section(None, &section);
}

fn gtk_menu_item(spec: &MenuItemSpec) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(spec.label), Some(spec.gtk_action));
    if let Some(accel) = spec.gtk_accels.first() {
        item.set_attribute_value("accel", Some(&glib::Variant::from(*accel)));
    }
    item
}

fn connect_button_actions(
    launcher: &GtkLauncher,
    button_widget: &gtk::Button,
    tab_idx: usize,
    button_idx: usize,
    slot_idx: usize,
) {
    let drag_click_guard = Rc::new(DragClickGuard::default());

    let weak = launcher.self_ref.clone();
    let click_guard = Rc::clone(&drag_click_guard);
    button_widget.connect_clicked(move |_| {
        if click_guard.should_suppress_click() {
            return;
        }
        if let Some(launcher) = weak.upgrade() {
            let _ = try_with_launcher_mut(&launcher, |launcher| {
                launcher.click_button(tab_idx, button_idx);
            });
        }
    });

    let source_payload = format!("{tab_idx}:{button_idx}:{slot_idx}");
    let drag_source = gtk::DragSource::builder()
        .actions(gtk::gdk::DragAction::MOVE)
        .build();
    let click_guard = Rc::clone(&drag_click_guard);
    drag_source.connect_drag_begin(move |_, _| {
        click_guard.mark_drag_started();
    });
    let click_guard = Rc::clone(&drag_click_guard);
    drag_source.connect_drag_end(move |_, _, _| {
        schedule_drag_click_guard_clear(&click_guard);
    });
    let click_guard = Rc::clone(&drag_click_guard);
    drag_source.connect_drag_cancel(move |_, _, _| {
        schedule_drag_click_guard_clear(&click_guard);
        false
    });
    let weak = launcher.self_ref.clone();
    drag_source.connect_prepare(move |_, _, _| {
        let launcher = weak.upgrade()?;
        let launcher = launcher.try_borrow().ok()?;
        if !can_accept_button_drag(launcher.active_scan.is_some(), launcher.close_in_progress) {
            return None;
        }
        Some(gtk::gdk::ContentProvider::for_value(
            &source_payload.to_value(),
        ))
    });
    button_widget.add_controller(drag_source);

    let weak = launcher.self_ref.clone();
    let drop_target = gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::MOVE);
    drop_target.connect_drop(move |_, value, _, _| {
        let Ok(payload) = value.get::<String>() else {
            return false;
        };
        let Some((source_tab, source_button, source_slot)) = parse_drag_payload(&payload) else {
            return false;
        };
        if let Some(launcher) = weak.upgrade() {
            return try_with_launcher_mut(&launcher, |launcher| {
                if !can_accept_button_drag(
                    launcher.active_scan.is_some(),
                    launcher.close_in_progress,
                ) {
                    return false;
                }
                launcher.drop_button_on_button(
                    source_tab,
                    source_button,
                    source_slot,
                    tab_idx,
                    button_idx,
                    slot_idx,
                );
                true
            })
            .unwrap_or(false);
        }
        false
    });
    button_widget.add_controller(drop_target);

    let weak = launcher.self_ref.clone();
    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let button_for_popover = button_widget.clone();
    gesture.connect_pressed(move |_, _, x, y| {
        if let Some(launcher) = weak.upgrade() {
            let pointing_to = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            show_button_context_menu(
                &launcher,
                &button_for_popover,
                tab_idx,
                button_idx,
                Some(pointing_to),
                false,
            );
        }
    });
    button_widget.add_controller(gesture);

    let weak = launcher.self_ref.clone();
    let key_controller = gtk::EventControllerKey::new();
    let button_for_popover = button_widget.clone();
    key_controller.connect_key_pressed(move |_, key, _, state| {
        let key_name = key.name();
        let is_context_key = key_name.as_deref() == Some("Menu");
        let is_shift_f10 = key_name.as_deref() == Some("F10")
            && state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if !is_context_key && !is_shift_f10 {
            return glib::Propagation::Proceed;
        }

        if let Some(launcher) = weak.upgrade() {
            show_button_context_menu(
                &launcher,
                &button_for_popover,
                tab_idx,
                button_idx,
                Some(keyboard_context_menu_rect(
                    button_for_popover.allocated_width(),
                    button_for_popover.allocated_height(),
                )),
                true,
            );
        }
        glib::Propagation::Stop
    });
    button_widget.add_controller(key_controller);
}

fn start_config_save_poll(launcher: &Rc<RefCell<GtkLauncher>>) {
    let weak = Rc::downgrade(launcher);
    glib::timeout_add_local(Duration::from_millis(CONFIG_SAVE_POLL_MS), move || {
        let Some(launcher) = weak.upgrade() else {
            return ControlFlow::Break;
        };
        let _ = try_with_launcher_mut(&launcher, GtkLauncher::process_config_save_results);
        ControlFlow::Continue
    });
}

fn start_admin_launch_poll(launcher: &Rc<RefCell<GtkLauncher>>) {
    let weak = Rc::downgrade(launcher);
    glib::timeout_add_local(Duration::from_millis(ADMIN_LAUNCH_POLL_MS), move || {
        let Some(launcher) = weak.upgrade() else {
            return ControlFlow::Break;
        };
        let _ = try_with_launcher_mut(&launcher, GtkLauncher::process_admin_launch_results);
        ControlFlow::Continue
    });
}

fn start_scan_poll(launcher: &GtkLauncher) {
    let weak = launcher.self_ref.clone();
    glib::timeout_add_local(Duration::from_millis(SCAN_POLL_MS), move || {
        let Some(launcher) = weak.upgrade() else {
            return ControlFlow::Break;
        };
        let Some(keep_polling) = try_with_launcher_mut(&launcher, GtkLauncher::poll_scan) else {
            return ControlFlow::Continue;
        };
        if keep_polling {
            ControlFlow::Continue
        } else {
            ControlFlow::Break
        }
    });
}

fn build_launcher_button(button: &LauncherButton, base_dir: Option<&Path>) -> gtk::Button {
    let label_text = button_label(button);
    let button_widget = gtk::Button::new();
    button_widget.set_hexpand(true);
    button_widget.set_vexpand(true);
    button_widget.set_width_request(BUTTON_MIN_WIDTH);
    button_widget.set_height_request(BUTTON_MIN_HEIGHT);
    button_widget.add_css_class("launcher-button");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 3);
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::Center);
    if let Some(icon_name) = button_icon_name(button, base_dir) {
        let image = gtk::Image::from_icon_name(icon_name);
        image.set_pixel_size(BUTTON_ICON_SIZE);
        content.append(&image);
    }
    let label = gtk::Label::new(Some(&label_text));
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_lines(2);
    label.set_justify(gtk::Justification::Center);
    label.set_xalign(0.5);
    content.append(&label);
    button_widget.set_child(Some(&content));
    button_widget
}

fn build_empty_slot() -> gtk::Box {
    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    placeholder.set_hexpand(true);
    placeholder.set_vexpand(true);
    placeholder.set_width_request(BUTTON_MIN_WIDTH);
    placeholder.set_height_request(BUTTON_MIN_HEIGHT);
    placeholder.add_css_class("launcher-empty-slot");
    placeholder
}

fn required_grid_rows(tab: &LauncherTab, slots: &[VisibleButtonSlot]) -> usize {
    let cols = usize::from(tab.cols.max(1));
    let max_slot = slots.iter().map(|slot| slot.slot_idx).max();
    let required_slots = max_slot
        .map(|slot| slot.saturating_add(1))
        .unwrap_or(0)
        .max(slots.len());
    let configured_slots = usize::from(tab.rows.max(1)).saturating_mul(cols);
    configured_slots.max(required_slots).div_ceil(cols).max(1)
}

fn can_accept_button_drag(active_scan: bool, close_in_progress: bool) -> bool {
    !active_scan && !close_in_progress
}

#[derive(Debug, Default)]
struct DragClickGuard {
    suppress_next_click: Cell<bool>,
}

impl DragClickGuard {
    fn mark_drag_started(&self) {
        self.suppress_next_click.set(true);
    }

    fn should_suppress_click(&self) -> bool {
        self.suppress_next_click.replace(false)
    }

    fn clear(&self) {
        self.suppress_next_click.set(false);
    }
}

fn schedule_drag_click_guard_clear(guard: &Rc<DragClickGuard>) {
    let guard = Rc::clone(guard);
    glib::timeout_add_local_once(
        Duration::from_millis(DRAG_CLICK_SUPPRESS_RESET_MS),
        move || {
            guard.clear();
        },
    );
}

fn indexed_context_command_enabled(
    tabs: &[LauncherTab],
    tab_idx: usize,
    button_idx: usize,
    command: ButtonContextCommand,
) -> bool {
    tabs.get(tab_idx).is_some_and(|tab| {
        tab.buttons
            .get(button_idx)
            .is_some_and(|button| button_context_command_enabled(command, tab, button))
    })
}

fn indexed_context_target_exists(tabs: &[LauncherTab], tab_idx: usize, button_idx: usize) -> bool {
    tabs.get(tab_idx)
        .and_then(|tab| tab.buttons.get(button_idx))
        .is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextMenuFocusMove {
    Previous,
    Next,
    First,
    Last,
}

fn context_menu_focus_move_for_key(key: gtk::gdk::Key) -> Option<ContextMenuFocusMove> {
    let key_name = key.name();
    match key_name.as_deref() {
        Some("Up") => Some(ContextMenuFocusMove::Previous),
        Some("Down") => Some(ContextMenuFocusMove::Next),
        Some("Home") => Some(ContextMenuFocusMove::First),
        Some("End") => Some(ContextMenuFocusMove::Last),
        _ => None,
    }
}

fn context_menu_activation_key(key: gtk::gdk::Key) -> bool {
    matches!(key.name().as_deref(), Some("Return" | "KP_Enter" | "space"))
}

fn context_menu_activation_target(
    enabled_items: &[bool],
    current_index: Option<usize>,
) -> Option<usize> {
    let index = current_index?;
    enabled_items
        .get(index)
        .copied()
        .filter(|enabled| *enabled)
        .map(|_| index)
}

fn context_menu_focus_target(
    enabled_items: &[bool],
    current_index: Option<usize>,
    movement: ContextMenuFocusMove,
) -> Option<usize> {
    if enabled_items.is_empty() {
        return None;
    }

    match movement {
        ContextMenuFocusMove::First => enabled_items.iter().position(|enabled| *enabled),
        ContextMenuFocusMove::Last => enabled_items.iter().rposition(|enabled| *enabled),
        ContextMenuFocusMove::Next => {
            let start = current_index
                .filter(|index| *index < enabled_items.len())
                .unwrap_or(enabled_items.len().saturating_sub(1));
            (1..=enabled_items.len())
                .map(|offset| (start + offset) % enabled_items.len())
                .find(|index| enabled_items[*index])
        }
        ContextMenuFocusMove::Previous => {
            let start = current_index
                .filter(|index| *index < enabled_items.len())
                .unwrap_or(0);
            (1..=enabled_items.len())
                .map(|offset| (start + enabled_items.len() - offset) % enabled_items.len())
                .find(|index| enabled_items[*index])
        }
    }
}

fn connect_context_menu_keyboard_navigation(
    popover: &gtk::Popover,
    item_buttons: Vec<gtk::Button>,
    enabled_items: Vec<bool>,
) {
    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let popover_for_key = popover.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key.name().as_deref() == Some("Escape") {
            popover_for_key.popdown();
            return glib::Propagation::Stop;
        }

        let current_index = item_buttons
            .iter()
            .position(|button| button.has_focus() || button.is_focus());
        if context_menu_activation_key(key) {
            if let Some(index) = context_menu_activation_target(&enabled_items, current_index)
                && let Some(button) = item_buttons.get(index)
            {
                button.activate();
            }
            return glib::Propagation::Stop;
        }

        let Some(movement) = context_menu_focus_move_for_key(key) else {
            return glib::Propagation::Proceed;
        };
        if let Some(index) = context_menu_focus_target(&enabled_items, current_index, movement)
            && let Some(button) = item_buttons.get(index)
        {
            button.grab_focus();
        }
        glib::Propagation::Stop
    });
    popover.add_controller(key_controller);
}

fn show_button_context_menu(
    launcher: &Rc<RefCell<GtkLauncher>>,
    parent: &gtk::Button,
    tab_idx: usize,
    button_idx: usize,
    pointing_to: Option<gtk::gdk::Rectangle>,
    focus_first_item: bool,
) {
    let target_exists = launcher.try_borrow().ok().is_some_and(|launcher| {
        indexed_context_target_exists(&launcher.folder_tabs, tab_idx, button_idx)
    });
    let _ = try_with_launcher_mut(launcher, GtkLauncher::close_context_popover);
    if !target_exists {
        return;
    }

    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_pointing_to(pointing_to.as_ref());
    let weak = Rc::downgrade(launcher);
    popover.connect_closed(move |popover| {
        if popover.parent().is_some() {
            popover.unparent();
        }
        clear_context_popover_when_available(weak.clone(), popover.clone());
    });
    let box_widget = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let mut first_item = None;
    let mut item_buttons = Vec::new();
    let mut enabled_items = Vec::new();
    for item in BUTTON_CONTEXT_MENU_ITEMS {
        let item_button = gtk::Button::with_label(item.label);
        let enabled = launcher.try_borrow().ok().is_some_and(|launcher| {
            indexed_context_command_enabled(
                &launcher.folder_tabs,
                tab_idx,
                button_idx,
                item.command,
            )
        });
        item_button.set_sensitive(enabled);
        connect_context_menu_item(
            &item_button,
            item.command,
            launcher,
            &popover,
            tab_idx,
            button_idx,
        );
        if enabled && first_item.is_none() {
            first_item = Some(item_button.clone());
        }
        item_buttons.push(item_button.clone());
        enabled_items.push(enabled);
        box_widget.append(&item_button);
    }

    connect_context_menu_keyboard_navigation(&popover, item_buttons, enabled_items);
    popover.set_child(Some(&box_widget));
    popover.popup();
    let _ = try_with_launcher_mut(launcher, |launcher| {
        launcher.active_context_popover = Some(popover.clone());
    });
    if focus_first_item && let Some(first_item) = first_item {
        first_item.grab_focus();
    }
}

fn request_gtk_window_close(window: &gtk::ApplicationWindow) {
    let window = window.clone();
    glib::idle_add_local_once(move || {
        window.close();
    });
}

fn about_dialog(parent: &gtk::ApplicationWindow) {
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .modal(true)
        .title(format!("About {APP_DISPLAY_NAME}"))
        .default_width(450)
        .default_height(350)
        .build();
    dialog.add_button("Close", gtk::ResponseType::Close);

    let layout = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(18)
        .margin_end(18)
        .build();
    let version = gtk::Label::new(Some(&format!("Version {APP_VERSION}")));
    version.set_xalign(0.0);
    let link = gtk::LinkButton::with_label(APP_AUTHOR_URL, APP_AUTHOR_URL);
    link.set_halign(gtk::Align::Start);
    let dialog_for_error = dialog.clone();
    link.connect_activate_link(move |_| {
        if let Err(error) = open_uri(APP_AUTHOR_URL) {
            show_message(
                &dialog_for_error,
                gtk::MessageType::Warning,
                "About",
                &format!("브라우저에서 링크를 열 수 없습니다:\n{error}"),
            );
        }
        glib::Propagation::Stop
    });
    let licenses_view = gtk::TextView::new();
    licenses_view.set_editable(false);
    licenses_view.set_cursor_visible(false);
    licenses_view.set_monospace(true);
    licenses_view.set_wrap_mode(gtk::WrapMode::WordChar);
    licenses_view.buffer().set_text(APP_ABOUT_TEXT);
    let licenses_scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_height(160)
        .child(&licenses_view)
        .build();

    layout.append(&version);
    layout.append(&link);
    layout.append(&licenses_scrolled);
    dialog.content_area().append(&layout);
    dialog.set_default_response(gtk::ResponseType::Close);

    let _ = glib::MainContext::default().block_on(dialog.run_future());
    dialog.close();
}

fn clear_context_popover_when_available(
    launcher: Weak<RefCell<GtkLauncher>>,
    popover: gtk::Popover,
) {
    let Some(strong) = launcher.upgrade() else {
        return;
    };
    if try_with_launcher_mut(&strong, |launcher| {
        launcher.clear_active_context_popover_if_matches(&popover);
    })
    .is_none()
    {
        glib::idle_add_local_once(move || {
            clear_context_popover_when_available(launcher, popover);
        });
    }
}

fn connect_context_menu_item(
    item_button: &gtk::Button,
    command: ButtonContextCommand,
    launcher: &Rc<RefCell<GtkLauncher>>,
    popover: &gtk::Popover,
    tab_idx: usize,
    button_idx: usize,
) {
    let weak = Rc::downgrade(launcher);
    let popover = popover.downgrade();
    item_button.connect_clicked(move |_| {
        if let Some(popover) = popover.upgrade() {
            popover.popdown();
        }
        if let Some(launcher) = weak.upgrade() {
            let _ = try_with_launcher_mut(&launcher, |launcher| {
                if !indexed_context_command_enabled(
                    &launcher.folder_tabs,
                    tab_idx,
                    button_idx,
                    command,
                ) {
                    return;
                }
                match command {
                    ButtonContextCommand::Edit => {
                        launcher.edit_button(tab_idx, button_idx);
                    }
                    ButtonContextCommand::OpenInExplorer => {
                        launcher.open_button_in_explorer(tab_idx, button_idx);
                    }
                    ButtonContextCommand::Hide => {
                        launcher.hide_button(tab_idx, button_idx);
                    }
                }
            });
        }
    });
}

fn keyboard_context_menu_rect(width: i32, height: i32) -> gtk::gdk::Rectangle {
    gtk::gdk::Rectangle::new(width.max(1) / 2, height.max(1) / 2, 1, 1)
}

fn show_message(
    parent: &impl IsA<gtk::Window>,
    message_type: gtk::MessageType,
    title: &str,
    message: &str,
) {
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .message_type(message_type)
        .buttons(gtk::ButtonsType::Ok)
        .text(title)
        .secondary_text(message)
        .build();
    let _ = glib::MainContext::default().block_on(dialog.run_future());
    close_modal_dialog(&dialog, parent);
}

fn confirm_message(parent: &gtk::ApplicationWindow, title: &str, message: &str) -> bool {
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .message_type(gtk::MessageType::Warning)
        .buttons(gtk::ButtonsType::YesNo)
        .text(title)
        .secondary_text(message)
        .build();
    dialog.set_default_response(destructive_confirm_default_response());
    let response = glib::MainContext::default().block_on(dialog.run_future());
    close_modal_dialog(&dialog, parent);
    response == gtk::ResponseType::Yes
}

fn destructive_confirm_default_response() -> gtk::ResponseType {
    gtk::ResponseType::No
}

#[cfg(debug_assertions)]
fn apply_debug_scan_delay() {
    if let Ok(marker_path) = std::env::var("J3LAUNCHER_TEST_SCAN_DELAY_MARKER") {
        let _ = std::fs::write(marker_path, "started");
    }
    if let Some(delay) =
        debug_scan_delay_from_env(std::env::var("J3LAUNCHER_TEST_SCAN_DELAY_MS").ok())
    {
        thread::sleep(delay);
    }
}

#[cfg(not(debug_assertions))]
fn apply_debug_scan_delay() {}

#[cfg(debug_assertions)]
fn debug_scan_delay_from_env(value: Option<String>) -> Option<Duration> {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map(Duration::from_millis)
}

fn dialog_default_response_key(key: gtk::gdk::Key) -> bool {
    matches!(key.name().as_deref(), Some("Return" | "KP_Enter"))
}

fn connect_dialog_default_response_key(
    dialog: &gtk::Dialog,
    widget: &impl IsA<gtk::Widget>,
    response: gtk::ResponseType,
) {
    let key_controller = gtk::EventControllerKey::new();
    let dialog = dialog.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if dialog_default_response_key(key) {
            dialog.response(response);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    widget.add_controller(key_controller);
}

fn text_input_dialog(
    parent: &gtk::ApplicationWindow,
    title: &str,
    label: &str,
    initial: &str,
) -> Option<String> {
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(TEXT_DIALOG_WIDTH)
        .default_height(TEXT_DIALOG_HEIGHT)
        .build();
    dialog.add_button("OK", gtk::ResponseType::Ok);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let area = dialog.content_area();
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.set_margin_top(12);
    layout.set_margin_bottom(12);
    layout.set_margin_start(12);
    layout.set_margin_end(12);
    layout.append(&gtk::Label::new(Some(label)));
    let entry = gtk::Entry::new();
    entry.set_text(initial);
    entry.set_max_length(EDIT_DIALOG_TEXT_LIMIT);
    entry.set_activates_default(true);
    entry.set_hexpand(true);
    layout.append(&entry);
    area.append(&layout);
    dialog.set_default_response(gtk::ResponseType::Ok);
    entry.grab_focus();
    let response = glib::MainContext::default().block_on(dialog.run_future());
    let result = (response == gtk::ResponseType::Ok).then(|| entry.text().to_string());
    close_modal_dialog(&dialog, parent);
    result
}

fn tab_layout_dialog(
    parent: &gtk::ApplicationWindow,
    rows: u16,
    cols: u16,
    defaults: (u16, u16),
) -> Option<(u16, u16)> {
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .modal(true)
        .title("Tab Layout")
        .default_width(LAYOUT_DIALOG_WIDTH)
        .default_height(LAYOUT_DIALOG_HEIGHT)
        .build();
    dialog.add_button("Apply", gtk::ResponseType::Apply);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let area = dialog.content_area();
    let grid = gtk::Grid::builder()
        .column_spacing(8)
        .row_spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    let rows_entry = gtk::Entry::new();
    rows_entry.set_text(&rows.to_string());
    rows_entry.set_max_length(EDIT_DIALOG_TEXT_LIMIT);
    rows_entry.set_activates_default(true);
    rows_entry.set_hexpand(true);
    let cols_entry = gtk::Entry::new();
    cols_entry.set_text(&cols.to_string());
    cols_entry.set_max_length(EDIT_DIALOG_TEXT_LIMIT);
    cols_entry.set_activates_default(true);
    cols_entry.set_hexpand(true);
    grid.attach(&gtk::Label::new(Some("Rows")), 0, 0, 1, 1);
    grid.attach(&rows_entry, 1, 0, 1, 1);
    grid.attach(&gtk::Label::new(Some("Cols")), 0, 1, 1, 1);
    grid.attach(&cols_entry, 1, 1, 1, 1);
    area.append(&grid);
    dialog.set_default_response(gtk::ResponseType::Apply);
    rows_entry.grab_focus();
    let result = loop {
        let response = glib::MainContext::default().block_on(dialog.run_future());
        if response != gtk::ResponseType::Apply {
            break None;
        }
        match read_tab_layout_entries(&rows_entry, &cols_entry, defaults) {
            Ok(layout) => break Some(layout),
            Err(error) => {
                show_message(
                    &dialog,
                    gtk::MessageType::Warning,
                    "Tab Layout",
                    error.message,
                );
                error.entry.grab_focus();
                dialog.present();
            }
        }
    };
    close_modal_dialog(&dialog, parent);
    result
}

struct LayoutEntryValidationError<'a> {
    message: &'static str,
    entry: &'a gtk::Entry,
}

fn read_tab_layout_entries<'a>(
    rows_entry: &'a gtk::Entry,
    cols_entry: &'a gtk::Entry,
    defaults: (u16, u16),
) -> std::result::Result<(u16, u16), LayoutEntryValidationError<'a>> {
    let rows = parse_layout_value(
        rows_entry.text().as_str(),
        defaults.0,
        MAX_BUTTON_ROWS,
        "Rows",
    )
    .map_err(|message| LayoutEntryValidationError {
        message,
        entry: rows_entry,
    })?;
    let cols = parse_layout_value(
        cols_entry.text().as_str(),
        defaults.1,
        MAX_BUTTON_COLS,
        "Cols",
    )
    .map_err(|message| LayoutEntryValidationError {
        message,
        entry: cols_entry,
    })?;
    Ok((rows, cols))
}

fn parse_layout_value(
    raw_value: &str,
    default: u16,
    max_value: u16,
    label: &'static str,
) -> std::result::Result<u16, &'static str> {
    let text = raw_value.trim();
    if text.is_empty() {
        return Err(match label {
            "Rows" => "Rows is required.",
            _ => "Cols is required.",
        });
    }
    let parsed = text.parse::<i64>().map_err(|_| match label {
        "Rows" => "Rows must be a whole number.",
        _ => "Cols must be a whole number.",
    })?;
    if parsed < 1 {
        return Ok(default);
    }
    let parsed = u16::try_from(parsed).unwrap_or(u16::MAX);
    Ok(parsed.clamp(1, max_value))
}

fn edit_button_dialog(parent: &gtk::ApplicationWindow, initial: ButtonInfo) -> Option<ButtonInfo> {
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .modal(true)
        .title("Edit Button")
        .default_width(EDIT_DIALOG_WIDTH)
        .default_height(EDIT_DIALOG_HEIGHT)
        .build();
    dialog.add_button("OK", gtk::ResponseType::Ok);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let area = dialog.content_area();
    let grid = gtk::Grid::builder()
        .column_spacing(8)
        .row_spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    let name = gtk::Entry::new();
    name.set_text(&initial.name);
    name.set_max_length(EDIT_DIALOG_TEXT_LIMIT);
    name.set_activates_default(true);
    name.set_hexpand(true);
    let path = gtk::Entry::new();
    path.set_text(&initial.path);
    path.set_max_length(EDIT_DIALOG_TEXT_LIMIT);
    path.set_activates_default(true);
    path.set_hexpand(true);
    let path_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    let path_picker = gtk::Button::builder()
        .icon_name("document-open-symbolic")
        .tooltip_text("Select file")
        .width_request(36)
        .build();
    path_row.append(&path);
    path_row.append(&path_picker);
    let params = gtk::Entry::new();
    params.set_text(&initial.params);
    params.set_max_length(EDIT_DIALOG_TEXT_LIMIT);
    params.set_activates_default(true);
    params.set_hexpand(true);
    let admin = gtk::CheckButton::with_label("Run as administrator");
    admin.set_active(initial.admin);
    let copy = gtk::CheckButton::with_label("Copy Path + Params");
    copy.set_active(initial.action == 1);
    connect_dialog_default_response_key(&dialog, &admin, gtk::ResponseType::Ok);
    connect_dialog_default_response_key(&dialog, &copy, gtk::ResponseType::Ok);
    grid.attach(&gtk::Label::new(Some("Name")), 0, 0, 1, 1);
    grid.attach(&name, 1, 0, 1, 1);
    grid.attach(&gtk::Label::new(Some("Path")), 0, 1, 1, 1);
    grid.attach(&path_row, 1, 1, 1, 1);
    grid.attach(&gtk::Label::new(Some("Params")), 0, 2, 1, 1);
    grid.attach(&params, 1, 2, 1, 1);
    grid.attach(&admin, 1, 3, 1, 1);
    grid.attach(&copy, 1, 4, 1, 1);
    area.append(&grid);
    {
        let dialog = dialog.clone();
        let path = path.clone();
        path_picker.connect_clicked(move |_| {
            pick_button_path_file(&dialog, &path);
        });
    }
    dialog.set_default_response(gtk::ResponseType::Ok);
    name.grab_focus();
    let response = glib::MainContext::default().block_on(dialog.run_future());
    let result = (response == gtk::ResponseType::Ok).then(|| {
        button_info_from_edit_fields(
            &initial,
            name.text().as_str(),
            path.text().as_str(),
            params.text().as_str(),
            admin.is_active(),
            copy.is_active(),
        )
    });
    close_modal_dialog(&dialog, parent);
    result
}

fn pick_button_path_file(parent: &gtk::Dialog, path_entry: &gtk::Entry) {
    if let Some(result) = debug_file_picker_override() {
        apply_button_path_file_result(parent, path_entry, result);
        return;
    }

    let chooser = gtk::FileChooserNative::new(
        Some("Select File"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Select"),
        Some("Cancel"),
    );
    chooser.set_modal(true);
    set_file_chooser_initial_path(&chooser, path_entry.text().as_str());

    let parent = parent.clone();
    let path_entry = path_entry.clone();
    glib::MainContext::default().spawn_local(async move {
        let response = chooser.run_future().await;
        let selected_path = chooser.file().and_then(|file| file.path());
        chooser.destroy();
        parent.present();
        apply_button_path_file_result(
            &parent,
            &path_entry,
            selected_file_from_chooser_response(response, selected_path),
        );
    });
}

fn set_file_chooser_initial_path(chooser: &gtk::FileChooserNative, raw_path: &str) {
    let raw_path = raw_path.trim();
    if raw_path.is_empty() {
        return;
    }

    let path = PathBuf::from(raw_path);
    if path.is_file() {
        let file = gio::File::for_path(path);
        let _ = chooser.set_file(&file);
        return;
    }

    let folder = if path.is_dir() {
        Some(path.as_path())
    } else {
        path.parent()
    };
    let Some(folder) = folder.filter(|folder| !folder.as_os_str().is_empty()) else {
        return;
    };
    let file = gio::File::for_path(folder);
    let _ = chooser.set_current_folder(Some(&file));
}

fn apply_button_path_file_result(
    parent: &impl IsA<gtk::Window>,
    path_entry: &gtk::Entry,
    result: std::result::Result<Option<String>, String>,
) {
    match result {
        Ok(Some(path)) => {
            path_entry.set_text(&path);
            path_entry.grab_focus();
            path_entry.set_position(-1);
        }
        Ok(None) => {}
        Err(message) => show_message(parent, gtk::MessageType::Warning, "Select File", &message),
    }
}

fn button_info_from_edit_fields(
    initial: &ButtonInfo,
    name: &str,
    path: &str,
    params: &str,
    admin: bool,
    copy: bool,
) -> ButtonInfo {
    ButtonInfo {
        name: name.to_owned(),
        path: path.to_owned(),
        params: params.to_owned(),
        admin,
        action: if copy { 1 } else { 0 },
        auto_enter: initial.auto_enter,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiddenItemsDialogContent {
    EmptyLabel,
    SelectableList,
}

fn hidden_items_dialog_content(items: &[HiddenItem]) -> HiddenItemsDialogContent {
    if items.is_empty() {
        HiddenItemsDialogContent::EmptyLabel
    } else {
        HiddenItemsDialogContent::SelectableList
    }
}

fn hidden_items_dialog(parent: &gtk::ApplicationWindow, tab: &LauncherTab) -> Option<Vec<String>> {
    let items = hidden_items_for_tab(tab);
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .modal(true)
        .title("Manage Hidden Items")
        .default_width(HIDDEN_DIALOG_WIDTH)
        .default_height(HIDDEN_DIALOG_HEIGHT)
        .build();
    dialog.add_button("Unhide Selected", gtk::ResponseType::Apply);
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.set_default_response(gtk::ResponseType::Apply);
    let area = dialog.content_area();
    let mut list = None;
    match hidden_items_dialog_content(&items) {
        HiddenItemsDialogContent::EmptyLabel => {
            area.append(&gtk::Label::new(Some("No hidden items.")));
        }
        HiddenItemsDialogContent::SelectableList => {
            let item_list = gtk::ListBox::new();
            item_list.set_selection_mode(gtk::SelectionMode::Multiple);
            for item in &items {
                item_list.append(&gtk::Label::new(Some(&item.label)));
            }
            let scrolled = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Automatic)
                .vscrollbar_policy(gtk::PolicyType::Automatic)
                .min_content_height(220)
                .child(&item_list)
                .build();
            area.append(&scrolled);
            list = Some(item_list);
        }
    }
    let result = loop {
        let response = glib::MainContext::default().block_on(dialog.run_future());
        if response != gtk::ResponseType::Apply {
            break None;
        }
        let selected = list
            .as_ref()
            .map_or_else(Vec::new, |list| selected_hidden_item_ids(list, &items));
        if selected.is_empty() {
            dialog.present();
            continue;
        }
        break Some(selected);
    };
    close_modal_dialog(&dialog, parent);
    result
}

fn selected_hidden_item_ids(list: &gtk::ListBox, items: &[HiddenItem]) -> Vec<String> {
    selected_hidden_item_ids_from_indices(
        items,
        list.selected_rows()
            .into_iter()
            .filter_map(|row| usize::try_from(row.index()).ok()),
    )
}

fn close_modal_dialog<W, P>(dialog: &W, parent: &P)
where
    W: IsA<gtk::Window>,
    P: IsA<gtk::Window>,
{
    dialog.close();
    parent.present();
}

fn complete_scan_signature(result: &FolderScanResult) -> Option<ScanSignature> {
    if result.is_complete() {
        result.signature.clone()
    } else {
        None
    }
}

fn known_scan_options_from_tab(
    tab: &LauncherTab,
    include_known_items: bool,
) -> (Option<ScanSignature>, Option<Vec<ScanItem>>) {
    if include_known_items {
        (
            tab.scan_signature.clone(),
            build_known_scan_items_from_tab(tab),
        )
    } else {
        (tab.scan_signature.clone(), None)
    }
}

fn tab_title(tab: &LauncherTab, tab_idx: usize) -> String {
    if tab.title.trim().is_empty() {
        format!("Tab {}", tab_idx + 1)
    } else {
        tab.title.clone()
    }
}

fn button_icon_name(button: &LauncherButton, base_dir: Option<&Path>) -> Option<&'static str> {
    if !button_has_icon_info(button) || button.action == 1 {
        None
    } else if button.is_dir {
        Some("folder-symbolic")
    } else if has_executable_path_hint(&button.path, base_dir)
        || has_executable_path_hint(&button.source_path, base_dir)
    {
        Some("application-x-executable-symbolic")
    } else {
        Some("text-x-generic-symbolic")
    }
}

fn button_has_icon_info(button: &LauncherButton) -> bool {
    !button.name.trim().is_empty()
        || !button.source_name.trim().is_empty()
        || !button.path.trim().is_empty()
        || !button.source_path.trim().is_empty()
}

fn has_executable_path_hint(path: &str, base_dir: Option<&Path>) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let runtime_path = normalize_linux_runtime_path(resolve_runtime_path(path, base_dir));
    let target = Path::new(&runtime_path);
    if is_executable_file(target) {
        return true;
    }
    path_file_name(&runtime_path).is_some_and(has_executable_extension)
}

fn path_file_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn has_executable_extension(file_name: &str) -> bool {
    let Some((_, extension)) = file_name.rsplit_once('.') else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "appimage" | "bat" | "cmd" | "com" | "exe" | "msi" | "ps1" | "run" | "sh"
    )
}

fn normalize_linux_runtime_path(path: String) -> String {
    if has_windows_path_syntax(&path) || path.starts_with("\\\\") {
        path
    } else {
        path.replace('\\', "/")
    }
}

fn parse_initial_geometry(value: &str) -> (i32, i32) {
    let size = value.split(['+', '-']).next().unwrap_or(value).trim();
    let Some(separator_idx) = size.find(['x', 'X']) else {
        return (800, 600);
    };
    let (width, height) = size.split_at(separator_idx);
    let height = &height[1..];
    let width = width.trim().parse::<i32>().ok().filter(|value| *value > 0);
    let height = height.trim().parse::<i32>().ok().filter(|value| *value > 0);
    (width.unwrap_or(800), height.unwrap_or(600))
}

fn gtk_close_geometry_with_preserved_position(width: i32, height: i32, previous: &str) -> String {
    format!(
        "{}x{}{}",
        width.max(1),
        height.max(1),
        valid_geometry_position_suffix(previous).unwrap_or("")
    )
}

fn valid_geometry_position_suffix(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let separator = trimmed.find(['x', 'X'])?;
    if !trimmed[..separator].chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let height_and_position = &trimmed[separator + 1..];
    let height_len: usize = height_and_position
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum();
    if height_len == 0 {
        return None;
    }
    let suffix = &height_and_position[height_len..];
    is_valid_geometry_position_suffix(suffix).then_some(suffix)
}

fn is_valid_geometry_position_suffix(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let mut rest = value;
    for _ in 0..2 {
        let Some(sign) = rest.chars().next() else {
            return false;
        };
        if sign != '+' && sign != '-' {
            return false;
        }
        rest = &rest[sign.len_utf8()..];
        let digit_len: usize = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum();
        if digit_len == 0 {
            return false;
        }
        rest = &rest[digit_len..];
    }
    rest.is_empty()
}

fn launcher_grid_spacing(dark_theme: bool) -> i32 {
    if dark_theme {
        DARK_GRID_SPACING
    } else {
        LIGHT_GRID_SPACING
    }
}

fn launcher_grid_scroll_to_focus() -> bool {
    false
}

fn launcher_grid_overflow() -> gtk::Overflow {
    gtk::Overflow::Hidden
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .launcher-grid {
            padding: 0;
        }
        .launcher-button {
            min-width: 48px;
            min-height: 36px;
        }
        .launcher-empty-slot {
            min-width: 48px;
            min-height: 36px;
        }
        .j3-compact-titlebar,
        .j3-titlebar-layout {
            min-height: 22px;
            padding-top: 0;
            padding-bottom: 0;
        }
        .j3-titlebar-layout {
            padding-left: 6px;
        }
        .j3-titlebar-start {
            min-width: 42px;
            min-height: 22px;
        }
        .j3-titlebar-icon {
            min-width: 12px;
            min-height: 12px;
            padding: 0;
            margin: 0;
        }
        .j3-titlebar-controls {
            min-height: 22px;
            padding: 0;
            margin: 0;
        }
        .j3-titlebar-controls button {
            min-width: 20px;
            min-height: 20px;
            padding: 0;
            margin: 0;
        }
        .j3-titlebar-controls image {
            -gtk-icon-size: 12px;
            min-width: 12px;
            min-height: 12px;
        }
        .j3-titlebar-label {
            min-height: 20px;
            padding-top: 0;
            padding-bottom: 0;
        }
        ",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn apply_gtk_theme(dark: bool) {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
}

fn install_compact_titlebar(
    window: &gtk::ApplicationWindow,
    title: &str,
    icon_path: Option<&Path>,
) {
    let titlebar = gtk::WindowHandle::new();
    titlebar.add_css_class("j3-compact-titlebar");

    let layout = gtk::CenterBox::new();
    layout.add_css_class("j3-titlebar-layout");

    let start = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    start.add_css_class("j3-titlebar-start");
    start.append(&titlebar_icon(icon_path));

    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("j3-titlebar-label");
    title_label.set_single_line_mode(true);
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_label.set_valign(gtk::Align::Center);

    let controls = gtk::WindowControls::new(gtk::PackType::End);
    controls.add_css_class("j3-titlebar-controls");
    controls.set_valign(gtk::Align::Center);

    layout.set_start_widget(Some(&start));
    layout.set_center_widget(Some(&title_label));
    layout.set_end_widget(Some(&controls));
    titlebar.set_child(Some(&layout));
    window.set_titlebar(Some(&titlebar));
}

fn titlebar_icon(icon_path: Option<&Path>) -> gtk::Image {
    let image = if let Some(icon_path) = icon_path {
        gtk::Image::from_file(icon_path)
    } else {
        gtk::Image::from_icon_name(gtk_window_icon_name())
    };
    image.add_css_class("j3-titlebar-icon");
    image.set_pixel_size(TITLEBAR_ICON_SIZE);
    image
}

fn install_window_icon(svg_file_name: &str, png_file_name: &str) -> Option<PathBuf> {
    let search_dirs = gtk_window_icon_search_dirs();
    let texture_path = find_window_icon_file_from_candidates(
        svg_file_name,
        png_file_name,
        search_dirs.iter().map(PathBuf::as_path),
    );
    if let Some(display) = gtk::gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        for dir in search_dirs {
            icon_theme.add_search_path(dir);
        }
    }
    gtk::Window::set_default_icon_name(gtk_window_icon_name());
    texture_path
}

fn install_toplevel_window_icon_on_realize(window: &gtk::ApplicationWindow, icon_path: PathBuf) {
    let icon_path_for_realize = icon_path.clone();
    window.connect_realize(move |window| {
        apply_toplevel_window_icon(window, &icon_path_for_realize);
    });
    if window.is_realized() {
        apply_toplevel_window_icon(window, &icon_path);
    }
}

fn apply_toplevel_window_icon(window: &gtk::ApplicationWindow, icon_path: &Path) {
    let file = gio::File::for_path(icon_path);
    let Ok(texture) = gtk::gdk::Texture::from_file(&file) else {
        return;
    };
    let Some(surface) = window.surface() else {
        return;
    };
    let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() else {
        return;
    };
    toplevel.set_icon_list(&[texture]);
}

fn gtk_window_icon_name() -> &'static str {
    APP_LINUX_APPLICATION_ID
}

fn gtk_window_icon_search_dirs() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        candidates.push(exe_dir.to_path_buf());
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir);
    }
    deduplicate_paths(candidates)
}

fn find_window_icon_file_from_candidates<'a>(
    svg_file_name: &str,
    png_file_name: &str,
    candidates: impl IntoIterator<Item = &'a Path>,
) -> Option<PathBuf> {
    let candidates = deduplicate_paths(candidates.into_iter().map(Path::to_path_buf).collect());
    for candidate in &candidates {
        let icon_path = candidate.join(svg_file_name);
        if icon_path.is_file() {
            return Some(icon_path);
        }
    }
    for candidate in candidates {
        let icon_path = candidate.join(png_file_name);
        if icon_path.is_file() {
            return Some(icon_path);
        }
    }
    None
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduplicated = Vec::new();
    for path in paths {
        if !deduplicated.iter().any(|existing| existing == &path) {
            deduplicated.push(path);
        }
    }
    deduplicated
}

fn open_uri_for_path(path: &Path) -> ActionResult<()> {
    let file = gio::File::for_path(path);
    let uri = file.uri();
    open_uri(&uri)
}

fn open_uri(uri: &str) -> ActionResult<()> {
    gio::AppInfo::launch_default_for_uri(uri, None::<&gio::AppLaunchContext>)
        .map_err(gio_launch_failure)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinuxAdminCommandSpec {
    launcher: PathBuf,
    target: PathBuf,
    args: Vec<String>,
    current_dir: Option<PathBuf>,
}

fn linux_admin_command_spec(path: &str, params: &str) -> ActionResult<LinuxAdminCommandSpec> {
    let launcher = linux_admin_launcher()
        .ok_or_else(|| ActionFailure::runtime_unavailable("pkexec was not found in PATH"))?;
    linux_admin_command_spec_with_launcher(path, params, launcher)
}

fn linux_admin_command_spec_with_launcher(
    path: &str,
    params: &str,
    launcher: PathBuf,
) -> ActionResult<LinuxAdminCommandSpec> {
    if path.trim().is_empty() {
        return Err(ActionFailure::invalid_input("program path is empty"));
    }

    let target = PathBuf::from(path);
    if target.is_dir() {
        return Err(ActionFailure::is_directory(
            "directories cannot be run as administrator",
        ));
    }
    if admin_target_requires_direct_file_check(path, &target) {
        let metadata = target
            .metadata()
            .map_err(|source| action_failure_from_io(source, "administrator launch target"))?;
        if !metadata.is_file() {
            return Err(ActionFailure::invalid_input(
                "administrator launch target is not a file",
            ));
        }
        if !is_executable_file(&target) {
            return Err(ActionFailure::permission_denied(
                "administrator launch target is not executable",
            ));
        }
    } else if find_program_on_path(path).is_none() {
        return Err(ActionFailure::not_found(
            "administrator launch target was not found in PATH",
        ));
    }

    Ok(LinuxAdminCommandSpec {
        launcher,
        target,
        args: split_command_params(params.trim())?,
        current_dir: None,
    })
}

fn admin_target_requires_direct_file_check(raw_path: &str, target: &Path) -> bool {
    target.is_absolute() || raw_path.contains('/') || raw_path.contains('\\')
}

fn linux_admin_result_from_exit_status(
    status: ExitStatus,
    executable: &str,
) -> Option<AdminLaunchResult> {
    match status.code() {
        Some(126) => Some(AdminLaunchResult::cancelled(
            "pkexec authentication dialog was dismissed.",
            126,
            0,
            executable,
        )),
        Some(127) => Some(AdminLaunchResult::failed(
            "pkexec authorization failed or an authentication error occurred.",
            127,
            0,
            executable,
        )),
        _ => None,
    }
}

fn linux_admin_launcher() -> Option<PathBuf> {
    find_program_on_path("pkexec")
}

fn find_program_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_program_on_path_with_path(program, &path)
}

fn find_program_on_path_with_path(program: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable_file(candidate))
}

fn linux_folder_scan_guard_message(folder_path: &str) -> Option<String> {
    let expanded = expand_environment_variables(folder_path);
    if has_windows_path_syntax(&expanded) || has_unresolved_windows_env_reference(&expanded) {
        return Some(format!(
            "Windows 전용 폴더 경로는 Linux에서 직접 스캔할 수 없습니다.\n{}",
            normalize_linux_runtime_path(expanded)
        ));
    }
    None
}

fn open_in_file_manager_with(
    target_path: &str,
    mut show_item: impl FnMut(&Path) -> ActionResult<()>,
    mut open_path: impl FnMut(&Path) -> ActionResult<()>,
) -> ActionResult<Option<ExplorerOpenFeedback>> {
    if target_path.trim().is_empty() {
        return Err(ActionFailure::invalid_input("path is empty"));
    }

    let target = PathBuf::from(target_path);
    if target.is_dir() {
        open_path(&target)?;
        return Ok(None);
    }
    if target.is_file() {
        if show_item(&target).is_ok() {
            return Ok(None);
        }
        if let Some((parent, feedback)) = file_manager_selection_fallback(&target) {
            open_path(parent)?;
            return Ok(Some(feedback));
        }
    }
    if let Some(parent) = target.parent().filter(|parent| parent.is_dir()) {
        open_path(parent)?;
        return Ok(Some(ExplorerOpenFeedback::new(
            "warning",
            format!("대상을 찾을 수 없어 상위 폴더를 엽니다:\n{target_path}"),
        )));
    }

    Ok(Some(ExplorerOpenFeedback::new(
        "warning",
        format!("경로를 찾을 수 없습니다:\n{target_path}"),
    )))
}

fn file_manager_selection_fallback(path: &Path) -> Option<(&Path, ExplorerOpenFeedback)> {
    path.parent()
        .filter(|parent| parent.is_dir())
        .map(|parent| {
            (
                parent,
                ExplorerOpenFeedback::new(
                    "warning",
                    "파일 관리자에서 파일 선택을 요청할 수 없어 상위 폴더를 엽니다.",
                ),
            )
        })
}

fn show_item_in_file_manager(path: &Path) -> ActionResult<()> {
    let file = gio::File::for_path(path);
    let uri = file.uri();
    let parameters = (vec![uri.as_str()], "").to_variant();
    let proxy = gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | gio::DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
        None::<&gio::DBusInterfaceInfo>,
        "org.freedesktop.FileManager1",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        None::<&gio::Cancellable>,
    )
    .map_err(file_manager_dbus_failure)?;
    proxy
        .call_sync(
            "ShowItems",
            Some(&parameters),
            gio::DBusCallFlags::NONE,
            FILE_MANAGER_DBUS_TIMEOUT_MS,
            None::<&gio::Cancellable>,
        )
        .map(|_| ())
        .map_err(file_manager_dbus_failure)
}

fn file_manager_dbus_failure(source: glib::Error) -> ActionFailure {
    ActionFailure::runtime_unavailable(format!("file manager item selection failed: {source}"))
}

fn gio_launch_failure(source: glib::Error) -> ActionFailure {
    let detail = format!("gio launch failed: {source}");
    match source.kind::<gio::IOErrorEnum>() {
        Some(gio::IOErrorEnum::NotFound) => ActionFailure::not_found(detail),
        Some(gio::IOErrorEnum::PermissionDenied) => ActionFailure::permission_denied(detail),
        Some(gio::IOErrorEnum::InvalidArgument | gio::IOErrorEnum::InvalidFilename) => {
            ActionFailure::invalid_input(detail)
        }
        Some(gio::IOErrorEnum::IsDirectory) => ActionFailure::is_directory(detail),
        Some(gio::IOErrorEnum::NotSupported | gio::IOErrorEnum::NotInitialized) => {
            ActionFailure::runtime_unavailable(detail)
        }
        _ => ActionFailure::platform(detail),
    }
}

fn windows_compatible_clipboard_text(text: &str) -> String {
    text.replace('\0', "\u{FFFD}")
}

fn close_geometry_save_failure_level() -> &'static str {
    "warning"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkLaunchDecision {
    OpenPathUri,
    OpenRawUri,
    SpawnCommand,
}

fn gtk_launch_decision(
    path_text: &str,
    target: &Path,
    params: &str,
) -> ActionResult<GtkLaunchDecision> {
    if launch_uri_scheme(path_text).is_some() {
        return Ok(GtkLaunchDecision::OpenRawUri);
    }

    if target.is_dir() {
        if !params.is_empty() {
            return Err(ActionFailure::invalid_input(
                "directory launch does not support additional parameters",
            ));
        }
        return Ok(GtkLaunchDecision::OpenPathUri);
    }

    if target.exists() && !is_executable_file(target) {
        return Ok(GtkLaunchDecision::OpenPathUri);
    }

    Ok(GtkLaunchDecision::SpawnCommand)
}

fn launch_uri_scheme(value: &str) -> Option<&str> {
    runtime_uri_scheme(value)
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn action_failure_from_io(source: std::io::Error, detail: &str) -> ActionFailure {
    match source.kind() {
        std::io::ErrorKind::NotFound => ActionFailure::not_found(source.to_string()),
        std::io::ErrorKind::PermissionDenied => {
            ActionFailure::permission_denied(source.to_string())
        }
        std::io::ErrorKind::InvalidInput => ActionFailure::invalid_input(source.to_string()),
        _ => ActionFailure::platform(format!("{detail}: {source}")),
    }
}

fn split_command_params(value: &str) -> ActionResult<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();
    let mut in_quotes = false;
    let mut in_arg = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let mut slash_count = 1usize;
                while chars.peek().is_some_and(|next| *next == '\\') {
                    slash_count = slash_count.saturating_add(1);
                    let _ = chars.next();
                }
                if chars.peek().is_some_and(|next| *next == '"') {
                    for _ in 0..slash_count / 2 {
                        current.push('\\');
                    }
                    let _ = chars.next();
                    if slash_count.is_multiple_of(2) {
                        in_quotes = !in_quotes;
                    } else {
                        current.push('"');
                    }
                } else {
                    for _ in 0..slash_count {
                        current.push('\\');
                    }
                }
                in_arg = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                in_arg = true;
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            ch => {
                current.push(ch);
                in_arg = true;
            }
        }
    }

    if in_arg {
        args.push(current);
    }
    Ok(args)
}

fn parse_drag_payload(value: &str) -> Option<(usize, usize, usize)> {
    let mut parts = value.split(':');
    let tab_idx = parts.next()?.parse().ok()?;
    let button_idx = parts.next()?.parse().ok()?;
    let slot_idx = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((tab_idx, button_idx, slot_idx))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::ui::common::{button_context_hide_enabled, button_context_open_enabled};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_tab(tab_type: TabType) -> LauncherTab {
        LauncherTab {
            id: String::from("test-tab"),
            tab_type,
            title: String::from("Test"),
            folder_path: String::new(),
            rows: 1,
            cols: 1,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: Vec::new(),
            scan_signature: None,
            scan_item_order: None,
        }
    }

    fn make_executable(path: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
    }

    fn unix_exit_status(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;

        std::process::ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn linux_runtime_path_accepts_windows_style_relative_separators() {
        let resolved = resolve_runtime_path("tools\\run.sh", Some(Path::new("/tmp/j3launcher")));

        assert_eq!(
            normalize_linux_runtime_path(resolved),
            "/tmp/j3launcher/tools/run.sh"
        );
    }

    #[test]
    fn linux_runtime_path_keeps_windows_absolute_paths_for_platform_guard() {
        assert_eq!(
            normalize_linux_runtime_path(String::from("C:\\Tools\\app.exe")),
            "C:\\Tools\\app.exe"
        );
    }

    #[test]
    fn linux_runtime_path_keeps_windows_drive_qualified_paths_for_platform_guard() {
        assert_eq!(
            normalize_linux_runtime_path(String::from("C:Tools\\app.exe")),
            "C:Tools\\app.exe"
        );
    }

    #[test]
    fn linux_folder_scan_guard_blocks_windows_only_paths_before_worker_scan() {
        let drive_message = linux_folder_scan_guard_message("C:\\Users\\me\\Tools")
            .expect("drive paths should be blocked on Linux");
        assert!(drive_message.contains("Windows 전용 폴더 경로"));
        assert!(drive_message.contains("C:\\Users\\me\\Tools"));

        let drive_relative_message = linux_folder_scan_guard_message("C:Users\\me\\Tools")
            .expect("drive-relative paths should be blocked on Linux");
        assert!(drive_relative_message.contains("Windows 전용 폴더 경로"));
        assert!(drive_relative_message.contains("C:Users\\me\\Tools"));

        let unc_message = linux_folder_scan_guard_message("\\\\server\\share\\Tools")
            .expect("UNC paths should be blocked on Linux");
        assert!(unc_message.contains("Windows 전용 폴더 경로"));
        assert!(unc_message.contains("\\\\server\\share\\Tools"));

        let env_message = linux_folder_scan_guard_message("%USERPROFILE%\\Tools")
            .expect("unresolved Windows env paths should be blocked on Linux");
        assert!(env_message.contains("%USERPROFILE%/Tools"));

        assert!(linux_folder_scan_guard_message("/home/me/tools").is_none());
        assert!(linux_folder_scan_guard_message("/tmp/%stage%/tools").is_none());
    }

    #[test]
    fn gtk_scan_request_folder_path_normalizes_windows_relative_separators() {
        let request = GtkScanRequest::Refresh {
            tab_id: String::from("tab"),
            folder_path: String::from("fixtures\\tools"),
            known_signature: None,
        };
        assert_eq!(request.folder_path(), PathBuf::from("fixtures/tools"));

        let windows_only = GtkScanRequest::Refresh {
            tab_id: String::from("tab"),
            folder_path: String::from("C:\\Tools"),
            known_signature: None,
        };
        assert_eq!(windows_only.folder_path(), PathBuf::from("C:\\Tools"));
    }

    #[test]
    fn gtk_scan_request_folder_path_uses_expanded_guard_path_for_worker_io() {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("cargo should set CARGO_MANIFEST_DIR for tests");
        let expected = PathBuf::from(manifest_dir).join("fixtures/tools");
        let request = GtkScanRequest::Refresh {
            tab_id: String::from("tab"),
            folder_path: String::from("$CARGO_MANIFEST_DIR\\fixtures\\tools"),
            known_signature: None,
        };

        assert_eq!(request.folder_path(), expected);
    }

    #[test]
    fn gtk_scan_request_options_match_folder_picker_scan_flows() {
        let signature = ScanSignature::new("/tmp/tools", 1, 2, 3);
        let known_items = vec![ScanItem::new("file-a", "a.txt", "/tmp/tools/a.txt", false)];
        let mut set_folder = GtkScanRequest::SetFolder {
            tab_id: String::from("tab"),
            folder_path: String::from("/tmp/tools"),
            known_signature: Some(signature.clone()),
            known_items: Some(known_items.clone()),
        };

        let set_options = set_folder.take_options(new_scan_cancel_token());

        assert!(set_options.cancel_token.is_some());
        assert_eq!(set_options.known_signature, Some(signature));
        assert_eq!(set_options.known_items, Some(known_items));
        assert!(!set_options.allow_signature_only_unchanged);

        let mut add_folder = GtkScanRequest::AddFolder {
            folder_path: String::from("/tmp/tools"),
        };

        let add_options = add_folder.take_options(new_scan_cancel_token());

        assert!(add_options.cancel_token.is_some());
        assert!(add_options.known_signature.is_none());
        assert!(add_options.known_items.is_none());
        assert!(!add_options.allow_signature_only_unchanged);
    }

    #[test]
    fn initial_geometry_accepts_uppercase_separator_like_win32() {
        assert_eq!(parse_initial_geometry("1024X768+10+20"), (1024, 768));
    }

    #[test]
    fn gtk_close_geometry_preserves_existing_valid_position_suffix() {
        assert_eq!(
            gtk_close_geometry_with_preserved_position(900, 700, "631x324+943+1873"),
            "900x700+943+1873"
        );
        assert_eq!(
            gtk_close_geometry_with_preserved_position(900, 700, "631X324-10+20"),
            "900x700-10+20"
        );
        assert_eq!(
            gtk_close_geometry_with_preserved_position(900, 700, "800x600 trailing"),
            "900x700"
        );
        assert_eq!(
            gtk_close_geometry_with_preserved_position(0, -1, "800x600+1+2"),
            "1x1+1+2"
        );
    }

    #[test]
    fn empty_tab_title_uses_win32_index_fallback() {
        let tab = LauncherTab {
            id: String::from("tab-1"),
            tab_type: TabType::Manual,
            title: String::from("  "),
            folder_path: String::new(),
            rows: MANUAL_DEFAULT_BUTTON_ROWS,
            cols: MANUAL_DEFAULT_BUTTON_COLS,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: Vec::new(),
            scan_signature: None,
            scan_item_order: None,
        };

        assert_eq!(tab_title(&tab, 2), "Tab 3");
    }

    #[test]
    fn gtk_grid_spacing_matches_win32_theme_gap_rule() {
        assert_eq!(launcher_grid_spacing(false), 6);
        assert_eq!(launcher_grid_spacing(true), 0);
    }

    #[test]
    fn gtk_button_cell_floor_matches_win32_layout_floor() {
        assert_eq!((BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT), (48, 36));
    }

    #[test]
    fn gtk_window_icon_name_uses_linux_application_id() {
        assert_eq!(gtk_window_icon_name(), APP_LINUX_APPLICATION_ID);
    }

    #[test]
    fn gtk_window_icon_file_prefers_svg_over_png()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let exe = TempTestDir::new("gtk-window-icon-exe")?;
        let current = TempTestDir::new("gtk-window-icon-current")?;
        fs::write(exe.path().join("icon.png"), b"png")?;
        fs::write(current.path().join("icon.svg"), b"svg")?;

        let icon_path = find_window_icon_file_from_candidates(
            "icon.svg",
            "icon.png",
            [exe.path(), current.path()],
        );

        assert_eq!(icon_path, Some(current.path().join("icon.svg")));
        Ok(())
    }

    #[test]
    fn gtk_window_icon_file_falls_back_to_png_in_runtime_lookup_order()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let exe = TempTestDir::new("gtk-window-icon-exe")?;
        let current = TempTestDir::new("gtk-window-icon-current")?;
        fs::write(exe.path().join("icon.png"), b"exe-png")?;
        fs::write(current.path().join("icon.png"), b"png")?;

        let icon_path = find_window_icon_file_from_candidates(
            "icon.svg",
            "icon.png",
            [exe.path(), exe.path(), current.path()],
        );

        assert_eq!(icon_path, Some(exe.path().join("icon.png")));
        Ok(())
    }

    #[test]
    fn gtk_grid_viewport_clips_without_scrolling_like_win32_tab_client() {
        assert!(!launcher_grid_scroll_to_focus());
        assert_eq!(launcher_grid_overflow(), gtk::Overflow::Hidden);
    }

    #[test]
    fn gtk_dialog_default_sizes_follow_win32_client_baseline() {
        assert_eq!((TEXT_DIALOG_WIDTH, TEXT_DIALOG_HEIGHT), (420, 150));
        assert_eq!((LAYOUT_DIALOG_WIDTH, LAYOUT_DIALOG_HEIGHT), (260, 160));
        assert_eq!((EDIT_DIALOG_WIDTH, EDIT_DIALOG_HEIGHT), (460, 280));
        assert_eq!((HIDDEN_DIALOG_WIDTH, HIDDEN_DIALOG_HEIGHT), (360, 280));
    }

    #[test]
    fn gtk_destructive_confirm_default_matches_win32_no_button() {
        assert_eq!(
            destructive_confirm_default_response(),
            gtk::ResponseType::No
        );
    }

    #[test]
    fn gtk_dialog_default_response_key_matches_win32_enter_behavior() {
        assert!(dialog_default_response_key(gtk::gdk::Key::Return));
        assert!(dialog_default_response_key(gtk::gdk::Key::KP_Enter));
        assert!(!dialog_default_response_key(gtk::gdk::Key::space));
        assert!(!dialog_default_response_key(gtk::gdk::Key::Escape));
    }

    #[test]
    fn gtk_debug_scan_delay_accepts_positive_milliseconds_only() {
        assert_eq!(
            debug_scan_delay_from_env(Some(String::from("25"))),
            Some(Duration::from_millis(25))
        );
        assert_eq!(debug_scan_delay_from_env(Some(String::from("0"))), None);
        assert_eq!(
            debug_scan_delay_from_env(Some(String::from("invalid"))),
            None
        );
        assert_eq!(debug_scan_delay_from_env(None), None);
    }

    #[test]
    fn gtk_text_entry_limit_matches_win32_edit_control_limit() {
        assert_eq!(EDIT_DIALOG_TEXT_LIMIT, 32_767);
    }

    #[test]
    fn gtk_edit_button_result_preserves_auto_enter_like_win32() {
        let initial = ButtonInfo {
            name: String::from("Old"),
            path: String::from("old.exe"),
            params: String::from("--old"),
            admin: false,
            action: 0,
            auto_enter: true,
        };

        let updated =
            button_info_from_edit_fields(&initial, "New", "new.exe", "--fast", true, true);

        assert_eq!(
            updated,
            ButtonInfo {
                name: String::from("New"),
                path: String::from("new.exe"),
                params: String::from("--fast"),
                admin: true,
                action: 1,
                auto_enter: true,
            }
        );
    }

    #[test]
    fn gtk_clipboard_text_replaces_nul_like_win32_clipboard() {
        assert_eq!(
            windows_compatible_clipboard_text("a\0b\0"),
            "a\u{FFFD}b\u{FFFD}"
        );
    }

    #[test]
    fn gtk_close_geometry_save_failure_uses_win32_warning_level() {
        assert_eq!(close_geometry_save_failure_level(), "warning");
    }

    #[test]
    fn gtk_folder_chooser_distinguishes_cancel_and_non_local_accept()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            selected_folder_from_chooser_response(gtk::ResponseType::Cancel, None)?,
            None
        );
        assert_eq!(
            selected_folder_from_chooser_response(gtk::ResponseType::Close, None)?,
            None
        );
        assert_eq!(
            selected_folder_from_chooser_response(gtk::ResponseType::Reject, None)?,
            None
        );
        assert_eq!(
            selected_folder_from_chooser_response(gtk::ResponseType::None, None)?,
            None
        );

        assert!(
            selected_folder_from_chooser_response(gtk::ResponseType::Accept, None).is_err(),
            "accepted non-local folder selections must report an error"
        );

        let selected =
            selected_folder_from_chooser_response(gtk::ResponseType::Accept, Some("/tmp".into()))?;
        assert_eq!(selected, Some(String::from("/tmp")));
        Ok(())
    }

    #[test]
    fn gtk_file_chooser_distinguishes_cancel_and_non_local_accept()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            selected_file_from_chooser_response(gtk::ResponseType::Cancel, None)?,
            None
        );
        assert_eq!(
            selected_file_from_chooser_response(gtk::ResponseType::Close, None)?,
            None
        );
        assert_eq!(
            selected_file_from_chooser_response(gtk::ResponseType::Reject, None)?,
            None
        );
        assert_eq!(
            selected_file_from_chooser_response(gtk::ResponseType::None, None)?,
            None
        );

        assert!(
            selected_file_from_chooser_response(gtk::ResponseType::Accept, None).is_err(),
            "accepted non-local file selections must report an error"
        );

        let selected = selected_file_from_chooser_response(
            gtk::ResponseType::Accept,
            Some("/tmp/tool.sh".into()),
        )?;
        assert_eq!(selected, Some(String::from("/tmp/tool.sh")));
        Ok(())
    }

    #[test]
    fn gtk_debug_folder_picker_override_supports_path_cancel_and_error() {
        assert_eq!(
            debug_folder_picker_override_from_env(Some("/tmp/tools".into()), None),
            Some(Ok(Some(String::from("/tmp/tools"))))
        );
        assert_eq!(
            debug_folder_picker_override_from_env(Some("__CANCEL__".into()), None),
            Some(Ok(None))
        );
        assert_eq!(
            debug_folder_picker_override_from_env(None, Some("no local path".into())),
            Some(Err(String::from("no local path")))
        );
        assert_eq!(
            debug_folder_picker_override_from_env(
                Some("/tmp/tools".into()),
                Some("no local path".into())
            ),
            Some(Err(String::from("no local path")))
        );
        assert_eq!(debug_folder_picker_override_from_env(None, None), None);
    }

    #[test]
    fn gtk_debug_file_picker_override_supports_path_cancel_and_error() {
        assert_eq!(
            debug_file_picker_override_from_env(Some("/tmp/tool.sh".into()), None),
            Some(Ok(Some(String::from("/tmp/tool.sh"))))
        );
        assert_eq!(
            debug_file_picker_override_from_env(Some("__CANCEL__".into()), None),
            Some(Ok(None))
        );
        assert_eq!(
            debug_file_picker_override_from_env(None, Some("no local path".into())),
            Some(Err(String::from("no local path")))
        );
        assert_eq!(
            debug_file_picker_override_from_env(
                Some("/tmp/tool.sh".into()),
                Some("no local path".into())
            ),
            Some(Err(String::from("no local path")))
        );
        assert_eq!(debug_file_picker_override_from_env(None, None), None);
    }

    #[test]
    fn gtk_button_icon_keeps_dotted_documents_generic() {
        let button = icon_test_button("notes.txt", "", false, 0);

        assert_eq!(
            button_icon_name(&button, None),
            Some("text-x-generic-symbolic")
        );
    }

    #[test]
    fn gtk_button_icon_uses_executable_extension_hints() {
        let button = icon_test_button("C:\\Tools\\app.exe", "", false, 0);

        assert_eq!(
            button_icon_name(&button, None),
            Some("application-x-executable-symbolic")
        );
    }

    #[test]
    fn gtk_button_icon_omits_copy_button_icon_like_win32() {
        let button = icon_test_button("/tmp/note.txt", "", false, 1);

        assert_eq!(button_icon_name(&button, None), None);
    }

    #[test]
    fn gtk_button_icon_omits_empty_button_info() {
        let button = LauncherButton {
            is_dir: true,
            ..LauncherButton::manual_default()
        };

        assert_eq!(button_icon_name(&button, None), None);
    }

    #[test]
    fn gtk_button_icon_uses_unix_execute_bit() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempTestDir::new("gtk-executable-icon")?;
        let executable = temp.path().join("tool");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n")?;
        let mut permissions = fs::metadata(&executable)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions)?;
        let button = icon_test_button(executable.to_string_lossy().as_ref(), "", false, 0);

        assert_eq!(
            button_icon_name(&button, None),
            Some("application-x-executable-symbolic")
        );
        Ok(())
    }

    #[test]
    fn gtk_button_icon_resolves_relative_executable_from_config_base()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempTestDir::new("gtk-relative-executable-icon")?;
        let bin = temp.path().join("bin");
        fs::create_dir(&bin)?;
        let executable = bin.join("tool");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n")?;
        let mut permissions = fs::metadata(&executable)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions)?;
        let button = icon_test_button("bin/tool", "", false, 0);

        assert_eq!(
            button_icon_name(&button, Some(temp.path())),
            Some("application-x-executable-symbolic")
        );
        Ok(())
    }

    #[test]
    fn gtk_button_drag_guard_blocks_scan_and_close_reentry() {
        assert!(can_accept_button_drag(false, false));
        assert!(!can_accept_button_drag(true, false));
        assert!(!can_accept_button_drag(false, true));
    }

    #[test]
    fn gtk_drag_click_guard_suppresses_drag_release_click_once_like_win32() {
        let guard = DragClickGuard::default();

        assert!(!guard.should_suppress_click());
        guard.mark_drag_started();
        assert!(guard.should_suppress_click());
        assert!(!guard.should_suppress_click());
        guard.mark_drag_started();
        guard.clear();
        assert!(!guard.should_suppress_click());
    }

    #[test]
    fn gtk_context_menu_open_enabled_matches_win32_path_rule() {
        assert!(!button_context_open_enabled(&LauncherButton::default()));
        assert!(button_context_open_enabled(&LauncherButton {
            path: String::from("tool.exe"),
            ..LauncherButton::default()
        }));
        assert!(button_context_open_enabled(&LauncherButton {
            source_path: String::from("C:/Tools/source.exe"),
            ..LauncherButton::default()
        }));
    }

    #[test]
    fn gtk_context_menu_hide_enabled_matches_win32_folder_item_rule() {
        let folder_tab = test_tab(TabType::Folder);
        let manual_tab = test_tab(TabType::Manual);
        let hidden_item = LauncherButton {
            item_id: String::from("item-1"),
            ..LauncherButton::default()
        };

        assert!(button_context_hide_enabled(&folder_tab, &hidden_item));
        assert!(!button_context_hide_enabled(&manual_tab, &hidden_item));
        assert!(!button_context_hide_enabled(
            &folder_tab,
            &LauncherButton::default()
        ));
    }

    #[test]
    fn gtk_indexed_context_menu_guard_matches_win32_rules() {
        let mut folder_tab = test_tab(TabType::Folder);
        folder_tab.buttons.push(LauncherButton {
            item_id: String::from("item-1"),
            path: String::from("tool.exe"),
            ..LauncherButton::default()
        });
        let mut manual_tab = test_tab(TabType::Manual);
        manual_tab.buttons.push(LauncherButton {
            item_id: String::from("manual-item"),
            path: String::from("tool.exe"),
            ..LauncherButton::default()
        });
        let tabs = vec![folder_tab, manual_tab];

        assert!(indexed_context_command_enabled(
            &tabs,
            0,
            0,
            ButtonContextCommand::Edit
        ));
        assert!(indexed_context_command_enabled(
            &tabs,
            0,
            0,
            ButtonContextCommand::OpenInExplorer
        ));
        assert!(indexed_context_command_enabled(
            &tabs,
            0,
            0,
            ButtonContextCommand::Hide
        ));
        assert!(!indexed_context_command_enabled(
            &tabs,
            1,
            0,
            ButtonContextCommand::Hide
        ));
        assert!(!indexed_context_command_enabled(
            &tabs,
            99,
            0,
            ButtonContextCommand::Edit
        ));
        assert!(!indexed_context_command_enabled(
            &tabs,
            0,
            99,
            ButtonContextCommand::Edit
        ));
    }

    #[test]
    fn gtk_context_menu_target_validation_matches_win32_no_popup_for_stale_target() {
        let mut tab = test_tab(TabType::Folder);
        tab.buttons.push(LauncherButton::default());
        let tabs = vec![tab];

        assert!(indexed_context_target_exists(&tabs, 0, 0));
        assert!(!indexed_context_target_exists(&tabs, 1, 0));
        assert!(!indexed_context_target_exists(&tabs, 0, 1));
    }

    #[test]
    fn gtk_context_menu_keyboard_navigation_skips_disabled_items_and_wraps() {
        let enabled = [true, false, true];

        assert_eq!(
            context_menu_focus_move_for_key(gtk::gdk::Key::Down),
            Some(ContextMenuFocusMove::Next)
        );
        assert_eq!(
            context_menu_focus_move_for_key(gtk::gdk::Key::Up),
            Some(ContextMenuFocusMove::Previous)
        );
        assert_eq!(
            context_menu_focus_move_for_key(gtk::gdk::Key::Home),
            Some(ContextMenuFocusMove::First)
        );
        assert_eq!(
            context_menu_focus_move_for_key(gtk::gdk::Key::End),
            Some(ContextMenuFocusMove::Last)
        );
        assert_eq!(context_menu_focus_move_for_key(gtk::gdk::Key::F5), None);

        assert_eq!(
            context_menu_focus_target(&enabled, None, ContextMenuFocusMove::Next),
            Some(0)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, None, ContextMenuFocusMove::Previous),
            Some(2)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, Some(0), ContextMenuFocusMove::Next),
            Some(2)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, Some(2), ContextMenuFocusMove::Next),
            Some(0)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, Some(0), ContextMenuFocusMove::Previous),
            Some(2)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, Some(2), ContextMenuFocusMove::Previous),
            Some(0)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, Some(1), ContextMenuFocusMove::First),
            Some(0)
        );
        assert_eq!(
            context_menu_focus_target(&enabled, Some(1), ContextMenuFocusMove::Last),
            Some(2)
        );
        assert_eq!(
            context_menu_focus_target(&[false, false], Some(0), ContextMenuFocusMove::Next),
            None
        );
    }

    #[test]
    fn gtk_context_menu_keyboard_activation_requires_focused_enabled_item() {
        let enabled = [true, false, true];

        assert!(context_menu_activation_key(gtk::gdk::Key::Return));
        assert!(context_menu_activation_key(gtk::gdk::Key::KP_Enter));
        assert!(context_menu_activation_key(gtk::gdk::Key::space));
        assert!(!context_menu_activation_key(gtk::gdk::Key::Down));

        assert_eq!(context_menu_activation_target(&enabled, Some(0)), Some(0));
        assert_eq!(context_menu_activation_target(&enabled, Some(2)), Some(2));
        assert_eq!(context_menu_activation_target(&enabled, Some(1)), None);
        assert_eq!(context_menu_activation_target(&enabled, Some(99)), None);
        assert_eq!(context_menu_activation_target(&enabled, None), None);
    }

    #[test]
    fn gtk_hidden_items_empty_state_uses_static_label_like_win32() {
        assert_eq!(
            hidden_items_dialog_content(&[]),
            HiddenItemsDialogContent::EmptyLabel
        );
        assert_eq!(
            hidden_items_dialog_content(&[HiddenItem {
                item_id: String::from("item-1"),
                label: String::from("Hidden item"),
            }]),
            HiddenItemsDialogContent::SelectableList
        );
    }

    #[test]
    fn gtk_keyboard_context_menu_anchors_to_button_center_like_win32() {
        assert_eq!(
            keyboard_context_menu_rect(48, 36),
            gtk::gdk::Rectangle::new(24, 18, 1, 1)
        );
        assert_eq!(
            keyboard_context_menu_rect(0, 0),
            gtk::gdk::Rectangle::new(0, 0, 1, 1)
        );
    }

    #[test]
    fn gtk_grid_rows_preserve_configured_and_sparse_slots_like_win32() {
        let mut manual = LauncherTab {
            id: String::from("manual"),
            tab_type: TabType::Manual,
            title: String::from("Manual"),
            folder_path: String::new(),
            rows: 2,
            cols: 3,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: vec![LauncherButton::manual_default()],
            scan_signature: None,
            scan_item_order: None,
        };
        let mut slots = Vec::new();
        let mut scratch = VisibleButtonSlotScratch::default();
        collect_visible_button_slots(&manual, &mut slots, &mut scratch);

        assert_eq!(required_grid_rows(&manual, &slots), 2);

        manual.rows = 1;
        manual.cols = 3;
        manual.buttons = vec![
            LauncherButton::manual_default(),
            LauncherButton::manual_default(),
            LauncherButton::manual_default(),
            LauncherButton::manual_default(),
        ];
        collect_visible_button_slots(&manual, &mut slots, &mut scratch);

        assert_eq!(required_grid_rows(&manual, &slots), 1);

        let mut slot_positions = BTreeMap::new();
        slot_positions.insert(String::from("item-1"), 5);
        let folder = LauncherTab {
            id: String::from("folder"),
            tab_type: TabType::Folder,
            title: String::from("Folder"),
            folder_path: String::from("/tmp"),
            rows: 1,
            cols: 3,
            hidden_item_ids: Vec::new(),
            slot_positions,
            buttons: vec![LauncherButton {
                item_id: String::from("item-1"),
                source_name: String::from("Item"),
                source_path: String::from("/tmp/item"),
                is_dir: false,
                name: String::from("Item"),
                path: String::from("/tmp/item"),
                params: String::new(),
                admin: false,
                action: 0,
                auto_enter: false,
            }],
            scan_signature: None,
            scan_item_order: None,
        };
        collect_visible_button_slots(&folder, &mut slots, &mut scratch);

        assert_eq!(required_grid_rows(&folder, &slots), 2);
    }

    #[test]
    fn gtk_tab_layout_value_validation_matches_win32_policy() {
        assert!(parse_layout_value("", DEFAULT_BUTTON_ROWS, MAX_BUTTON_ROWS, "Rows").is_err());
        assert_eq!(
            parse_layout_value("-5", DEFAULT_BUTTON_ROWS, MAX_BUTTON_ROWS, "Rows").ok(),
            Some(DEFAULT_BUTTON_ROWS)
        );
        assert_eq!(
            parse_layout_value("999", DEFAULT_BUTTON_COLS, MAX_BUTTON_COLS, "Cols").ok(),
            Some(MAX_BUTTON_COLS)
        );
    }

    #[test]
    fn split_command_params_preserves_windows_style_backslashes()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            split_command_params(r#"--path C:\Tools\bin --name "C:\Program Files\App""#)?,
            vec![
                String::from("--path"),
                String::from(r"C:\Tools\bin"),
                String::from("--name"),
                String::from(r"C:\Program Files\App"),
            ]
        );
        Ok(())
    }

    #[test]
    fn split_command_params_preserves_empty_windows_quoted_args()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            split_command_params(r#"--empty "" --name "A B""#)?,
            vec![
                String::from("--empty"),
                String::new(),
                String::from("--name"),
                String::from("A B"),
            ]
        );
        Ok(())
    }

    #[test]
    fn split_command_params_treats_single_quotes_as_windows_literals()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            split_command_params("--name 'A B'")?,
            vec![
                String::from("--name"),
                String::from("'A"),
                String::from("B'"),
            ]
        );
        Ok(())
    }

    #[test]
    fn split_command_params_handles_backslash_escaped_quotes()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            split_command_params(r#"--title \"literal\" --path "C:\Program Files\App""#)?,
            vec![
                String::from("--title"),
                String::from("\"literal\""),
                String::from("--path"),
                String::from(r"C:\Program Files\App"),
            ]
        );
        Ok(())
    }

    #[test]
    fn file_manager_selection_fallback_opens_parent_with_warning()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-file-manager-fallback")?;
        let file = temp.path().join("note.txt");
        fs::write(&file, b"hello")?;

        let Some((parent, feedback)) = file_manager_selection_fallback(&file) else {
            panic!("existing file should have a parent fallback");
        };
        assert_eq!(parent, temp.path());
        assert_eq!(feedback.level, "warning");
        assert!(feedback.message.contains("상위 폴더"));
        Ok(())
    }

    #[test]
    fn gtk_open_explorer_selects_existing_file_when_file_manager_supports_show_items()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-open-explorer-select-file")?;
        let file = temp.path().join("note.txt");
        fs::write(&file, b"hello")?;
        let target = file.to_string_lossy().into_owned();
        let mut selected = Vec::new();
        let mut opened = Vec::new();

        let feedback = open_in_file_manager_with(
            &target,
            |path| {
                selected.push(path.to_path_buf());
                Ok(())
            },
            |path| {
                opened.push(path.to_path_buf());
                Ok(())
            },
        )?;

        assert!(feedback.is_none());
        assert_eq!(selected, vec![file]);
        assert!(opened.is_empty());
        Ok(())
    }

    #[test]
    fn gtk_open_explorer_file_selection_failure_opens_parent_with_warning()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-open-explorer-selection-failure")?;
        let file = temp.path().join("note.txt");
        fs::write(&file, b"hello")?;
        let target = file.to_string_lossy().into_owned();
        let mut selected = Vec::new();
        let mut opened = Vec::new();

        let feedback = open_in_file_manager_with(
            &target,
            |path| {
                selected.push(path.to_path_buf());
                Err(ActionFailure::runtime_unavailable(
                    "file manager item selection failed",
                ))
            },
            |path| {
                opened.push(path.to_path_buf());
                Ok(())
            },
        )?;

        let Some(feedback) = feedback else {
            panic!("selection failure should produce warning feedback");
        };
        assert_eq!(feedback.level, "warning");
        assert!(feedback.message.contains("상위 폴더"));
        assert_eq!(selected, vec![file]);
        assert_eq!(opened, vec![temp.path().to_path_buf()]);
        Ok(())
    }

    #[test]
    fn gtk_open_explorer_missing_file_opens_existing_parent_with_warning()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-open-explorer-missing-file")?;
        let missing = temp.path().join("missing.txt");
        let target = missing.to_string_lossy().into_owned();
        let mut opened = Vec::new();

        let feedback = open_in_file_manager_with(
            &target,
            |path| panic!("missing target should not request selection: {path:?}"),
            |path| {
                opened.push(path.to_path_buf());
                Ok(())
            },
        )?;

        let Some(feedback) = feedback else {
            panic!("missing target with existing parent should produce warning feedback");
        };
        assert_eq!(feedback.level, "warning");
        assert!(feedback.message.contains("대상을 찾을 수 없어"));
        assert_eq!(opened, vec![temp.path().to_path_buf()]);
        Ok(())
    }

    #[test]
    fn linux_admin_command_uses_pkexec_with_windows_compatible_args()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-admin-command")?;
        let target = temp.path().join("tool.sh");
        let launcher = temp.path().join("pkexec");
        fs::write(&target, b"#!/bin/sh\n")?;
        make_executable(&target)?;

        let spec = linux_admin_command_spec_with_launcher(
            &target.to_string_lossy(),
            r#"--empty "" --name "A B" --literal \"quote\""#,
            launcher.clone(),
        )?;

        assert_eq!(spec.launcher, launcher);
        assert_eq!(spec.target, target);
        assert_eq!(
            spec.args,
            vec![
                String::from("--empty"),
                String::new(),
                String::from("--name"),
                String::from("A B"),
                String::from("--literal"),
                String::from("\"quote\""),
            ]
        );
        assert_eq!(spec.current_dir, None);
        Ok(())
    }

    #[test]
    fn linux_admin_command_rejects_missing_direct_targets_like_win32_runas()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-admin-missing")?;
        let missing = temp.path().join("missing-tool");
        let error = linux_admin_command_spec_with_launcher(
            &missing.to_string_lossy(),
            "",
            temp.path().join("pkexec"),
        )
        .expect_err("missing direct targets should be rejected");

        assert_eq!(error.kind, crate::app::actions::ActionFailureKind::NotFound);
        Ok(())
    }

    #[test]
    fn linux_admin_command_rejects_missing_bare_targets_before_pkexec()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-admin-missing-bare")?;
        let error = linux_admin_command_spec_with_launcher(
            "j3launcher-definitely-missing-admin-target",
            "",
            temp.path().join("pkexec"),
        )
        .expect_err("missing bare targets should be rejected before pkexec");

        assert_eq!(error.kind, crate::app::actions::ActionFailureKind::NotFound);
        Ok(())
    }

    #[test]
    fn linux_admin_command_rejects_non_executable_direct_targets()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-admin-non-executable")?;
        let target = temp.path().join("tool.sh");
        fs::write(&target, b"#!/bin/sh\n")?;

        let error = linux_admin_command_spec_with_launcher(
            &target.to_string_lossy(),
            "",
            temp.path().join("pkexec"),
        )
        .expect_err("non-executable direct targets should be rejected");

        assert_eq!(
            error.kind,
            crate::app::actions::ActionFailureKind::PermissionDenied
        );
        Ok(())
    }

    #[test]
    fn linux_admin_command_rejects_directories_like_win32_admin_launch()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-admin-directory")?;
        let error = linux_admin_command_spec_with_launcher(
            &temp.path().to_string_lossy(),
            "",
            temp.path().join("pkexec"),
        )
        .expect_err("directories should be rejected");

        assert_eq!(
            error.kind,
            crate::app::actions::ActionFailureKind::IsDirectory
        );
        Ok(())
    }

    #[test]
    fn linux_admin_exit_status_maps_pkexec_cancel_and_auth_failure() {
        let cancelled = linux_admin_result_from_exit_status(unix_exit_status(126), "/bin/tool")
            .expect("pkexec cancellation should be reported");
        assert_eq!(
            cancelled.status,
            crate::app::actions::AdminLaunchStatus::Cancelled
        );
        assert_eq!(cancelled.code, 126);
        assert_eq!(cancelled.executable, "/bin/tool");

        let failed = linux_admin_result_from_exit_status(unix_exit_status(127), "/bin/tool")
            .expect("pkexec authorization failure should be reported");
        assert_eq!(
            failed.status,
            crate::app::actions::AdminLaunchStatus::Failed
        );
        assert_eq!(failed.code, 127);
        assert_eq!(failed.executable, "/bin/tool");
    }

    #[test]
    fn linux_admin_exit_status_ignores_program_exit_codes_like_win32_runas() {
        assert!(linux_admin_result_from_exit_status(unix_exit_status(0), "/bin/tool").is_none());
        assert!(linux_admin_result_from_exit_status(unix_exit_status(42), "/bin/tool").is_none());
    }

    #[test]
    fn gtk_admin_launch_monitor_reports_pkexec_terminal_status()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let child = Command::new("sh").arg("-c").arg("exit 126").spawn()?;
        let mut monitor = GtkAdminLaunchMonitor::default();
        monitor.track(child, String::from("/bin/tool"));

        for _ in 0..50 {
            let results = monitor.poll();
            if let Some(result) = results.first() {
                assert_eq!(
                    result.status,
                    crate::app::actions::AdminLaunchStatus::Cancelled
                );
                assert_eq!(result.code, 126);
                assert_eq!(result.executable, "/bin/tool");
                assert!(monitor.processes.is_empty());
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }

        panic!("admin launch monitor did not report completed pkexec child");
    }

    #[test]
    fn linux_admin_launcher_finds_executable_pkexec_on_path()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-admin-path")?;
        let pkexec = temp.path().join("pkexec");
        fs::write(&pkexec, b"#!/bin/sh\n")?;
        make_executable(&pkexec)?;

        assert_eq!(
            find_program_on_path_with_path("pkexec", temp.path().as_os_str()),
            Some(pkexec)
        );
        assert_eq!(
            find_program_on_path_with_path("missing", temp.path().as_os_str()),
            None
        );
        Ok(())
    }

    #[test]
    fn gtk_menu_model_exposes_accelerator_hints() {
        let menu = build_menu_model();
        let file = menu.item_link(0, "submenu").expect("File submenu");
        let mut accels = Vec::new();

        for section_idx in 0..file.n_items() {
            let section = file
                .item_link(section_idx, "section")
                .expect("menu section");
            for item_idx in 0..section.n_items() {
                let label = section
                    .item_attribute_value(item_idx, "label", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.str().map(ToOwned::to_owned))
                    .unwrap_or_default();
                let accel = section
                    .item_attribute_value(item_idx, "accel", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.str().map(ToOwned::to_owned));
                if let Some(accel) = accel {
                    accels.push((label, accel));
                }
            }
        }

        assert_eq!(
            accels,
            vec![
                (
                    String::from("Move Tab Left"),
                    String::from("<Control><Shift>Left"),
                ),
                (
                    String::from("Move Tab Right"),
                    String::from("<Control><Shift>Right"),
                ),
                (
                    String::from("Select Previous Tab"),
                    String::from("<Control>Page_Up"),
                ),
                (
                    String::from("Select Next Tab"),
                    String::from("<Control>Page_Down"),
                ),
                (String::from("Sorting Current Tab"), String::from("F5"),),
            ]
        );
    }

    #[test]
    fn gtk_accelerator_bindings_are_registered_from_shared_menu_spec() {
        let bindings = gtk_accelerator_bindings_from_menu_spec()
            .into_iter()
            .map(|(action, accels)| (action.to_owned(), accels.to_vec()))
            .collect::<Vec<_>>();

        assert_eq!(
            bindings,
            vec![
                (String::from("win.move-left"), vec!["<Control><Shift>Left"]),
                (
                    String::from("win.move-right"),
                    vec!["<Control><Shift>Right"],
                ),
                (String::from("win.select-prev"), vec!["<Control>Page_Up"]),
                (String::from("win.select-next"), vec!["<Control>Page_Down"],),
                (String::from("win.sort"), vec!["F5"]),
            ]
        );
    }

    #[test]
    fn gtk_window_key_controller_maps_win32_accelerator_tuples() {
        let ctrl = gtk::gdk::ModifierType::CONTROL_MASK;
        let shift = gtk::gdk::ModifierType::SHIFT_MASK;
        let alt = gtk::gdk::ModifierType::ALT_MASK;
        let lock = gtk::gdk::ModifierType::LOCK_MASK;

        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Left, ctrl | shift),
            Some(MenuCommand::MoveLeft)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Right, ctrl | shift | lock),
            Some(MenuCommand::MoveRight)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Page_Up, ctrl),
            Some(MenuCommand::SelectPrev)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Prior, ctrl),
            Some(MenuCommand::SelectPrev)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Page_Down, ctrl),
            Some(MenuCommand::SelectNext)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Next, ctrl),
            Some(MenuCommand::SelectNext)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::F5, gtk::gdk::ModifierType::empty()),
            Some(MenuCommand::Sort)
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Page_Up, ctrl | shift),
            None
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::F5, ctrl),
            None
        );
        assert_eq!(
            gtk_accelerator_command_for_key(gtk::gdk::Key::Right, ctrl | shift | alt),
            None
        );
    }

    #[test]
    fn gtk_menu_model_mirrors_shared_file_menu_sections() {
        let menu = build_menu_model();
        let file = menu.item_link(0, "submenu").expect("File submenu");
        let about = menu.item_link(1, "submenu").expect("About submenu");

        assert_eq!(menu.n_items(), 2);
        assert_eq!(
            usize::try_from(file.n_items()).ok(),
            Some(MAIN_MENU_SECTIONS.len())
        );
        for (section_idx, expected_section) in MAIN_MENU_SECTIONS.iter().enumerate() {
            let section_idx = i32::try_from(section_idx).expect("section index fits i32");
            let section = file
                .item_link(section_idx, "section")
                .expect("menu section");

            assert_eq!(
                usize::try_from(section.n_items()).ok(),
                Some(expected_section.len())
            );
            for (item_idx, expected_item) in expected_section.iter().enumerate() {
                let item_idx = i32::try_from(item_idx).expect("item index fits i32");
                let label = section
                    .item_attribute_value(item_idx, "label", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.str().map(ToOwned::to_owned));
                let action = section
                    .item_attribute_value(item_idx, "action", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.str().map(ToOwned::to_owned));

                assert_eq!(label.as_deref(), Some(expected_item.label));
                assert_eq!(action.as_deref(), Some(expected_item.gtk_action));
            }
        }

        assert_eq!(
            usize::try_from(about.n_items()).ok(),
            Some(ABOUT_MENU_SECTIONS.len())
        );
        let section = about.item_link(0, "section").expect("About menu section");
        let item = ABOUT_MENU_SECTIONS[0][0];
        let label = section
            .item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
            .and_then(|value| value.str().map(ToOwned::to_owned));
        let action = section
            .item_attribute_value(0, "action", Some(glib::VariantTy::STRING))
            .and_then(|value| value.str().map(ToOwned::to_owned));
        assert_eq!(label.as_deref(), Some(item.label));
        assert_eq!(action.as_deref(), Some(item.gtk_action));
    }

    #[test]
    fn gtk_menu_actions_cover_shared_menu_spec() {
        let actions = MenuActions::empty();
        let action_names = main_menu_items()
            .map(|item| format!("win.{}", actions.action_for_command(item.command).name()))
            .collect::<Vec<_>>();
        let expected_action_names = main_menu_items()
            .map(|item| item.gtk_action.to_owned())
            .collect::<Vec<_>>();

        assert_eq!(action_names, expected_action_names);
    }

    #[test]
    fn gio_not_supported_launch_error_maps_to_runtime_unavailable() {
        let error = glib::Error::new(gio::IOErrorEnum::NotSupported, "no default app");
        let failure = gio_launch_failure(error);

        assert_eq!(
            failure.kind,
            crate::app::actions::ActionFailureKind::RuntimeUnavailable
        );
    }

    #[test]
    fn gtk_launch_decision_opens_associated_files_even_with_params()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-associated-file")?;
        let file = temp.path().join("note.txt");
        fs::write(&file, b"hello")?;

        assert_eq!(
            gtk_launch_decision(file.to_string_lossy().as_ref(), &file, "--ignored-by-gio")?,
            GtkLaunchDecision::OpenPathUri
        );
        Ok(())
    }

    #[test]
    fn gtk_launch_decision_opens_protocol_uris_like_shellexecute()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            gtk_launch_decision(
                "https://example.test",
                Path::new("https://example.test"),
                ""
            )?,
            GtkLaunchDecision::OpenRawUri
        );
        assert_eq!(
            gtk_launch_decision(
                "mailto:user@example.test",
                Path::new("mailto:user@example.test"),
                ""
            )?,
            GtkLaunchDecision::OpenRawUri
        );
        assert_eq!(launch_uri_scheme("C:Tools\\app.exe"), None);

        Ok(())
    }

    #[test]
    fn gtk_launch_decision_rejects_directory_params()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("gtk-directory-params")?;
        let error = gtk_launch_decision(
            temp.path().to_string_lossy().as_ref(),
            temp.path(),
            "--flag",
        )
        .expect_err("directory params fail");

        assert_eq!(
            error.kind,
            crate::app::actions::ActionFailureKind::InvalidInput
        );
        Ok(())
    }

    #[test]
    fn gtk_launch_decision_spawns_missing_bare_commands() {
        assert_eq!(
            gtk_launch_decision("missing-tool", Path::new("missing-tool"), "")
                .expect("missing command decision"),
            GtkLaunchDecision::SpawnCommand
        );
    }

    fn icon_test_button(
        path: impl Into<String>,
        source_path: impl Into<String>,
        is_dir: bool,
        action: u8,
    ) -> LauncherButton {
        LauncherButton {
            item_id: String::new(),
            source_name: String::new(),
            source_path: source_path.into(),
            is_dir,
            name: String::from("Button"),
            path: path.into(),
            params: String::new(),
            admin: false,
            action,
            auto_enter: false,
        }
    }

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(label: &str) -> std::io::Result<Self> {
            let unique = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "j3launcher-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
