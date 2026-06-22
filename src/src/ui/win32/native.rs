use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{
    ERROR_SUCCESS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SetLastError,
    WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX, DT_WORDBREAK,
    DeleteObject, DrawFocusRect, DrawTextW, FillRect, FrameRect, HBRUSH, HDC, HGDIOBJ,
    InvalidateRect, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    DRAWITEMSTRUCT, EM_SETLIMITTEXT, HIMAGELIST, ICC_TAB_CLASSES, ILC_COLOR32, ILD_NORMAL,
    INITCOMMONCONTROLSEX, ImageList_Create, ImageList_Destroy, ImageList_Draw,
    ImageList_ReplaceIcon, InitCommonControlsEx, NMHDR, ODS_DISABLED, ODS_FOCUS, ODS_HOTLIGHT,
    ODS_SELECTED, ODT_BUTTON, SetWindowTheme, TCIF_TEXT, TCITEMW, TCM_ADJUSTRECT,
    TCM_DELETEALLITEMS, TCM_GETCURSEL, TCM_INSERTITEMW, TCM_SETCURSEL, TCN_SELCHANGE,
    WC_TABCONTROLW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, ReleaseCapture, SetActiveWindow, SetFocus, VK_ESCAPE, VK_F5,
    VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ACCEL, AppendMenuW, BM_GETCHECK, BM_SETCHECK, BM_SETSTATE, BN_CLICKED, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, BS_MULTILINE, BS_OWNERDRAW, BS_PUSHBUTTON, CREATESTRUCTW, CS_HREDRAW,
    CS_VREDRAW, CW_USEDEFAULT, CheckMenuItem, CreateAcceleratorTableW, CreateMenu, CreatePopupMenu,
    CreateWindowExW, DefWindowProcW, DestroyAcceleratorTable, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, DrawMenuBar, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE,
    ES_READONLY, EnableMenuItem, FCONTROL, FSHIFT, FVIRTKEY, GWLP_USERDATA, GetClientRect,
    GetCursorPos, GetDlgItem, GetDlgItemTextW, GetMessageW, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, HACCEL, HWND_TOP, ICON_BIG, ICON_SMALL, IDC_ARROW, IDCANCEL, IDOK, IDYES,
    IMAGE_ICON, IsDialogMessageW, IsWindow, LB_ADDSTRING, LB_GETSELCOUNT, LB_GETSELITEMS,
    LBS_EXTENDEDSEL, LBS_NOINTEGRALHEIGHT, LR_DEFAULTSIZE, LR_LOADFROMFILE, LoadCursorW,
    LoadImageW, MB_DEFBUTTON2, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_YESNO,
    MF_BYCOMMAND, MF_CHECKED, MF_ENABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED, MSG, MessageBoxW, MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW,
    SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetForegroundWindow,
    SetMenu, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, TranslateAcceleratorW, TranslateMessage, WM_CAPTURECHANGED,
    WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORDLG,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_ENTERSIZEMOVE, WM_ERASEBKGND,
    WM_EXITSIZEMOVE, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE,
    WM_NCDESTROY, WM_NOTIFY, WM_SETICON, WM_SETREDRAW, WM_SIZE, WNDCLASSW, WS_BORDER, WS_CAPTION,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME,
    WS_EX_WINDOWEDGE, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    WS_VSCROLL,
};
use windows_sys::core::w;

use crate::app::actions::{
    LauncherActionService, LauncherPlatform, SystemLauncherPlatform, UserMessage,
};
use crate::app::button_layout::{ButtonSlotMove, ButtonSlotMoveError, move_button_between_slots};
use crate::app::config_service::ConfigService;
use crate::app::folder_tabs::{
    FolderTabMutationError, add_folder_tab, add_manual_tab, build_known_scan_items_from_tab,
    delete_tab, hide_item, refresh_tab_from_scan_result, rename_tab, reset_tab, set_tab_folder,
    sort_tab, unhide_items, update_tab_layout,
};
use crate::app::tab_actions::{self, TabMoveDirection};
#[cfg(test)]
use crate::domain::tab::max_button_slot_index;
use crate::domain::{
    APP_ABOUT_TEXT, APP_AUTHOR_URL, APP_DISPLAY_NAME, APP_VERSION, DEFAULT_BUTTON_COLS,
    DEFAULT_BUTTON_ROWS, FolderScanResult, LauncherButton, LauncherTab, MANUAL_DEFAULT_BUTTON_COLS,
    MANUAL_DEFAULT_BUTTON_ROWS, MAX_BUTTON_COLS, MAX_BUTTON_ROWS, ScanSignature, TabType,
};
use crate::infra::config_store::ButtonInfo;
use crate::platform::windows::dpi::{DpiMetrics, get_dpi_for_system, get_window_dpi_metrics};
use crate::platform::windows::icon::{
    self, ButtonIconKey, ButtonIconRequest, ButtonIconResult, ButtonIconWorker, IconBitmap,
    IconCacheKey, LruCache, RenderedIconKey,
};
use crate::platform::windows::input::WindowHandle;
use crate::ui::WindowSpec;
use crate::ui::common::{
    ABOUT_MENU_SECTIONS, BUTTON_CONTEXT_MENU_ITEMS, ButtonContextCommand, HiddenItem,
    MAIN_MENU_SECTIONS, MenuActionAvailability, MenuCommand, MenuCommandHandler, VisibleButtonSlot,
    VisibleButtonSlotScratch, button_context_command_enabled, button_label,
    button_open_in_explorer_path, collect_visible_button_slots, dispatch_menu_command_if_enabled,
    folder_tab_mutation_error_message, hidden_items_for_tab, menu_action_availability,
    selected_hidden_item_ids_from_indices, user_message_title,
};
use crate::ui::win32::button_drag::{ButtonDragController, ButtonDragEndpoint, CursorPoint};
use crate::ui::win32::geometry::{WindowGeometry, parse_window_geometry};
use crate::ui::win32::scan_worker::{ScanRequest, ScanWorker};
use crate::{LauncherError, Result};

const MAIN_CLASS_NAME: &str = "j3Launcher.Win32.MainWindow";
const CONTENT_BACKGROUND_CLASS_NAME: &str = "j3Launcher.Win32.ContentBackground";
const EDIT_CLASS_NAME: &str = "j3Launcher.Win32.EditButtonDialog";
const TEXT_INPUT_CLASS_NAME: &str = "j3Launcher.Win32.TextInputDialog";
const TAB_LAYOUT_CLASS_NAME: &str = "j3Launcher.Win32.TabLayoutDialog";
const HIDDEN_ITEMS_CLASS_NAME: &str = "j3Launcher.Win32.HiddenItemsDialog";
const ABOUT_CLASS_NAME: &str = "j3Launcher.Win32.AboutDialog";
const APP_ICON_RESOURCE_ID: u16 = 1;

const ID_MENU_ADD_FOLDER_TAB: usize = 100;
const ID_MENU_ADD_MANUAL_TAB: usize = 101;
const ID_MENU_SET_TAB_FOLDER: usize = 102;
const ID_MENU_TAB_LAYOUT: usize = 103;
const ID_MENU_RENAME_TAB: usize = 104;
const ID_MENU_DELETE_TAB: usize = 105;
const ID_MENU_MOVE_LEFT: usize = 106;
const ID_MENU_MOVE_RIGHT: usize = 107;
const ID_MENU_SELECT_PREV: usize = 108;
const ID_MENU_SELECT_NEXT: usize = 109;
const ID_MENU_SORT: usize = 110;
const ID_MENU_REFRESH: usize = 111;
const ID_MENU_RESET: usize = 112;
const ID_MENU_MANAGE_HIDDEN: usize = 113;
const ID_MENU_DARK_THEME: usize = 114;
const ID_MENU_EXIT: usize = 115;
const ID_MENU_ABOUT: usize = 116;

const ID_CONTEXT_EDIT: usize = 200;
const ID_CONTEXT_OPEN_EXPLORER: usize = 201;
const ID_CONTEXT_HIDE: usize = 202;

const ID_BUTTON_BASE: usize = 10_000;
const ID_EDIT_NAME: i32 = 20_001;
const ID_EDIT_PATH: i32 = 20_002;
const ID_EDIT_PARAMS: i32 = 20_003;
const ID_CHECK_ADMIN: i32 = 20_004;
const ID_CHECK_COPY: i32 = 20_005;
const ID_EDIT_OK: i32 = 20_007;
const ID_EDIT_CANCEL: i32 = 20_008;
const ID_TEXT_INPUT: i32 = 20_102;
const ID_TEXT_OK: i32 = 20_103;
const ID_TEXT_CANCEL: i32 = 20_104;
const ID_LAYOUT_ROWS: i32 = 20_201;
const ID_LAYOUT_COLS: i32 = 20_202;
const ID_LAYOUT_APPLY: i32 = 20_203;
const ID_LAYOUT_CANCEL: i32 = 20_204;
const ID_HIDDEN_LIST: i32 = 20_301;
const ID_HIDDEN_UNHIDE: i32 = 20_302;
const ID_HIDDEN_CLOSE: i32 = 20_303;
const ID_ABOUT_VERSION: i32 = 20_402;
const ID_ABOUT_LINK: i32 = 20_403;
const ID_ABOUT_CLOSE: i32 = 20_404;
const ID_ABOUT_LICENSES: i32 = 20_405;

const EDIT_DIALOG_CLIENT_WIDTH: i32 = 460;
const EDIT_DIALOG_CLIENT_HEIGHT: i32 = 280;
const EDIT_DIALOG_ROW_HEIGHT: i32 = 24;
const EDIT_DIALOG_BUTTON_Y: i32 = 225;
const EDIT_DIALOG_BOTTOM_PADDING: i32 = 16;
const EDIT_DIALOG_TEXT_LIMIT: usize = 32_767;
const TEXT_DIALOG_CLIENT_WIDTH: i32 = 420;
const TEXT_DIALOG_CLIENT_HEIGHT: i32 = 150;
const LAYOUT_DIALOG_CLIENT_WIDTH: i32 = 260;
const LAYOUT_DIALOG_CLIENT_HEIGHT: i32 = 160;
const HIDDEN_DIALOG_CLIENT_WIDTH: i32 = 360;
const HIDDEN_DIALOG_CLIENT_HEIGHT: i32 = 280;
const ABOUT_DIALOG_CLIENT_WIDTH: i32 = 450;
const ABOUT_DIALOG_CLIENT_HEIGHT: i32 = 350;

const WM_SCAN_COMPLETE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;
const WM_ICON_COMPLETE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 2;
const WM_CONFIG_SAVE_COMPLETE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 3;
const WM_BUTTON_DRAG_EVENT: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 4;
const BUTTON_DRAG_SUBCLASS_ID: usize = 1;
const BUTTON_DRAG_EVENT_DOWN: usize = 1;
const BUTTON_DRAG_EVENT_MOVE: usize = 2;
const BUTTON_DRAG_EVENT_UP: usize = 3;
const BUTTON_DRAG_EVENT_CANCEL: usize = 4;
const BUTTON_DRAG_THRESHOLD_PX: i32 = 4;

const DARK_BACKGROUND: u32 = colorref(30, 30, 30);
const DARK_BUTTON: u32 = colorref(48, 48, 48);
const DARK_BUTTON_HOT: u32 = colorref(58, 58, 58);
const DARK_BUTTON_PRESSED: u32 = colorref(38, 38, 38);
const DARK_BORDER: u32 = colorref(92, 92, 92);
const DARK_TEXT: u32 = colorref(242, 242, 242);
const DARK_DISABLED_TEXT: u32 = colorref(132, 132, 132);
const LIGHT_BACKGROUND: u32 = colorref(240, 240, 240);
const LIGHT_BUTTON: u32 = colorref(255, 255, 255);
const LIGHT_BUTTON_HOT: u32 = colorref(229, 241, 251);
const LIGHT_BUTTON_PRESSED: u32 = colorref(204, 228, 247);
const LIGHT_BORDER: u32 = colorref(204, 204, 204);
const LIGHT_TEXT: u32 = colorref(0, 0, 0);
const LIGHT_DISABLED_TEXT: u32 = colorref(109, 109, 109);

pub fn run_window(spec: WindowSpec) -> Result<()> {
    crate::platform::windows::initialize_process_dpi_awareness()?;
    init_common_controls()?;
    let hinstance = current_hinstance()?;
    register_main_class(hinstance)?;
    register_content_background_class(hinstance)?;
    register_edit_class(hinstance)?;
    register_text_input_class(hinstance)?;
    register_tab_layout_class(hinstance)?;
    register_hidden_items_class(hinstance)?;
    register_about_class(hinstance)?;

    let config = match spec.config_path.as_deref() {
        Some(config_path) => ConfigService::open_path_from_executable_or_current_dir(config_path)?,
        None => ConfigService::open_from_executable_or_current_dir()?,
    };
    let dpi_scale = get_dpi_for_system()
        .map(DpiMetrics::from_dpi)
        .map(|metrics| metrics.scale)
        .unwrap_or(1.0);
    let title = wide_z(spec.title);
    let class_name = wide_z(MAIN_CLASS_NAME);
    let app = Box::new(Win32App::new(spec, hinstance, config, dpi_scale)?);
    let geometry = app.initial_geometry();
    let app_ptr = Box::into_raw(app);
    let mut create_state = MainWindowCreateState {
        app: app_ptr,
        destroyed: false,
    };
    // Safety: app_ptr was produced by Box::into_raw and remains valid until the
    // window takes ownership or the failure path reclaims it below.
    unsafe { (*app_ptr).creation_state = &mut create_state };

    // Safety: class_name/title are NUL-terminated and live for the call.
    // create_state lives for CreateWindowExW's synchronous creation messages.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            geometry.x.unwrap_or(CW_USEDEFAULT),
            geometry.y.unwrap_or(CW_USEDEFAULT),
            geometry.width,
            geometry.height,
            null_mut(),
            null_mut(),
            hinstance,
            (&mut create_state as *mut MainWindowCreateState).cast(),
        )
    };
    if hwnd.is_null() {
        if !create_state.destroyed {
            // Safety: if WM_NCDESTROY did not run, no window reclaimed app_ptr.
            unsafe { drop(Box::from_raw(app_ptr)) };
        }
        return Err(platform_error("CreateWindowExW failed for main window"));
    }

    // Safety: hwnd is alive and owns app_ptr through GWLP_USERDATA from here.
    unsafe { (*app_ptr).creation_state = null_mut() };
    let accelerators = create_main_accelerators();
    // Safety: hwnd is a valid top-level window created above.
    unsafe { ShowWindow(hwnd, SW_SHOW) };
    run_message_loop(hwnd, accelerators.handle())?;
    Ok(())
}

struct Win32App {
    spec: WindowSpec,
    hinstance: HINSTANCE,
    hwnd: HWND,
    creation_state: *mut MainWindowCreateState,
    tab_hwnd: HWND,
    content_bg_hwnd: HWND,
    file_menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    config: ConfigService,
    dark_theme: bool,
    theme: UiThemeResources,
    actions: LauncherActionService<SystemLauncherPlatform>,
    folder_tabs: Vec<LauncherTab>,
    selected_tab_idx: usize,
    buttons: Vec<ButtonControl>,
    visible_button_slots: Vec<VisibleButtonSlot>,
    visible_button_slot_scratch: VisibleButtonSlotScratch,
    button_drag: ButtonDragController,
    dpi_scale: f64,
    active_scan: Option<ScanWorker>,
    button_icon_worker: Option<ButtonIconWorker>,
    button_icon_worker_failed: bool,
    button_icon_generation: u64,
    button_icon_cache: LruCache<ButtonIconKey, Option<Arc<IconBitmap>>>,
    button_icon_path_cache: LruCache<String, Option<PathBuf>>,
    rendered_icon_cache: LruCache<RenderedIconKey, Arc<ButtonImageList>>,
    window_icon: Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON>,
    size_move: SizeMoveDpiState,
    close_in_progress: bool,
}

impl Win32App {
    fn new(
        spec: WindowSpec,
        hinstance: HINSTANCE,
        config: ConfigService,
        dpi_scale: f64,
    ) -> Result<Self> {
        let folder_tabs = config.get_folder_tabs();
        let dark_theme = config.dark_theme();
        let action_platform =
            SystemLauncherPlatform::with_base_dir(config.base_dir().to_path_buf());
        Ok(Self {
            spec,
            hinstance,
            hwnd: null_mut(),
            creation_state: null_mut(),
            tab_hwnd: null_mut(),
            content_bg_hwnd: null_mut(),
            file_menu: null_mut(),
            config,
            dark_theme,
            theme: UiThemeResources::new(dark_theme),
            actions: LauncherActionService::new(action_platform),
            folder_tabs,
            selected_tab_idx: 0,
            buttons: Vec::new(),
            visible_button_slots: Vec::new(),
            visible_button_slot_scratch: VisibleButtonSlotScratch::default(),
            button_drag: ButtonDragController::default(),
            dpi_scale,
            active_scan: None,
            button_icon_worker: None,
            button_icon_worker_failed: false,
            button_icon_generation: 0,
            button_icon_cache: LruCache::new(icon::DEFAULT_BUTTON_ICON_CACHE_MAX_ITEMS),
            button_icon_path_cache: LruCache::new(icon::DEFAULT_BUTTON_ICON_CACHE_MAX_ITEMS),
            rendered_icon_cache: LruCache::new(icon::DEFAULT_RENDERED_ICON_CACHE_MAX_ITEMS),
            window_icon: None,
            size_move: SizeMoveDpiState::default(),
            close_in_progress: false,
        })
    }

    fn initial_geometry(&self) -> WindowGeometry {
        let geometry = self
            .config
            .get_window_geometry_for_dpi(Some(self.dpi_scale));
        parse_window_geometry(&geometry).unwrap_or(WindowGeometry {
            width: scale_px(self.dpi_scale, 800),
            height: scale_px(self.dpi_scale, 600),
            x: None,
            y: None,
        })
    }

    fn on_create(&mut self) -> Result<()> {
        self.file_menu = create_main_menu(self.hwnd)?;
        self.tab_hwnd = create_tab_control(self.hwnd, self.hinstance)?;
        self.content_bg_hwnd = create_content_background(self.hwnd, self.hinstance)?;
        self.apply_window_title()?;
        self.apply_window_icon();
        self.ensure_icon_worker_started();
        self.refresh_dpi_from_window();
        self.apply_current_titlebar_theme();
        self.apply_theme_to_existing_controls();
        self.rebuild_tabs();
        self.layout();
        Ok(())
    }

    fn apply_window_title(&self) -> Result<()> {
        let title = wide_z(self.spec.title);
        // Safety: hwnd is the main window and title is NUL-terminated for the call.
        if unsafe { SetWindowTextW(self.hwnd, title.as_ptr()) } == 0 {
            Err(platform_error("창 제목을 설정할 수 없습니다."))
        } else {
            Ok(())
        }
    }

    fn refresh_dpi_from_window(&mut self) -> bool {
        if let Ok(hwnd) = WindowHandle::from_raw_value(self.hwnd as usize)
            && let Ok(metrics) = get_window_dpi_metrics(hwnd)
        {
            self.dpi_scale = metrics.scale;
            return true;
        }
        false
    }

    fn update_dpi_from_message(&mut self, dpi: u32) {
        self.dpi_scale = DpiMetrics::from_dpi(dpi).scale;
    }

    fn enter_size_move(&mut self) {
        self.size_move.enter();
    }

    fn exit_size_move(&mut self) {
        let exit = self.size_move.exit();
        if let Some(pending_dpi) = exit.pending_dpi {
            if !self.refresh_dpi_from_window() {
                self.update_dpi_from_message(pending_dpi);
            }
            self.layout();
            self.invalidate_after_dpi_change();
        }
    }

    fn apply_dpi_change(&mut self, dpi: u32, suggested: Option<&RECT>) {
        self.update_dpi_from_message(dpi);
        if let Some(suggested) = suggested {
            apply_suggested_dpi_rect(self.hwnd, suggested);
        }
        self.layout();
        self.invalidate_after_dpi_change();
    }

    fn defer_dpi_change_during_size_move(&mut self, dpi: u32) {
        self.size_move.defer_dpi_change(dpi);
    }

    fn should_defer_size_layout(&self) -> bool {
        self.size_move.has_pending_dpi_change()
    }

    fn invalidate_after_dpi_change(&self) {
        invalidate_window(self.hwnd);
        invalidate_window(self.tab_hwnd);
        invalidate_window(self.content_bg_hwnd);
        for button in &self.buttons {
            invalidate_window(button.hwnd);
        }
    }

    fn apply_current_titlebar_theme(&self) {
        if let Ok(hwnd) = WindowHandle::from_raw_value(self.hwnd as usize) {
            let _ = crate::platform::windows::dwm::apply_titlebar_theme(hwnd, self.dark_theme);
        }
    }

    fn apply_theme_to_existing_controls(&self) {
        apply_window_theme(self.tab_hwnd, self.dark_theme);
        invalidate_window(self.tab_hwnd);
        self.update_content_background_visibility();
        invalidate_window(self.content_bg_hwnd);
        for button in &self.buttons {
            apply_window_theme(button.hwnd, self.dark_theme);
            invalidate_window(button.hwnd);
        }
        invalidate_window(self.hwnd);
        if !self.hwnd.is_null() {
            // Safety: hwnd is this top-level window.
            unsafe { DrawMenuBar(self.hwnd) };
        }
    }

    fn update_content_background_visibility(&self) {
        if self.content_bg_hwnd.is_null() {
            return;
        }
        let command = if self.dark_theme { SW_SHOW } else { SW_HIDE };
        // Safety: content_bg_hwnd is a child window created and owned by this UI.
        unsafe { ShowWindow(self.content_bg_hwnd, command) };
    }

    fn update_theme_menu_check(&self) {
        if self.file_menu.is_null() {
            return;
        }
        let state = if self.dark_theme {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        // Safety: file_menu is owned by this window and item id is a command id.
        unsafe {
            CheckMenuItem(
                self.file_menu,
                ID_MENU_DARK_THEME as u32,
                MF_BYCOMMAND | state,
            )
        };
    }

    fn apply_window_icon(&mut self) {
        let Some(icon) = load_icon_from_resource(self.hinstance).or_else(|| {
            self.resolve_icon_path()
                .and_then(|icon_path| load_icon_from_file(&icon_path))
        }) else {
            return;
        };
        // Safety: hwnd is valid and WM_SETICON stores the icon handle for the
        // lifetime of the window. The state owns and destroys it on teardown.
        unsafe {
            SendMessageW(self.hwnd, WM_SETICON, ICON_BIG as WPARAM, icon as LPARAM);
            SendMessageW(self.hwnd, WM_SETICON, ICON_SMALL as WPARAM, icon as LPARAM);
        }
        self.window_icon = Some(icon);
    }

    fn resolve_icon_path(&self) -> Option<PathBuf> {
        let file_name = self.spec.icon_ico_file_name;
        let mut dirs = vec![self.config.base_dir().to_path_buf()];
        if let Ok(current_dir) = std::env::current_dir() {
            dirs.push(current_dir);
        }
        if let Ok(exe_path) = std::env::current_exe()
            && let Some(exe_dir) = exe_path.parent()
        {
            dirs.push(exe_dir.to_path_buf());
        }

        dirs.into_iter()
            .map(|dir| dir.join(file_name))
            .find(|path| path.is_file())
    }

    fn rebuild_tabs(&mut self) {
        clear_buttons(&mut self.buttons);
        // Safety: tab_hwnd is a valid tab control.
        unsafe { SendMessageW(self.tab_hwnd, TCM_DELETEALLITEMS, 0, 0) };
        for (index, tab) in self.folder_tabs.iter().enumerate() {
            let title = if tab.title.trim().is_empty() {
                format!("Tab {}", index + 1)
            } else {
                tab.title.clone()
            };
            let mut title_wide = wide_z(&title);
            let mut item = TCITEMW {
                mask: TCIF_TEXT,
                pszText: title_wide.as_mut_ptr(),
                ..TCITEMW::default()
            };
            // Safety: item references title_wide only for the duration of the
            // message, and the tab control copies the text.
            unsafe {
                SendMessageW(
                    self.tab_hwnd,
                    TCM_INSERTITEMW,
                    index as WPARAM,
                    (&mut item as *mut TCITEMW) as LPARAM,
                )
            };
        }

        if self.folder_tabs.is_empty() {
            self.selected_tab_idx = 0;
        } else if self.selected_tab_idx >= self.folder_tabs.len() {
            self.selected_tab_idx = self.folder_tabs.len() - 1;
        }
        // Safety: tab_hwnd is a valid tab control.
        unsafe {
            SendMessageW(
                self.tab_hwnd,
                TCM_SETCURSEL,
                self.selected_tab_idx as WPARAM,
                0,
            )
        };
        self.render_current_tab();
        self.update_menu_state();
    }

    fn update_menu_state(&self) {
        if self.file_menu.is_null() {
            return;
        }
        let availability = self.current_menu_availability();
        for item in MAIN_MENU_SECTIONS.iter().flat_map(|section| section.iter()) {
            self.enable_menu_item(
                menu_command_id(item.command),
                availability.is_command_enabled(item.command),
            );
        }
        self.update_theme_menu_check();
    }

    fn current_menu_availability(&self) -> MenuActionAvailability {
        menu_action_availability(
            &self.folder_tabs,
            self.selected_tab_idx,
            self.active_scan.is_none(),
        )
    }

    fn enable_menu_item(&self, item_id: usize, enabled: bool) {
        let state = if enabled { MF_ENABLED } else { MF_GRAYED };
        // Safety: file_menu is owned by this window and item ids are command ids.
        unsafe { EnableMenuItem(self.file_menu, item_id as u32, MF_BYCOMMAND | state) };
    }

    fn render_current_tab(&mut self) {
        self.button_icon_generation = self.button_icon_generation.wrapping_add(1);
        self.button_drag.reset();
        clear_buttons(&mut self.buttons);
        self.visible_button_slots.clear();

        let icon_worker_error = {
            let Some(tab) = self.folder_tabs.get(self.selected_tab_idx) else {
                return;
            };

            collect_visible_button_slots(
                tab,
                &mut self.visible_button_slots,
                &mut self.visible_button_slot_scratch,
            );

            let parent_hwnd = self.hwnd;
            let hinstance = self.hinstance;
            let dark_theme = self.dark_theme;
            let selected_tab_idx = self.selected_tab_idx;
            let icon_generation = self.button_icon_generation;
            let base_dir = self.config.base_dir();
            let platform = self.actions.platform();
            let expand_path = |value: &str| platform.expand_path(value);
            let mut icon_path_resolver = None;
            let visible_button_slots = &self.visible_button_slots;
            let mut icon_render = ButtonIconRenderState {
                hwnd: parent_hwnd,
                dpi_scale: self.dpi_scale,
                generation: icon_generation,
                base_dir,
                button_icon_worker: &mut self.button_icon_worker,
                button_icon_worker_failed: &mut self.button_icon_worker_failed,
                button_icon_cache: &mut self.button_icon_cache,
                button_icon_path_cache: &mut self.button_icon_path_cache,
                rendered_icon_cache: &mut self.rendered_icon_cache,
                icon_worker_error: None,
            };
            let buttons = &mut self.buttons;

            for (ordinal, slot) in visible_button_slots.iter().enumerate() {
                let Some(button) = tab.buttons.get(slot.button_idx) else {
                    continue;
                };
                let label = button_label(button);
                let label_wide = wide_z(&label);
                let control_id = ID_BUTTON_BASE + ordinal;
                // Safety: class name and label are valid for the call. The parent
                // owns the returned child HWND and destroys it during re-render.
                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        w!("BUTTON"),
                        label_wide.as_ptr(),
                        WS_CHILD
                            | WS_VISIBLE
                            | WS_TABSTOP
                            | (BS_OWNERDRAW as u32)
                            | (BS_MULTILINE as u32),
                        0,
                        0,
                        10,
                        10,
                        parent_hwnd,
                        control_id as _,
                        hinstance,
                        null(),
                    )
                };
                if hwnd.is_null() {
                    continue;
                }
                apply_window_theme(hwnd, dark_theme);
                install_button_drag_subclass(hwnd, parent_hwnd);
                let mut control = ButtonControl {
                    hwnd,
                    tab_idx: selected_tab_idx,
                    button_idx: slot.button_idx,
                    slot_idx: slot.slot_idx,
                    icon_generation,
                    icon_key: None,
                    image_list: None,
                };
                request_or_apply_button_icon(
                    &mut icon_render,
                    &mut control,
                    tab.id.as_str(),
                    button,
                    &mut icon_path_resolver,
                    &expand_path,
                );
                buttons.push(control);
            }

            icon_render.icon_worker_error
        };

        if let Some(message) = icon_worker_error {
            self.show_error("Button Icons", &message);
        }
        self.layout();
    }

    fn ensure_icon_worker_started(&mut self) {
        if self.button_icon_worker.is_some()
            || self.button_icon_worker_failed
            || self.hwnd.is_null()
        {
            return;
        }
        let hwnd_value = self.hwnd as usize;
        match icon::spawn_button_icon_worker(icon::DEFAULT_BUTTON_ICON_CACHE_MAX_ITEMS, move || {
            // Safety: hwnd_value was captured from this UI thread after the
            // window was created. Posting can fail during teardown, which is
            // harmless because stale worker results are dropped with state.
            unsafe { PostMessageW(hwnd_value as HWND, WM_ICON_COMPLETE, 0, 0) };
        }) {
            Ok(worker) => {
                self.button_icon_worker = Some(worker);
            }
            Err(error) => {
                self.button_icon_worker_failed = true;
                self.show_error("Button Icons", &error.user_message());
            }
        }
    }

    fn process_icon_results(&mut self) {
        let mut results = Vec::new();
        if let Some(worker) = &self.button_icon_worker {
            while let Some(result) = worker.try_recv() {
                results.push(result);
            }
        }

        if results.len() > 1 {
            let button_index = ButtonIconResultButtonIndex::from_buttons(&self.buttons);
            for result in results {
                self.apply_icon_result(result, |_, generation, button_key| {
                    button_index.position_for(generation, button_key)
                });
            }
        } else {
            for result in results {
                self.apply_icon_result(result, find_icon_result_button_position);
            }
        }
    }

    fn apply_icon_result<F>(&mut self, result: ButtonIconResult, find_position: F)
    where
        F: FnOnce(&[ButtonControl], u64, &ButtonIconKey) -> Option<usize>,
    {
        self.button_icon_cache
            .insert(result.button_key.clone(), result.bitmap.clone());
        let Some(bitmap) = result.bitmap else {
            return;
        };
        let position = find_position(&self.buttons, result.generation, &result.button_key);
        let Some(position) = position else {
            return;
        };
        let hwnd = self.buttons[position].hwnd;
        let target_size = result.button_key.target_size();
        let Some(image_list) =
            self.get_or_create_button_image_list(result.source_key, bitmap, target_size)
        else {
            return;
        };
        self.buttons[position].image_list = Some(image_list);
        invalidate_window(hwnd);
    }

    fn get_or_create_button_image_list(
        &mut self,
        source_key: IconCacheKey,
        bitmap: Arc<IconBitmap>,
        target_size: u32,
    ) -> Option<Arc<ButtonImageList>> {
        get_or_create_button_image_list(
            &mut self.rendered_icon_cache,
            source_key,
            bitmap,
            target_size,
        )
    }

    fn layout(&self) {
        if self.hwnd.is_null() || self.tab_hwnd.is_null() {
            return;
        }
        let mut client = RECT::default();
        // Safety: hwnd is valid and client points to writable memory.
        if unsafe { GetClientRect(self.hwnd, &mut client) } == 0 {
            return;
        }
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - client.top).max(1);
        // Safety: child HWND is valid.
        unsafe { MoveWindow(self.tab_hwnd, 0, 0, width, height, 1) };

        let mut content = client;
        // Safety: TCM_ADJUSTRECT writes into content and does not retain it.
        unsafe {
            SendMessageW(
                self.tab_hwnd,
                TCM_ADJUSTRECT,
                0,
                (&mut content as *mut RECT) as LPARAM,
            )
        };
        let pad = scale_px(self.dpi_scale, 6);
        content.left += pad;
        content.top += pad;
        content.right -= pad;
        content.bottom -= pad;
        self.layout_content_background(content);

        let Some(tab) = self.current_tab() else {
            return;
        };
        let cols = usize::from(tab.cols.max(1));
        let rows = required_rows(tab, &self.buttons, cols);
        if rows == 0 || cols == 0 {
            return;
        }
        let Ok(cols_i32) = i32::try_from(cols) else {
            return;
        };
        let Ok(rows_i32) = i32::try_from(rows) else {
            return;
        };
        let content_width = (content.right - content.left).max(1);
        let content_height = (content.bottom - content.top).max(1);
        let gap = if self.dark_theme {
            0
        } else {
            scale_px(self.dpi_scale, 6)
        };
        let width_gaps = gap.saturating_mul(cols_i32.saturating_sub(1));
        let height_gaps = gap.saturating_mul(rows_i32.saturating_sub(1));
        let cell_width =
            (content_width.saturating_sub(width_gaps) / cols_i32).max(scale_px(self.dpi_scale, 48));
        let cell_height = (content_height.saturating_sub(height_gaps) / rows_i32)
            .max(scale_px(self.dpi_scale, 36));

        for button in &self.buttons {
            let row = button.slot_idx / cols;
            let col = button.slot_idx % cols;
            let Ok(row_i32) = i32::try_from(row) else {
                continue;
            };
            let Ok(col_i32) = i32::try_from(col) else {
                continue;
            };
            let x = content
                .left
                .saturating_add(cell_width.saturating_add(gap).saturating_mul(col_i32));
            let y = content
                .top
                .saturating_add(cell_height.saturating_add(gap).saturating_mul(row_i32));
            // Safety: button hwnd is a valid child window. Raising it keeps
            // the dark content background behind the launcher buttons.
            unsafe {
                SetWindowPos(
                    button.hwnd,
                    HWND_TOP,
                    x,
                    y,
                    cell_width,
                    cell_height,
                    SWP_NOACTIVATE,
                )
            };
        }
    }

    fn layout_content_background(&self, content: RECT) {
        if self.content_bg_hwnd.is_null() {
            return;
        }
        let width = rect_span(content.left, content.right).max(1);
        let height = rect_span(content.top, content.bottom).max(1);
        // Safety: content_bg_hwnd is a child window created by this UI.
        // Raising it above the native tab control covers the tab page area
        // that common controls otherwise erase with a light system color.
        unsafe {
            SetWindowPos(
                self.content_bg_hwnd,
                HWND_TOP,
                content.left,
                content.top,
                width,
                height,
                SWP_NOACTIVATE,
            );
        }
        self.update_content_background_visibility();
    }

    fn erase_dark_background(&self, hdc: HDC) -> bool {
        if !self.dark_theme || hdc.is_null() {
            return false;
        }
        let Some(brush) = self.theme.background_brush() else {
            return false;
        };
        let mut rect = RECT::default();
        // Safety: hwnd is valid and rect is writable.
        if unsafe { GetClientRect(self.hwnd, &mut rect) } == 0 {
            return false;
        }
        // Safety: hdc belongs to WM_ERASEBKGND and brush is owned by self.
        unsafe { FillRect(hdc, &rect, brush) != 0 }
    }

    fn dark_control_brush_result(&self, hdc: HDC) -> Option<LRESULT> {
        if !self.dark_theme || hdc.is_null() {
            return None;
        }
        let brush = self.theme.background_brush()?;
        // Safety: hdc belongs to a WM_CTLCOLOR* message and color values are plain COLORREFs.
        unsafe {
            SetTextColor(hdc, DARK_TEXT);
            SetBkColor(hdc, DARK_BACKGROUND);
        }
        Some(brush as LRESULT)
    }

    fn draw_owner_draw_button(&self, draw: &DRAWITEMSTRUCT) -> bool {
        if draw.CtlType != ODT_BUTTON || draw.hDC.is_null() {
            return false;
        }
        let Some(control) = self
            .buttons
            .iter()
            .find(|control| control.hwnd == draw.hwndItem)
        else {
            return false;
        };
        let Some(fill_brush) = self.theme.button_brush(draw.itemState) else {
            return false;
        };

        let outer_rect = draw.rcItem;
        let Some(background_brush) = self.theme.background_brush() else {
            return false;
        };
        // Safety: draw contains the HDC and rect supplied by WM_DRAWITEM.
        unsafe {
            FillRect(draw.hDC, &outer_rect, background_brush);
        }

        let inset = if self.dark_theme {
            scale_px(self.dpi_scale, 3)
        } else {
            0
        };
        let mut rect = inset_rect(outer_rect, inset);
        // Safety: draw contains the HDC and rect supplied by WM_DRAWITEM.
        unsafe {
            FillRect(draw.hDC, &rect, fill_brush);
            if let Some(border) = self.theme.border_brush() {
                FrameRect(draw.hDC, &rect, border);
            }
        }

        let pressed_offset = if draw.itemState & ODS_SELECTED != 0 {
            scale_px(self.dpi_scale, 1)
        } else {
            0
        };
        rect.left += pressed_offset;
        rect.right += pressed_offset;
        rect.top += pressed_offset;
        rect.bottom += pressed_offset;

        let padding = scale_px(self.dpi_scale, 6);
        let content_rect = RECT {
            left: rect.left + padding,
            top: rect.top + padding,
            right: rect.right - padding,
            bottom: rect.bottom - padding,
        };

        let label = self.button_control_label(control);
        let label = wide_z(&label);
        let text_width = rect_span(content_rect.left, content_rect.right);
        let measured_text_height = measure_button_text_height(draw.hDC, &label, text_width);
        let icon_size = control.image_list.as_ref().map(|_| {
            control
                .icon_key
                .as_ref()
                .and_then(|key| i32::try_from(key.target_size()).ok())
                .unwrap_or_else(|| button_icon_target_size(self.dpi_scale) as i32)
        });
        let icon_gap = scale_px(self.dpi_scale, 4);
        let reserved_icon_height = icon_size.unwrap_or(0);
        let reserved_gap = if icon_size.is_some() && measured_text_height > 0 {
            icon_gap
        } else {
            0
        };
        let max_text_height = rect_span(content_rect.top, content_rect.bottom)
            .saturating_sub(reserved_icon_height)
            .saturating_sub(reserved_gap);
        let text_height = measured_text_height.min(max_text_height);
        let layout = button_content_layout(content_rect, icon_size, icon_gap, text_height);

        if let Some(image_list) = &control.image_list
            && let Some(icon_rect) = layout.icon_rect
        {
            // Safety: image_list owns a valid HIMAGELIST and draw.hDC is valid for drawing.
            unsafe {
                ImageList_Draw(
                    image_list.handle,
                    0,
                    draw.hDC,
                    icon_rect.left,
                    icon_rect.top,
                    ILD_NORMAL,
                )
            };
        }

        let mut text_rect = layout.text_rect;
        let text_color = if draw.itemState & ODS_DISABLED != 0 {
            self.theme.disabled_text_color()
        } else {
            self.theme.text_color()
        };
        // Safety: draw.hDC is valid during WM_DRAWITEM, and label/text_rect live for the call.
        unsafe {
            SetBkMode(draw.hDC, TRANSPARENT as i32);
            SetTextColor(draw.hDC, text_color);
            DrawTextW(
                draw.hDC,
                label.as_ptr(),
                -1,
                &mut text_rect,
                DT_CENTER | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            if draw.itemState & ODS_FOCUS != 0 {
                let focus_rect = RECT {
                    left: rect.left + scale_px(self.dpi_scale, 3),
                    top: rect.top + scale_px(self.dpi_scale, 3),
                    right: rect.right - scale_px(self.dpi_scale, 3),
                    bottom: rect.bottom - scale_px(self.dpi_scale, 3),
                };
                DrawFocusRect(draw.hDC, &focus_rect);
            }
        }
        true
    }

    fn button_control_label(&self, control: &ButtonControl) -> String {
        self.folder_tabs
            .get(control.tab_idx)
            .and_then(|tab| tab.buttons.get(control.button_idx))
            .map(button_label)
            .unwrap_or_default()
    }

    fn on_command(&mut self, command_id: usize, notification: u16, control_hwnd: HWND) {
        // Control ids in WM_COMMAND are limited to the low word; use the HWND
        // path for launcher buttons so truncated ids cannot collide.
        if !control_hwnd.is_null() {
            if notification == BN_CLICKED as u16 {
                self.click_button(control_hwnd);
            }
            return;
        }

        let Some(command) = menu_command_from_id(command_id) else {
            return;
        };
        let availability = self.current_menu_availability();
        dispatch_menu_command_if_enabled(command, &availability, self);
    }

    fn toggle_dark_theme(&mut self) {
        if !self.finish_config_saves_before_sync_change() {
            return;
        }
        let enabled = !self.dark_theme;
        match self.config.set_dark_theme(enabled) {
            Ok(()) => {
                self.dark_theme = enabled;
                self.theme = UiThemeResources::new(enabled);
                self.apply_current_titlebar_theme();
                self.rebuild_tabs();
                self.apply_theme_to_existing_controls();
                self.update_theme_menu_check();
            }
            Err(error) => self.show_error("Dark Theme", &error.user_message()),
        }
    }

    fn add_folder_tab(&mut self) {
        if self.active_scan.is_some() {
            self.show_info("Scan", "이미 폴더 스캔이 진행 중입니다.");
            return;
        }
        let owner = WindowHandle::from_raw_value(self.hwnd as usize).ok();
        let selected = match crate::platform::windows::dialogs::pick_folder(owner, "Add Folder Tab")
        {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                self.show_error("Add Folder Tab", &error.user_message());
                return;
            }
        };
        let folder_path = selected.to_string_lossy().into_owned();
        if let Some(tab_idx) =
            crate::app::folder_tabs::find_tab_index_by_folder(&self.folder_tabs, &folder_path)
        {
            self.selected_tab_idx = tab_idx;
            self.rebuild_tabs();
            return;
        }
        self.start_scan(ScanRequest::AddFolder { folder_path });
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
        let owner = WindowHandle::from_raw_value(self.hwnd as usize).ok();
        let selected = match crate::platform::windows::dialogs::pick_folder(owner, "Set Tab Folder")
        {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                self.show_error("Set Tab Folder", &error.user_message());
                return;
            }
        };
        let folder_path = selected.to_string_lossy().into_owned();
        let duplicate_idx =
            crate::app::folder_tabs::find_tab_index_by_folder(&self.folder_tabs, &folder_path);
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
        self.start_scan(ScanRequest::SetFolder {
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
            TabLayoutDefaults {
                rows: MANUAL_DEFAULT_BUTTON_ROWS,
                cols: MANUAL_DEFAULT_BUTTON_COLS,
            }
        } else {
            TabLayoutDefaults {
                rows: DEFAULT_BUTTON_ROWS,
                cols: DEFAULT_BUTTON_COLS,
            }
        };
        let Some(layout) = tab_layout_dialog(
            self.hwnd,
            self.hinstance,
            tab.rows,
            tab.cols,
            defaults,
            self.dpi_scale,
        ) else {
            return;
        };
        let selected_tab_idx = self.selected_tab_idx;
        if let Some((next_tabs, outcome)) = self.mutate_tabs_for_save(
            |tabs| update_tab_layout(tabs, selected_tab_idx, layout.rows, layout.cols),
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
        let Some(title) = text_input_dialog(
            self.hwnd,
            self.hinstance,
            "Rename Tab",
            "New tab title:",
            &tab.title,
            self.dpi_scale,
        ) else {
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
        let title = if tab.title.trim().is_empty() {
            format!("Tab {}", self.selected_tab_idx + 1)
        } else {
            tab.title.clone()
        };
        if !confirm_message(
            self.hwnd,
            "Delete Tab",
            &format!("Delete current tab '{title}'?"),
            MB_ICONWARNING | MB_DEFBUTTON2,
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
        let tab_id = tab.id.clone();
        let folder_path = tab.folder_path.clone();
        let known_signature = tab.scan_signature.clone();
        self.start_scan(ScanRequest::Refresh {
            tab_id,
            folder_path,
            known_signature,
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
        if !confirm_message(
            self.hwnd,
            "Reset Tab",
            "Reset this tab? Existing button settings will be rebuilt from folder scan.",
            MB_ICONWARNING | MB_DEFBUTTON2,
        ) {
            return;
        }
        let tab_id = tab.id.clone();
        let folder_path = tab.folder_path.clone();
        let (known_signature, known_items) = known_scan_options_from_tab(tab, true);
        self.start_scan(ScanRequest::Reset {
            tab_id,
            folder_path,
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
        let Some(item_ids) = hidden_items_dialog(self.hwnd, self.hinstance, &tab, self.dpi_scale)
        else {
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
        let selected_tab_idx = self.selected_tab_idx;
        let mut next_tabs = std::mem::take(&mut self.folder_tabs);
        let outcome = tab_actions::move_tab(&mut next_tabs, selected_tab_idx, direction);
        if !outcome.moved {
            self.folder_tabs = next_tabs;
            return;
        }
        self.persist_tabs(next_tabs, outcome.focus_tab_idx);
    }

    fn select_relative_tab(&mut self, delta: isize) {
        if self.active_scan.is_some() {
            return;
        }
        if self.folder_tabs.is_empty() {
            return;
        }
        let current = self.selected_tab_idx as isize;
        let requested = current + delta;
        if requested < 0 {
            return;
        }
        let requested = requested as usize;
        if requested >= self.folder_tabs.len() {
            return;
        }
        self.selected_tab_idx = requested;
        self.rebuild_tabs();
    }

    fn click_button(&mut self, control_hwnd: HWND) {
        let Some(control) = self
            .buttons
            .iter()
            .find(|button| button.hwnd == control_hwnd)
        else {
            return;
        };
        let Some(tab) = self.folder_tabs.get(control.tab_idx) else {
            return;
        };
        let Some(button) = tab.buttons.get(control.button_idx) else {
            return;
        };
        let request = self.actions.prepare_button_action(button);
        for message in &request.pre_messages {
            self.show_user_message(message);
        }
        let messages = self.actions.execute_button_action(&request);
        self.show_user_messages(&messages);
    }

    fn on_button_drag_event(&mut self, event: ButtonDragEvent, source_hwnd: HWND) -> bool {
        match event {
            ButtonDragEvent::Down => {
                self.start_button_drag(source_hwnd);
                false
            }
            ButtonDragEvent::Move => self.update_button_drag(source_hwnd),
            ButtonDragEvent::Up => self.finish_button_drag(source_hwnd),
            ButtonDragEvent::Cancel => self.cancel_button_drag(source_hwnd),
        }
    }

    fn start_button_drag(&mut self, source_hwnd: HWND) {
        self.button_drag.reset();
        if self.active_scan.is_some() || self.close_in_progress {
            return;
        }
        let Some(source) = self
            .buttons
            .iter()
            .find(|button| button.hwnd == source_hwnd)
            .map(button_drag_endpoint)
        else {
            return;
        };
        let Some(point) = cursor_position() else {
            return;
        };
        self.button_drag.start(source, point);
    }

    fn update_button_drag(&mut self, source_hwnd: HWND) -> bool {
        let Some(point) = cursor_position() else {
            return false;
        };
        self.button_drag.update(
            button_control_key(source_hwnd),
            point,
            scale_px(self.dpi_scale, BUTTON_DRAG_THRESHOLD_PX),
        )
    }

    fn finish_button_drag(&mut self, source_hwnd: HWND) -> bool {
        let Some(source) = self.button_drag.finish(button_control_key(source_hwnd)) else {
            return false;
        };
        if let Some(target) = self.button_drop_target_at_cursor() {
            self.drop_button_on_button(source, target);
        }
        true
    }

    fn cancel_button_drag(&mut self, source_hwnd: HWND) -> bool {
        self.button_drag.cancel(button_control_key(source_hwnd))
    }

    fn button_drop_target_at_cursor(&self) -> Option<ButtonDragEndpoint> {
        let point = cursor_position()?;
        self.buttons.iter().find_map(|button| {
            let mut rect = RECT::default();
            // Safety: button hwnd is a live child window and rect is writable.
            if unsafe { GetWindowRect(button.hwnd, &mut rect) } == 0 {
                return None;
            }
            point_in_rect(point, &rect).then(|| button_drag_endpoint(button))
        })
    }

    fn drop_button_on_button(&mut self, source: ButtonDragEndpoint, target: ButtonDragEndpoint) {
        if source.tab_idx() != target.tab_idx() || source.tab_idx() != self.selected_tab_idx {
            return;
        }

        let source_tab_idx = source.tab_idx();
        let source_button_idx = source.button_idx();
        let source_slot_idx = source.slot_idx();
        let target_button_idx = target.button_idx();
        let target_slot_idx = target.slot_idx();
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

    fn show_button_context_menu(&mut self, child_hwnd: HWND, x: i32, y: i32) {
        let Some(control) = self
            .buttons
            .iter()
            .find(|button| button.hwnd == child_hwnd)
            .cloned()
        else {
            return;
        };
        let Some(tab) = self.folder_tabs.get(control.tab_idx) else {
            return;
        };
        let Some(button) = tab.buttons.get(control.button_idx) else {
            return;
        };
        let menu = match create_context_menu(tab, button) {
            Ok(menu) => menu,
            Err(error) => {
                self.show_error("Context Menu", &error.user_message());
                return;
            }
        };
        let (x, y) = if x == -1 && y == -1 {
            button_window_center(child_hwnd).unwrap_or((0, 0))
        } else {
            (x, y)
        };
        let command = if let Some(command) = debug_context_menu_command_override() {
            Some(command)
        } else {
            // Safety: menu is a popup menu owned in this scope and hwnd is valid.
            let command = unsafe {
                TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    x,
                    y,
                    0,
                    self.hwnd,
                    null(),
                )
            };
            context_menu_command_from_id(command as usize)
        };
        // Safety: menu was created by CreatePopupMenu in this scope.
        unsafe { DestroyMenu(menu) };
        let Some(command) = command else {
            return;
        };
        if !button_context_command_enabled(command, tab, button) {
            return;
        }
        match command {
            ButtonContextCommand::Edit => self.edit_button(control.tab_idx, control.button_idx),
            ButtonContextCommand::OpenInExplorer => {
                self.open_button_in_explorer(control.tab_idx, control.button_idx)
            }
            ButtonContextCommand::Hide => self.hide_button(control.tab_idx, control.button_idx),
        }
    }

    fn edit_button(&mut self, tab_idx: usize, button_idx: usize) {
        if !self.finish_config_saves_before_sync_change() {
            return;
        }
        let initial = self.config.get_button_info(tab_idx, button_idx);
        let Some(updated) = edit_button_dialog(self.hwnd, self.hinstance, initial, self.dpi_scale)
        else {
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
            .actions
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

    fn start_scan(&mut self, request: ScanRequest) {
        let hwnd_value = self.hwnd as usize;
        let worker = match ScanWorker::spawn(request, hwnd_value, WM_SCAN_COMPLETE) {
            Ok(worker) => worker,
            Err(error) => {
                self.show_error(
                    "Scan",
                    &format!("폴더 스캔 worker를 시작할 수 없습니다: {error}"),
                );
                return;
            }
        };
        self.active_scan = Some(worker);
        self.update_menu_state();
    }

    fn complete_scan(&mut self) {
        let Some(mut worker) = self.active_scan.take() else {
            return;
        };
        let Some(message) = worker.take_result() else {
            self.active_scan = Some(worker);
            return;
        };
        worker.join();
        self.update_menu_state();
        match message.result {
            Ok(result) => self.apply_scan_result(message.request, result),
            Err(error) => self.show_error("Scan", &error.user_message()),
        }
    }

    fn apply_scan_result(&mut self, request: ScanRequest, result: FolderScanResult) {
        if result.cancelled {
            return;
        }
        match request {
            ScanRequest::AddFolder { folder_path } => {
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
            ScanRequest::SetFolder {
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
            ScanRequest::Refresh { tab_id, .. } => {
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
            ScanRequest::Reset { tab_id, .. } => {
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
        let hwnd_value = self.hwnd as usize;
        match self.config.set_folder_tabs_deferred(next_tabs, move || {
            // Safety: hwnd_value belongs to the UI thread. Posting can fail
            // during teardown; pending results are drained during close/drop.
            unsafe { PostMessageW(hwnd_value as HWND, WM_CONFIG_SAVE_COMPLETE, 0, 0) };
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

    fn finish_config_saves_before_sync_change(&mut self) -> bool {
        let statuses = self.config.finish_deferred_save_work();
        let success = statuses
            .iter()
            .all(|status| status.success || status.superseded);
        self.handle_config_save_statuses(statuses);
        success
    }

    fn request_close(&mut self) {
        // Safety: hwnd is this window and posting WM_CLOSE only queues a normal
        // close request.
        unsafe { PostMessageW(self.hwnd, WM_CLOSE, 0, 0) };
    }

    fn close(&mut self) {
        if self.close_in_progress {
            return;
        }
        self.close_in_progress = true;
        self.cancel_worker();
        self.shutdown_icon_worker();
        self.finish_config_saves_for_close();
        self.save_window_geometry_for_close();
        self.suspend_redraw_for_destroy();
        // Safety: hwnd is this top-level window.
        unsafe { DestroyWindow(self.hwnd) };
    }

    fn suspend_redraw_for_destroy(&self) {
        set_redraw_enabled(self.content_bg_hwnd, false);
        set_redraw_enabled(self.tab_hwnd, false);
        for button in &self.buttons {
            set_redraw_enabled(button.hwnd, false);
        }
        set_redraw_enabled(self.hwnd, false);
    }

    fn cancel_worker(&mut self) {
        if let Some(worker) = self.active_scan.take() {
            worker.cancel_without_join();
        }
    }

    fn shutdown_icon_worker(&mut self) {
        if let Some(worker) = self.button_icon_worker.take() {
            worker.shutdown_without_join();
        }
    }

    fn finish_config_saves_for_close(&mut self) {
        let statuses = self.config.shutdown_deferred_save_worker();
        self.handle_config_save_statuses(statuses);
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

    fn save_window_geometry_for_close(&mut self) {
        let mut rect = RECT::default();
        // Safety: hwnd is valid during close handling and rect is writable.
        if unsafe { GetWindowRect(self.hwnd, &mut rect) } == 0 {
            return;
        }
        let geometry = WindowGeometry::from_rect(rect).to_config_string();
        if let Err(error) = self
            .config
            .save_window_geometry_with_dpi(geometry, Some(self.dpi_scale))
        {
            self.show_warning("Configuration Save", &error.user_message());
        }
    }

    fn current_tab(&self) -> Option<&LauncherTab> {
        self.folder_tabs.get(self.selected_tab_idx)
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
        show_message(self.hwnd, title, message, MB_OK | MB_ICONINFORMATION);
    }

    fn show_warning(&self, title: &str, message: &str) {
        show_message(self.hwnd, title, message, MB_OK | MB_ICONWARNING);
    }

    fn show_error(&self, title: &str, message: &str) {
        show_message(self.hwnd, title, message, MB_OK | MB_ICONERROR);
    }

    fn show_about(&self) {
        about_dialog(self.hwnd, self.hinstance, self.dpi_scale);
    }
}

impl MenuCommandHandler for Win32App {
    fn add_folder_tab(&mut self) {
        Win32App::add_folder_tab(self);
    }

    fn add_manual_tab(&mut self) {
        Win32App::add_manual_tab(self);
    }

    fn set_current_tab_folder(&mut self) {
        Win32App::set_current_tab_folder(self);
    }

    fn edit_current_tab_layout(&mut self) {
        Win32App::edit_current_tab_layout(self);
    }

    fn rename_current_tab(&mut self) {
        Win32App::rename_current_tab(self);
    }

    fn delete_current_tab(&mut self) {
        Win32App::delete_current_tab(self);
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
        Win32App::sort_current_tab(self);
    }

    fn refresh_current_tab(&mut self) {
        Win32App::refresh_current_tab(self);
    }

    fn reset_current_tab(&mut self) {
        Win32App::reset_current_tab(self);
    }

    fn manage_hidden_items(&mut self) {
        Win32App::manage_hidden_items(self);
    }

    fn toggle_dark_theme(&mut self) {
        Win32App::toggle_dark_theme(self);
    }

    fn exit(&mut self) {
        self.request_close();
    }

    fn show_about(&mut self) {
        Win32App::show_about(self);
    }
}

#[derive(Debug)]
struct MainWindowCreateState {
    app: *mut Win32App,
    destroyed: bool,
}

impl Drop for Win32App {
    fn drop(&mut self) {
        self.cancel_worker();
        self.shutdown_icon_worker();
        let _ = self.config.shutdown_deferred_save_worker();
        clear_buttons(&mut self.buttons);
        if let Some(icon) = self.window_icon.take() {
            // Safety: this state owns the icon handle returned by LoadImageW.
            unsafe { DestroyIcon(icon) };
        }
    }
}

#[derive(Debug, Clone)]
struct ButtonControl {
    hwnd: HWND,
    tab_idx: usize,
    button_idx: usize,
    slot_idx: usize,
    icon_generation: u64,
    icon_key: Option<ButtonIconKey>,
    image_list: Option<Arc<ButtonImageList>>,
}

#[derive(Debug)]
struct ButtonIconResultButtonIndex {
    by_key: HashMap<ButtonIconKey, ButtonIconResultButtonEntry>,
}

impl ButtonIconResultButtonIndex {
    fn from_buttons(buttons: &[ButtonControl]) -> Self {
        let mut by_key = HashMap::with_capacity(buttons.len());
        for (position, control) in buttons.iter().enumerate() {
            let Some(icon_key) = control.icon_key.as_ref() else {
                continue;
            };
            let entry =
                by_key
                    .entry(icon_key.clone())
                    .or_insert_with(|| ButtonIconResultButtonEntry {
                        fallback_position: position,
                        by_generation: HashMap::new(),
                    });
            entry
                .by_generation
                .entry(control.icon_generation)
                .or_insert(position);
        }
        Self { by_key }
    }

    fn position_for(&self, generation: u64, button_key: &ButtonIconKey) -> Option<usize> {
        let entry = self.by_key.get(button_key)?;
        entry
            .by_generation
            .get(&generation)
            .copied()
            .or(Some(entry.fallback_position))
    }
}

#[derive(Debug)]
struct ButtonIconResultButtonEntry {
    fallback_position: usize,
    by_generation: HashMap<u64, usize>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct SizeMoveDpiState {
    in_loop: bool,
    pending_dpi: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SizeMoveExit {
    pending_dpi: Option<u32>,
}

impl SizeMoveDpiState {
    fn enter(&mut self) {
        self.in_loop = true;
        self.pending_dpi = None;
    }

    fn defer_dpi_change(&mut self, dpi: u32) {
        if self.in_loop {
            self.pending_dpi = Some(DpiMetrics::from_dpi(dpi).dpi);
        }
    }

    fn has_pending_dpi_change(&self) -> bool {
        self.in_loop && self.pending_dpi.is_some()
    }

    fn exit(&mut self) -> SizeMoveExit {
        let exit = SizeMoveExit {
            pending_dpi: self.pending_dpi,
        };
        self.in_loop = false;
        self.pending_dpi = None;
        exit
    }
}

#[derive(Debug, Clone, Copy)]
enum ButtonDragEvent {
    Down,
    Move,
    Up,
    Cancel,
}

impl ButtonDragEvent {
    fn from_wparam(value: WPARAM) -> Option<Self> {
        match value {
            BUTTON_DRAG_EVENT_DOWN => Some(Self::Down),
            BUTTON_DRAG_EVENT_MOVE => Some(Self::Move),
            BUTTON_DRAG_EVENT_UP => Some(Self::Up),
            BUTTON_DRAG_EVENT_CANCEL => Some(Self::Cancel),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct ButtonImageList {
    handle: HIMAGELIST,
}

#[derive(Debug, Clone, Copy)]
struct ControlRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Debug)]
struct UiThemeResources {
    dark: bool,
    background: Option<SolidBrush>,
    button: Option<SolidBrush>,
    button_hot: Option<SolidBrush>,
    button_pressed: Option<SolidBrush>,
    border: Option<SolidBrush>,
}

impl UiThemeResources {
    fn new(dark: bool) -> Self {
        if !dark {
            return Self {
                dark,
                background: SolidBrush::new(LIGHT_BACKGROUND),
                button: SolidBrush::new(LIGHT_BUTTON),
                button_hot: SolidBrush::new(LIGHT_BUTTON_HOT),
                button_pressed: SolidBrush::new(LIGHT_BUTTON_PRESSED),
                border: SolidBrush::new(LIGHT_BORDER),
            };
        }

        Self {
            dark,
            background: SolidBrush::new(DARK_BACKGROUND),
            button: SolidBrush::new(DARK_BUTTON),
            button_hot: SolidBrush::new(DARK_BUTTON_HOT),
            button_pressed: SolidBrush::new(DARK_BUTTON_PRESSED),
            border: SolidBrush::new(DARK_BORDER),
        }
    }

    fn background_brush(&self) -> Option<HBRUSH> {
        self.background.as_ref().map(SolidBrush::handle)
    }

    fn border_brush(&self) -> Option<HBRUSH> {
        self.border.as_ref().map(SolidBrush::handle)
    }

    fn button_brush(&self, item_state: u32) -> Option<HBRUSH> {
        if item_state & ODS_SELECTED != 0 {
            self.button_pressed.as_ref().map(SolidBrush::handle)
        } else if item_state & ODS_HOTLIGHT != 0 {
            self.button_hot.as_ref().map(SolidBrush::handle)
        } else {
            self.button.as_ref().map(SolidBrush::handle)
        }
    }

    fn text_color(&self) -> u32 {
        if self.dark { DARK_TEXT } else { LIGHT_TEXT }
    }

    fn disabled_text_color(&self) -> u32 {
        if self.dark {
            DARK_DISABLED_TEXT
        } else {
            LIGHT_DISABLED_TEXT
        }
    }
}

#[derive(Debug)]
struct SolidBrush {
    handle: HBRUSH,
}

impl SolidBrush {
    fn new(color: u32) -> Option<Self> {
        // Safety: color is a COLORREF value and CreateSolidBrush returns an owned GDI brush.
        let handle = unsafe { CreateSolidBrush(color) };
        (!handle.is_null()).then_some(Self { handle })
    }

    fn handle(&self) -> HBRUSH {
        self.handle
    }
}

impl Drop for SolidBrush {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // Safety: handle was returned by CreateSolidBrush and is owned by this wrapper.
            unsafe { DeleteObject(self.handle as HGDIOBJ) };
        }
    }
}

impl ButtonImageList {
    fn from_bitmap(bitmap: &IconBitmap) -> Result<Self> {
        let icon = icon::create_icon_from_bitmap(bitmap)?;
        let width = i32::try_from(bitmap.width).map_err(|_| {
            platform_error(format!("button icon width is too large: {}", bitmap.width))
        })?;
        let height = i32::try_from(bitmap.height).map_err(|_| {
            platform_error(format!(
                "button icon height is too large: {}",
                bitmap.height
            ))
        })?;

        // Safety: ImageList_Create returns a handle owned by ButtonImageList.
        let handle = unsafe { ImageList_Create(width, height, ILC_COLOR32, 1, 1) };
        if handle == 0 {
            return Err(platform_error("ImageList_Create failed for button icon"));
        }

        let image_list = Self { handle };
        // Safety: image_list owns a valid HIMAGELIST, and icon is a valid HICON
        // kept alive for the duration of this call.
        let index = unsafe {
            ImageList_ReplaceIcon(
                image_list.handle,
                -1,
                icon.raw_handle() as windows_sys::Win32::UI::WindowsAndMessaging::HICON,
            )
        };
        if index < 0 {
            Err(platform_error(
                "ImageList_ReplaceIcon failed for button icon",
            ))
        } else {
            Ok(image_list)
        }
    }
}

impl Drop for ButtonImageList {
    fn drop(&mut self) {
        if self.handle != 0 {
            // Safety: handle was created by ImageList_Create and is destroyed
            // once here after all button controls released their Arc.
            let _ = unsafe { ImageList_Destroy(self.handle) };
        }
    }
}

struct ButtonIconRenderState<'a> {
    hwnd: HWND,
    dpi_scale: f64,
    generation: u64,
    base_dir: &'a Path,
    button_icon_worker: &'a mut Option<ButtonIconWorker>,
    button_icon_worker_failed: &'a mut bool,
    button_icon_cache: &'a mut LruCache<ButtonIconKey, Option<Arc<IconBitmap>>>,
    button_icon_path_cache: &'a mut LruCache<String, Option<PathBuf>>,
    rendered_icon_cache: &'a mut LruCache<RenderedIconKey, Arc<ButtonImageList>>,
    icon_worker_error: Option<String>,
}

fn request_or_apply_button_icon<F>(
    state: &mut ButtonIconRenderState<'_>,
    control: &mut ButtonControl,
    tab_id: &str,
    button: &LauncherButton,
    icon_path_resolver: &mut Option<ButtonIconPathResolver>,
    expand_path: &F,
) where
    F: Fn(&str) -> String,
{
    if button.action != 0 {
        return;
    }
    let target_size = button_icon_target_size(state.dpi_scale);
    let source_size = target_size.clamp(icon::DEFAULT_ICON_SIZE, icon::MAX_ICON_SIZE);
    let Some(path_key) = button_icon_cache_path(button, state.base_dir) else {
        return;
    };
    let button_key = build_button_icon_key(
        tab_id,
        button,
        control.button_idx,
        path_key.as_str(),
        target_size,
    );
    let source_key = IconCacheKey::new(path_key.as_str(), source_size);
    control.icon_key = Some(button_key.clone());

    match state.button_icon_cache.get_cloned(&button_key) {
        Some(Some(bitmap)) => {
            apply_button_bitmap(
                state.rendered_icon_cache,
                control.hwnd,
                control,
                source_key,
                bitmap,
                target_size,
            );
        }
        Some(None) => {}
        None => {
            let Some(path) = resolve_cached_button_icon_path(
                state,
                path_key,
                button,
                icon_path_resolver,
                expand_path,
            ) else {
                state.button_icon_cache.insert(button_key, None);
                return;
            };
            ensure_icon_worker_started(state);
            if let Some(worker) = state.button_icon_worker.as_ref() {
                let request =
                    ButtonIconRequest::new(state.generation, button_key.clone(), path, source_size)
                        .with_verified_existing_path();
                if worker.request(request) {
                    state.button_icon_cache.insert(button_key, None);
                }
            }
        }
    }
}

fn resolve_cached_button_icon_path<F>(
    state: &mut ButtonIconRenderState<'_>,
    path_key: String,
    button: &LauncherButton,
    icon_path_resolver: &mut Option<ButtonIconPathResolver>,
    expand_path: &F,
) -> Option<PathBuf>
where
    F: Fn(&str) -> String,
{
    if let Some(cached) = state.button_icon_path_cache.get_cloned(&path_key) {
        return cached;
    }

    let icon_path_resolver =
        icon_path_resolver.get_or_insert_with(ButtonIconPathResolver::from_environment);
    let resolved = resolve_button_icon_path_in_context(
        button,
        state.base_dir,
        expand_path,
        icon_path_resolver,
    );
    state
        .button_icon_path_cache
        .insert(path_key, resolved.clone());
    resolved
}

fn ensure_icon_worker_started(state: &mut ButtonIconRenderState<'_>) {
    if state.button_icon_worker.as_ref().is_some()
        || *state.button_icon_worker_failed
        || state.hwnd.is_null()
    {
        return;
    }
    let hwnd_value = state.hwnd as usize;
    match icon::spawn_button_icon_worker(icon::DEFAULT_BUTTON_ICON_CACHE_MAX_ITEMS, move || {
        // Safety: hwnd_value was captured from this UI thread after the
        // window was created. Posting can fail during teardown, which is
        // harmless because stale worker results are dropped with state.
        unsafe { PostMessageW(hwnd_value as HWND, WM_ICON_COMPLETE, 0, 0) };
    }) {
        Ok(worker) => {
            *state.button_icon_worker = Some(worker);
        }
        Err(error) => {
            *state.button_icon_worker_failed = true;
            if state.icon_worker_error.is_none() {
                state.icon_worker_error = Some(error.user_message());
            }
        }
    }
}

fn apply_button_bitmap(
    rendered_icon_cache: &mut LruCache<RenderedIconKey, Arc<ButtonImageList>>,
    hwnd: HWND,
    control: &mut ButtonControl,
    source_key: IconCacheKey,
    bitmap: Arc<IconBitmap>,
    target_size: u32,
) {
    let Some(image_list) =
        get_or_create_button_image_list(rendered_icon_cache, source_key, bitmap, target_size)
    else {
        return;
    };
    control.image_list = Some(image_list);
    invalidate_window(hwnd);
}

fn get_or_create_button_image_list(
    rendered_icon_cache: &mut LruCache<RenderedIconKey, Arc<ButtonImageList>>,
    source_key: IconCacheKey,
    bitmap: Arc<IconBitmap>,
    target_size: u32,
) -> Option<Arc<ButtonImageList>> {
    let rendered_key = RenderedIconKey::new(source_key, target_size);
    if let Some(image_list) = rendered_icon_cache.get_cloned(&rendered_key) {
        return Some(image_list);
    }

    let rendered = icon::render_icon_bitmap(&bitmap, target_size).ok()?;
    let image_list = Arc::new(ButtonImageList::from_bitmap(&rendered).ok()?);
    rendered_icon_cache.insert(rendered_key, Arc::clone(&image_list));
    Some(image_list)
}

fn init_common_controls() -> Result<()> {
    let controls = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_TAB_CLASSES,
    };
    // Safety: controls points to initialized data for the duration of the call.
    if unsafe { InitCommonControlsEx(&controls) } == 0 {
        Err(platform_error("InitCommonControlsEx failed"))
    } else {
        Ok(())
    }
}

fn current_hinstance() -> Result<HINSTANCE> {
    // Safety: null requests the current process module handle.
    let module = unsafe { GetModuleHandleW(null()) };
    if module.is_null() {
        Err(platform_error("GetModuleHandleW failed"))
    } else {
        Ok(module as HINSTANCE)
    }
}

fn register_main_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(hinstance, MAIN_CLASS_NAME, Some(main_wnd_proc))
}

fn register_content_background_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(
        hinstance,
        CONTENT_BACKGROUND_CLASS_NAME,
        Some(content_background_wnd_proc),
    )
}

fn register_edit_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(hinstance, EDIT_CLASS_NAME, Some(edit_wnd_proc))
}

fn register_text_input_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(hinstance, TEXT_INPUT_CLASS_NAME, Some(text_input_wnd_proc))
}

fn register_tab_layout_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(hinstance, TAB_LAYOUT_CLASS_NAME, Some(tab_layout_wnd_proc))
}

fn register_hidden_items_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(
        hinstance,
        HIDDEN_ITEMS_CLASS_NAME,
        Some(hidden_items_wnd_proc),
    )
}

fn register_about_class(hinstance: HINSTANCE) -> Result<()> {
    register_window_class(hinstance, ABOUT_CLASS_NAME, Some(about_wnd_proc))
}

#[derive(Debug)]
struct AcceleratorTable {
    handle: HACCEL,
}

impl AcceleratorTable {
    fn handle(&self) -> HACCEL {
        self.handle
    }
}

impl Drop for AcceleratorTable {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // Safety: handle was returned by CreateAcceleratorTableW and is owned here.
            unsafe { DestroyAcceleratorTable(self.handle) };
        }
    }
}

fn create_main_accelerators() -> AcceleratorTable {
    let accelerators = main_accelerator_specs()
        .map(|(flags, key, command_id)| accelerator(flags, key, command_id));
    let handle = unsafe {
        // Safety: accelerators points to initialized ACCEL entries for this call.
        CreateAcceleratorTableW(accelerators.as_ptr(), accelerators.len() as i32)
    };
    AcceleratorTable { handle }
}

fn main_accelerator_specs() -> [(u8, u16, usize); 5] {
    [
        (FVIRTKEY | FCONTROL | FSHIFT, VK_LEFT, ID_MENU_MOVE_LEFT),
        (FVIRTKEY | FCONTROL | FSHIFT, VK_RIGHT, ID_MENU_MOVE_RIGHT),
        (FVIRTKEY | FCONTROL, VK_PRIOR, ID_MENU_SELECT_PREV),
        (FVIRTKEY | FCONTROL, VK_NEXT, ID_MENU_SELECT_NEXT),
        (FVIRTKEY, VK_F5, ID_MENU_SORT),
    ]
}

fn accelerator(flags: u8, key: u16, command_id: usize) -> ACCEL {
    ACCEL {
        fVirt: flags,
        key,
        cmd: command_id as u16,
    }
}

fn register_window_class(
    hinstance: HINSTANCE,
    class_name: &str,
    wnd_proc: windows_sys::Win32::UI::WindowsAndMessaging::WNDPROC,
) -> Result<()> {
    let class_name = wide_z(class_name);
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: wnd_proc,
        hInstance: hinstance,
        // Safety: IDC_ARROW is a predefined cursor resource.
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: null_mut::<c_void>() as HBRUSH,
        lpszClassName: class_name.as_ptr(),
        ..WNDCLASSW::default()
    };
    // Safety: window_class points to initialized class metadata.
    let atom = unsafe { RegisterClassW(&window_class) };
    if atom == 0 {
        Err(platform_error(format!(
            "RegisterClassW failed for {class_name:?}"
        )))
    } else {
        Ok(())
    }
}

fn create_main_menu(hwnd: HWND) -> Result<windows_sys::Win32::UI::WindowsAndMessaging::HMENU> {
    // Safety: CreateMenu/CreatePopupMenu allocate menus owned by the window.
    let menu = unsafe { CreateMenu() };
    let file_menu = unsafe { CreatePopupMenu() };
    let about_menu = unsafe { CreatePopupMenu() };
    if menu.is_null() || file_menu.is_null() || about_menu.is_null() {
        return Err(platform_error("CreateMenu failed"));
    }

    for (section_idx, section) in MAIN_MENU_SECTIONS.iter().enumerate() {
        if section_idx > 0 {
            append_separator(file_menu)?;
        }
        for item in *section {
            append_menu_item(file_menu, menu_command_id(item.command), item.win32_label)?;
        }
    }

    let label = wide_z("File");
    // Safety: menu handles are valid and file_menu remains owned by menu after
    // being appended as a popup.
    if unsafe { AppendMenuW(menu, MF_POPUP, file_menu as usize, label.as_ptr()) } == 0 {
        return Err(platform_error("AppendMenuW failed for File menu"));
    }

    for (section_idx, section) in ABOUT_MENU_SECTIONS.iter().enumerate() {
        if section_idx > 0 {
            append_separator(about_menu)?;
        }
        for item in *section {
            append_menu_item(about_menu, menu_command_id(item.command), item.win32_label)?;
        }
    }
    let label = wide_z("About");
    // Safety: menu handles are valid and about_menu remains owned by menu after
    // being appended as a popup.
    if unsafe { AppendMenuW(menu, MF_POPUP, about_menu as usize, label.as_ptr()) } == 0 {
        return Err(platform_error("AppendMenuW failed for About menu"));
    }
    // Safety: hwnd is the main window and menu is a valid menu handle.
    if unsafe { SetMenu(hwnd, menu) } == 0 {
        return Err(platform_error("SetMenu failed"));
    }
    Ok(file_menu)
}

fn menu_command_id(command: MenuCommand) -> usize {
    match command {
        MenuCommand::AddFolderTab => ID_MENU_ADD_FOLDER_TAB,
        MenuCommand::AddManualTab => ID_MENU_ADD_MANUAL_TAB,
        MenuCommand::SetTabFolder => ID_MENU_SET_TAB_FOLDER,
        MenuCommand::TabLayout => ID_MENU_TAB_LAYOUT,
        MenuCommand::RenameTab => ID_MENU_RENAME_TAB,
        MenuCommand::DeleteTab => ID_MENU_DELETE_TAB,
        MenuCommand::MoveLeft => ID_MENU_MOVE_LEFT,
        MenuCommand::MoveRight => ID_MENU_MOVE_RIGHT,
        MenuCommand::SelectPrev => ID_MENU_SELECT_PREV,
        MenuCommand::SelectNext => ID_MENU_SELECT_NEXT,
        MenuCommand::Sort => ID_MENU_SORT,
        MenuCommand::Refresh => ID_MENU_REFRESH,
        MenuCommand::Reset => ID_MENU_RESET,
        MenuCommand::ManageHidden => ID_MENU_MANAGE_HIDDEN,
        MenuCommand::DarkTheme => ID_MENU_DARK_THEME,
        MenuCommand::Exit => ID_MENU_EXIT,
        MenuCommand::About => ID_MENU_ABOUT,
    }
}

fn menu_command_from_id(command_id: usize) -> Option<MenuCommand> {
    match command_id {
        ID_MENU_ADD_FOLDER_TAB => Some(MenuCommand::AddFolderTab),
        ID_MENU_ADD_MANUAL_TAB => Some(MenuCommand::AddManualTab),
        ID_MENU_SET_TAB_FOLDER => Some(MenuCommand::SetTabFolder),
        ID_MENU_TAB_LAYOUT => Some(MenuCommand::TabLayout),
        ID_MENU_RENAME_TAB => Some(MenuCommand::RenameTab),
        ID_MENU_DELETE_TAB => Some(MenuCommand::DeleteTab),
        ID_MENU_MOVE_LEFT => Some(MenuCommand::MoveLeft),
        ID_MENU_MOVE_RIGHT => Some(MenuCommand::MoveRight),
        ID_MENU_SELECT_PREV => Some(MenuCommand::SelectPrev),
        ID_MENU_SELECT_NEXT => Some(MenuCommand::SelectNext),
        ID_MENU_SORT => Some(MenuCommand::Sort),
        ID_MENU_REFRESH => Some(MenuCommand::Refresh),
        ID_MENU_RESET => Some(MenuCommand::Reset),
        ID_MENU_MANAGE_HIDDEN => Some(MenuCommand::ManageHidden),
        ID_MENU_DARK_THEME => Some(MenuCommand::DarkTheme),
        ID_MENU_EXIT => Some(MenuCommand::Exit),
        ID_MENU_ABOUT => Some(MenuCommand::About),
        _ => None,
    }
}

fn context_menu_command_id(command: ButtonContextCommand) -> usize {
    match command {
        ButtonContextCommand::Edit => ID_CONTEXT_EDIT,
        ButtonContextCommand::OpenInExplorer => ID_CONTEXT_OPEN_EXPLORER,
        ButtonContextCommand::Hide => ID_CONTEXT_HIDE,
    }
}

fn context_menu_command_from_id(command_id: usize) -> Option<ButtonContextCommand> {
    match command_id {
        ID_CONTEXT_EDIT => Some(ButtonContextCommand::Edit),
        ID_CONTEXT_OPEN_EXPLORER => Some(ButtonContextCommand::OpenInExplorer),
        ID_CONTEXT_HIDE => Some(ButtonContextCommand::Hide),
        _ => None,
    }
}

#[cfg(debug_assertions)]
fn debug_context_menu_command_override() -> Option<ButtonContextCommand> {
    debug_context_menu_command_override_from_env(std::env::var_os(
        "J3LAUNCHER_TEST_CONTEXT_MENU_COMMAND",
    ))
}

#[cfg(not(debug_assertions))]
fn debug_context_menu_command_override() -> Option<ButtonContextCommand> {
    None
}

#[cfg(debug_assertions)]
fn debug_context_menu_command_override_from_env(
    command: Option<std::ffi::OsString>,
) -> Option<ButtonContextCommand> {
    match command?.to_string_lossy().as_ref() {
        "Edit" => Some(ButtonContextCommand::Edit),
        "OpenInExplorer" => Some(ButtonContextCommand::OpenInExplorer),
        "Hide" => Some(ButtonContextCommand::Hide),
        _ => None,
    }
}

fn create_tab_control(hwnd: HWND, hinstance: HINSTANCE) -> Result<HWND> {
    // Safety: WC_TABCONTROLW is a common-control class initialized earlier.
    let tab_hwnd = unsafe {
        CreateWindowExW(
            0,
            WC_TABCONTROLW,
            null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_TABSTOP,
            0,
            0,
            10,
            10,
            hwnd,
            1 as _,
            hinstance,
            null(),
        )
    };
    if tab_hwnd.is_null() {
        Err(platform_error("CreateWindowExW failed for tab control"))
    } else {
        Ok(tab_hwnd)
    }
}

fn create_content_background(hwnd: HWND, hinstance: HINSTANCE) -> Result<HWND> {
    let class_name = wide_z(CONTENT_BACKGROUND_CLASS_NAME);
    let background_hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            null(),
            WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
            0,
            0,
            10,
            10,
            hwnd,
            2 as _,
            hinstance,
            null(),
        )
    };
    if background_hwnd.is_null() {
        Err(platform_error(
            "CreateWindowExW failed for content background",
        ))
    } else {
        Ok(background_hwnd)
    }
}

unsafe extern "system" fn content_background_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            let hdc = wparam as HDC;
            if hdc.is_null() {
                return 0;
            }
            let mut rect = RECT::default();
            // Safety: hwnd is this background child and rect is writable.
            if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
                return 0;
            }
            // Safety: DARK_BACKGROUND is a COLORREF and the brush is deleted
            // before returning from this paint message.
            let brush = unsafe { CreateSolidBrush(DARK_BACKGROUND) };
            if brush.is_null() {
                return 0;
            }
            let painted = unsafe { FillRect(hdc, &rect, brush) != 0 };
            unsafe { DeleteObject(brush as HGDIOBJ) };
            painted as LRESULT
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn append_menu_item(
    menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    id: usize,
    label: &str,
) -> Result<()> {
    let label = wide_z(label);
    // Safety: menu is valid and label is NUL-terminated for this call.
    if unsafe { AppendMenuW(menu, MF_STRING, id, label.as_ptr()) } == 0 {
        Err(platform_error("AppendMenuW failed"))
    } else {
        Ok(())
    }
}

fn append_separator(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU) -> Result<()> {
    // Safety: menu is valid.
    if unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) } == 0 {
        Err(platform_error("AppendMenuW separator failed"))
    } else {
        Ok(())
    }
}

fn create_context_menu(
    tab: &LauncherTab,
    button: &LauncherButton,
) -> Result<windows_sys::Win32::UI::WindowsAndMessaging::HMENU> {
    // Safety: CreatePopupMenu allocates a standalone menu destroyed by caller.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return Err(platform_error("컨텍스트 메뉴를 만들 수 없습니다."));
    }
    if let Err(error) = populate_context_menu(menu, tab, button) {
        // Safety: menu was created in this function and has not been handed out.
        unsafe { DestroyMenu(menu) };
        return Err(error);
    }
    Ok(menu)
}

fn populate_context_menu(
    menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    tab: &LauncherTab,
    button: &LauncherButton,
) -> Result<()> {
    for item in BUTTON_CONTEXT_MENU_ITEMS {
        let state = if button_context_command_enabled(item.command, tab, button) {
            MF_ENABLED
        } else {
            MF_GRAYED
        };
        append_context_item(
            menu,
            context_menu_command_id(item.command),
            item.label,
            state,
        )?;
    }
    Ok(())
}

fn append_context_item(
    menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    id: usize,
    label: &str,
    state: windows_sys::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS,
) -> Result<()> {
    let label = wide_z(label);
    // Safety: menu is valid and label lives for the call.
    if unsafe { AppendMenuW(menu, MF_STRING | state, id, label.as_ptr()) } == 0 {
        Err(platform_error("컨텍스트 메뉴 항목을 추가할 수 없습니다."))
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn visible_button_slots(tab: &LauncherTab) -> Vec<VisibleButtonSlot> {
    let mut slots = Vec::new();
    let mut scratch = VisibleButtonSlotScratch::default();
    collect_visible_button_slots(tab, &mut slots, &mut scratch);
    slots
}

fn required_rows(tab: &LauncherTab, buttons: &[ButtonControl], cols: usize) -> usize {
    if cols == 0 {
        return 0;
    }
    let max_slot = buttons.iter().map(|button| button.slot_idx).max();
    let required_slots = max_slot
        .map(|slot| slot.saturating_add(1))
        .unwrap_or(0)
        .max(buttons.len());
    let configured_slots = usize::from(tab.rows.max(1)) * cols;
    configured_slots.max(required_slots).div_ceil(cols).max(1)
}

#[cfg(test)]
fn resolve_button_icon_path_with<F>(
    button: &LauncherButton,
    base_dir: &Path,
    expand_path: F,
    path_env: Option<OsString>,
    pathext_env: Option<OsString>,
) -> Option<PathBuf>
where
    F: Fn(&str) -> String,
{
    let mut icon_path_resolver = ButtonIconPathResolver::from_env(path_env, pathext_env);
    resolve_button_icon_path_in_context(button, base_dir, expand_path, &mut icon_path_resolver)
}

fn resolve_button_icon_path_in_context<F>(
    button: &LauncherButton,
    base_dir: &Path,
    expand_path: F,
    icon_path_resolver: &mut ButtonIconPathResolver,
) -> Option<PathBuf>
where
    F: Fn(&str) -> String,
{
    if button.action != 0 {
        return None;
    }

    for raw_path in button_icon_path_candidates(button) {
        if let Some(resolved) =
            resolve_existing_icon_candidate(raw_path, base_dir, &expand_path, icon_path_resolver)
        {
            return Some(resolved);
        }
    }

    None
}

fn button_icon_path_candidates(button: &LauncherButton) -> Vec<&str> {
    let mut candidates = Vec::new();
    let path = button.path.trim();
    if !path.is_empty() {
        candidates.push(path);
    }
    let source_path = button.source_path.trim();
    if !source_path.is_empty() && !candidates.contains(&source_path) {
        candidates.push(source_path);
    }
    candidates
}

fn button_icon_cache_path(button: &LauncherButton, base_dir: &Path) -> Option<String> {
    let candidates = button_icon_path_candidates(button);
    if candidates.is_empty() {
        return None;
    }

    let mut key = String::new();
    append_button_icon_cache_key_part(&mut key, &icon::normalize_icon_cache_path(base_dir));
    for raw_path in candidates {
        let normalized = icon::normalize_icon_cache_path(Path::new(raw_path));
        append_button_icon_cache_key_part(&mut key, &normalized);
    }
    Some(key)
}

fn append_button_icon_cache_key_part(key: &mut String, value: &str) {
    key.push_str(&value.len().to_string());
    key.push(':');
    key.push_str(value);
    key.push(';');
}

fn resolve_existing_icon_candidate<F>(
    raw_path: &str,
    base_dir: &Path,
    expand_path: &F,
    icon_path_resolver: &mut ButtonIconPathResolver,
) -> Option<PathBuf>
where
    F: Fn(&str) -> String,
{
    let expanded = expand_path(raw_path.trim_matches('"'));
    let expanded = expanded.trim();
    if expanded.is_empty() {
        return None;
    }

    let direct = PathBuf::from(expanded);
    if icon_path_resolver.path_exists(&direct) {
        return Some(direct);
    }

    let local = base_dir.join(expanded);
    if icon_path_resolver.path_exists(&local) {
        return Some(local);
    }

    icon_path_resolver.find_program_on_path(expanded)
}

// Keep small renders cheap, then index PATH directories once for many command lookups.
const PATH_DIRECTORY_INDEX_LOOKUP_THRESHOLD: usize = 4;

struct ButtonIconPathResolver {
    path_dirs: Vec<PathBuf>,
    executable_extensions: Vec<String>,
    path_lookup_cache: HashMap<String, Option<PathBuf>>,
    path_dir_entries: Vec<Option<PathDirEntries>>,
    exists_cache: HashMap<PathBuf, bool>,
    path_lookup_count: usize,
}

impl ButtonIconPathResolver {
    fn from_environment() -> Self {
        Self::from_env(env::var_os("PATH"), env::var_os("PATHEXT"))
    }

    fn from_env(path_env: Option<OsString>, pathext_env: Option<OsString>) -> Self {
        let path_dirs = path_env
            .as_ref()
            .map(|value| env::split_paths(value).collect::<Vec<_>>())
            .unwrap_or_default();
        let path_dir_entries = (0..path_dirs.len()).map(|_| None).collect();
        Self {
            path_dirs,
            executable_extensions: executable_extensions_from_env(&pathext_env),
            path_lookup_cache: HashMap::new(),
            path_dir_entries,
            exists_cache: HashMap::new(),
            path_lookup_count: 0,
        }
    }

    fn path_exists(&mut self, path: &Path) -> bool {
        if let Some(exists) = self.exists_cache.get(path) {
            return *exists;
        }

        let exists = path.exists();
        self.exists_cache.insert(path.to_path_buf(), exists);
        exists
    }

    fn find_program_on_path(&mut self, command: &str) -> Option<PathBuf> {
        let command_path = Path::new(command);
        if command_path.is_absolute() || has_path_separator(command) {
            return None;
        }

        if let Some(cached) = self.path_lookup_cache.get(command) {
            return cached.clone();
        }

        let candidates = self.executable_candidate_names(command_path, command);
        let resolved = if self.path_lookup_count < PATH_DIRECTORY_INDEX_LOOKUP_THRESHOLD {
            self.path_lookup_count += 1;
            self.find_program_on_path_by_exists(&candidates)
        } else {
            self.find_program_on_path_by_index(&candidates)
        };
        self.path_lookup_cache
            .insert(command.to_owned(), resolved.clone());
        resolved
    }

    fn executable_candidate_names(
        &self,
        command_path: &Path,
        command: &str,
    ) -> Vec<ProgramCandidateName> {
        if command_path.extension().is_some() {
            return vec![ProgramCandidateName::new(command)];
        }

        self.executable_extensions
            .iter()
            .map(|extension| {
                let mut file_name = String::with_capacity(command.len() + extension.len());
                file_name.push_str(command);
                file_name.push_str(extension);
                ProgramCandidateName::new(file_name)
            })
            .collect()
    }

    fn find_program_on_path_by_exists(
        &mut self,
        candidates: &[ProgramCandidateName],
    ) -> Option<PathBuf> {
        for dir_index in 0..self.path_dirs.len() {
            for candidate_name in candidates {
                let mut candidate = self.path_dirs[dir_index].clone();
                candidate.push(&candidate_name.file_name);
                if self.path_exists(&candidate) {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn find_program_on_path_by_index(
        &mut self,
        candidates: &[ProgramCandidateName],
    ) -> Option<PathBuf> {
        for dir_index in 0..self.path_dirs.len() {
            for candidate_name in candidates {
                if self.path_dir_contains(dir_index, candidate_name) {
                    let mut candidate = self.path_dirs[dir_index].clone();
                    candidate.push(&candidate_name.file_name);
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn path_dir_contains(
        &mut self,
        dir_index: usize,
        candidate_name: &ProgramCandidateName,
    ) -> bool {
        if self.path_dir_entries[dir_index].is_none() {
            self.path_dir_entries[dir_index] =
                Some(PathDirEntries::read(&self.path_dirs[dir_index]));
        }
        self.path_dir_entries[dir_index]
            .as_ref()
            .is_some_and(|entries| entries.contains(candidate_name))
    }
}

struct ProgramCandidateName {
    file_name: OsString,
    folded_file_name: String,
}

impl ProgramCandidateName {
    fn new<S: Into<OsString>>(file_name: S) -> Self {
        let file_name = file_name.into();
        let folded_file_name = fold_file_name(&file_name);
        Self {
            file_name,
            folded_file_name,
        }
    }
}

#[derive(Default)]
struct PathDirEntries {
    exact_names: HashSet<OsString>,
    folded_names: HashSet<String>,
}

impl PathDirEntries {
    fn read(dir: &Path) -> Self {
        let mut entries = Self::default();
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return entries;
        };
        for entry in read_dir.flatten() {
            let file_name = entry.file_name();
            entries.folded_names.insert(fold_file_name(&file_name));
            entries.exact_names.insert(file_name);
        }
        entries
    }

    fn contains(&self, candidate_name: &ProgramCandidateName) -> bool {
        self.exact_names.contains(&candidate_name.file_name)
            || self
                .folded_names
                .contains(candidate_name.folded_file_name.as_str())
    }
}

fn fold_file_name(file_name: &OsString) -> String {
    file_name.to_string_lossy().to_lowercase()
}

fn executable_extensions_from_env(pathext_env: &Option<OsString>) -> Vec<String> {
    let mut extensions = Vec::new();
    if let Some(raw_pathext) = pathext_env {
        for extension in raw_pathext.to_string_lossy().split(';') {
            let extension = extension.trim();
            if extension.is_empty() {
                continue;
            }
            let extension = if extension.starts_with('.') {
                extension.to_owned()
            } else {
                format!(".{extension}")
            };
            if !extensions
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&extension))
            {
                extensions.push(extension);
            }
        }
    }
    if extensions.is_empty() {
        extensions.extend([".COM", ".EXE", ".BAT", ".CMD"].map(str::to_owned));
    }
    extensions
}

fn has_path_separator(value: &str) -> bool {
    value.contains(['\\', '/'])
}

fn build_button_icon_key(
    tab_id: &str,
    button: &LauncherButton,
    button_idx: usize,
    path_key: &str,
    target_size: u32,
) -> ButtonIconKey {
    let button_id = if !button.item_id.trim().is_empty() {
        button.item_id.trim().to_owned()
    } else if !button.source_path.trim().is_empty() {
        format!("source:{}", button.source_path.trim())
    } else if !button.path.trim().is_empty() {
        format!("path:{}", button.path.trim())
    } else {
        format!("button:{button_idx}")
    };
    ButtonIconKey::new(
        tab_id.to_owned(),
        button_id,
        path_key.to_owned(),
        target_size,
    )
}

fn button_icon_target_size(dpi_scale: f64) -> u32 {
    u32::try_from(scale_px(
        dpi_scale,
        icon::DEFAULT_BUTTON_ICON_RENDER_SIZE as i32,
    ))
    .unwrap_or(icon::DEFAULT_BUTTON_ICON_RENDER_SIZE)
    .clamp(1, icon::MAX_ICON_SIZE)
}

fn find_icon_result_button_position(
    buttons: &[ButtonControl],
    generation: u64,
    button_key: &ButtonIconKey,
) -> Option<usize> {
    buttons
        .iter()
        .position(|control| {
            control.icon_generation == generation && control.icon_key.as_ref() == Some(button_key)
        })
        .or_else(|| {
            buttons
                .iter()
                .position(|control| control.icon_key.as_ref() == Some(button_key))
        })
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
    require_items: bool,
) -> (Option<ScanSignature>, Option<Vec<crate::domain::ScanItem>>) {
    let Some(signature) = tab.scan_signature.clone() else {
        return (None, None);
    };
    let known_items = build_known_scan_items_from_tab(tab);
    if require_items && known_items.is_none() {
        return (None, None);
    }
    (Some(signature), known_items)
}

fn clear_buttons(buttons: &mut Vec<ButtonControl>) {
    for button in buttons.drain(..) {
        if !button.hwnd.is_null() {
            // Safety: button hwnd was subclassed with this proc/id when created.
            unsafe {
                RemoveWindowSubclass(
                    button.hwnd,
                    Some(button_drag_subclass_proc),
                    BUTTON_DRAG_SUBCLASS_ID,
                )
            };
            // Safety: these HWNDs are child buttons created by this UI.
            unsafe { DestroyWindow(button.hwnd) };
        }
    }
}

fn button_drag_endpoint(control: &ButtonControl) -> ButtonDragEndpoint {
    ButtonDragEndpoint::new(
        button_control_key(control.hwnd),
        control.tab_idx,
        control.button_idx,
        control.slot_idx,
    )
}

fn button_control_key(hwnd: HWND) -> usize {
    hwnd as usize
}

fn button_window_center(hwnd: HWND) -> Option<(i32, i32)> {
    let mut rect = RECT::default();
    // Safety: hwnd is a child button and rect is writable.
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        None
    } else {
        Some(((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2))
    }
}

fn load_icon_from_file(path: &Path) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    let wide = path_to_wide_z(path);
    // Safety: wide is a NUL-terminated filesystem path for this call.
    let handle = unsafe {
        LoadImageW(
            null_mut(),
            wide.as_ptr(),
            IMAGE_ICON,
            0,
            0,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        )
    };
    if handle.is_null() {
        None
    } else {
        Some(handle as windows_sys::Win32::UI::WindowsAndMessaging::HICON)
    }
}

fn load_icon_from_resource(
    hinstance: HINSTANCE,
) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    // Safety: APP_ICON_RESOURCE_ID matches app.rc and hinstance is this module.
    let handle = unsafe {
        LoadImageW(
            hinstance,
            resource_id(APP_ICON_RESOURCE_ID),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        )
    };
    if handle.is_null() {
        None
    } else {
        Some(handle as windows_sys::Win32::UI::WindowsAndMessaging::HICON)
    }
}

fn resource_id(id: u16) -> windows_sys::core::PCWSTR {
    id as usize as windows_sys::core::PCWSTR
}

fn path_to_wide_z(path: &Path) -> Vec<u16> {
    let mut units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    units.push(0);
    units
}

fn show_message(
    hwnd: HWND,
    title: &str,
    message: &str,
    style: windows_sys::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) {
    let title = wide_z(title);
    let message = wide_z(message);
    // Safety: strings are NUL-terminated and live for the call.
    unsafe { MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), style) };
}

fn confirm_message(
    hwnd: HWND,
    title: &str,
    message: &str,
    style: windows_sys::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> bool {
    let title = wide_z(title);
    let message = wide_z(message);
    unsafe { MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_YESNO | style) == IDYES }
}

const fn colorref(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

fn apply_window_theme(hwnd: HWND, dark: bool) {
    if hwnd.is_null() {
        return;
    }
    let theme = if dark {
        w!("DarkMode_Explorer")
    } else {
        null()
    };
    // Safety: hwnd is a live window handle owned by this UI. Theme strings are
    // static or null and SetWindowTheme does not retain Rust-owned memory.
    unsafe { SetWindowTheme(hwnd, theme, null()) };
}

fn install_button_drag_subclass(hwnd: HWND, parent: HWND) {
    if hwnd.is_null() || parent.is_null() {
        return;
    }
    // Safety: hwnd is a button child window owned by this UI. The subclass proc
    // stores only the parent HWND value and is removed before/during destruction.
    unsafe {
        SetWindowSubclass(
            hwnd,
            Some(button_drag_subclass_proc),
            BUTTON_DRAG_SUBCLASS_ID,
            parent as usize,
        )
    };
}

fn cursor_position() -> Option<CursorPoint> {
    let mut point = POINT::default();
    // Safety: point is writable and GetCursorPos does not retain it.
    if unsafe { GetCursorPos(&mut point) } == 0 {
        None
    } else {
        Some(CursorPoint::new(point.x, point.y))
    }
}

fn point_in_rect(point: CursorPoint, rect: &RECT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

fn apply_suggested_dpi_rect(hwnd: HWND, suggested: &RECT) {
    if hwnd.is_null() {
        return;
    }
    let width = rect_span(suggested.left, suggested.right).max(1);
    let height = rect_span(suggested.top, suggested.bottom).max(1);
    // Safety: hwnd is a top-level window and suggested is the RECT supplied by
    // WM_DPICHANGED. This is only called outside the native size/move loop.
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            suggested.left,
            suggested.top,
            width,
            height,
            SWP_NOZORDER,
        )
    };
}

fn invalidate_window(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    // Safety: hwnd is a live window handle; null rect invalidates the full client area.
    unsafe { InvalidateRect(hwnd, null::<RECT>(), 1) };
}

fn set_redraw_enabled(hwnd: HWND, enabled: bool) {
    if hwnd.is_null() {
        return;
    }
    // Safety: WM_SETREDRAW toggles repaint generation for this window only and
    // does not retain pointers. It is used during close teardown, so no redraw
    // re-enable is needed before DestroyWindow.
    unsafe { SendMessageW(hwnd, WM_SETREDRAW, usize::from(enabled), 0) };
}

fn scale_px(scale: f64, value: i32) -> i32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (f64::from(value) * scale)
        .round()
        .clamp(1.0, f64::from(i32::MAX)) as i32
}

fn wide_z(value: &str) -> Vec<u16> {
    value
        .encode_utf16()
        .map(|unit| if unit == 0 { 0xFFFD } else { unit })
        .chain(std::iter::once(0))
        .collect()
}

fn platform_error(message: impl Into<String>) -> LauncherError {
    LauncherError::Platform {
        message: message.into(),
    }
}

fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
    let value = lparam as u32;
    let x = (value & 0xFFFF) as u16 as i16 as i32;
    let y = ((value >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

fn run_message_loop(hwnd: HWND, accelerators: HACCEL) -> Result<()> {
    let mut message = MSG::default();
    loop {
        // Safety: message points to writable memory and null hwnd means all
        // thread messages.
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == 0 {
            break;
        }
        if result < 0 {
            let error_code = unsafe { GetLastError() };
            return Err(platform_error(format!(
                "GetMessageW failed in main message loop: {error_code}"
            )));
        }
        if !accelerators.is_null()
            && unsafe { TranslateAcceleratorW(hwnd, accelerators, &message) } != 0
        {
            continue;
        }
        // Safety: message was filled by GetMessageW.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            // Safety: WM_NCCREATE lparam points to CREATESTRUCTW supplied by
            // CreateWindowExW. lpCreateParams is the creation state we passed.
            let create_state = unsafe { (*create).lpCreateParams as *mut MainWindowCreateState };
            if !create_state.is_null() {
                let app = unsafe { (*create_state).app };
                if app.is_null() {
                    return 0;
                }
                // Safety: hwnd is the current window and app is valid during
                // window creation. GWLP_USERDATA stores an integer-sized pointer.
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize) };
                // Safety: app points to boxed state owned by the window after
                // CreateWindowExW succeeds.
                unsafe { (*app).hwnd = hwnd };
                return 1;
            }
        }
        return 0;
    }

    // Safety: GWLP_USERDATA contains the Win32App pointer set in WM_NCCREATE or 0.
    let app_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Win32App };
    if app_ptr.is_null() {
        // Safety: default processing for messages before state is attached.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    // Safety: app_ptr remains valid until WM_NCDESTROY.
    let app = unsafe { &mut *app_ptr };

    match message {
        WM_CREATE => {
            if let Err(error) = app.on_create() {
                app.show_error("Startup", &error.user_message());
                return -1;
            }
            0
        }
        WM_ENTERSIZEMOVE => {
            app.enter_size_move();
            0
        }
        WM_EXITSIZEMOVE => {
            app.exit_size_move();
            0
        }
        WM_SIZE => {
            if !app.should_defer_size_layout() {
                app.layout();
            }
            0
        }
        WM_ERASEBKGND => {
            if app.erase_dark_background(wparam as HDC) {
                1
            } else {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORDLG => {
            if let Some(result) = app.dark_control_brush_result(wparam as HDC) {
                result
            } else {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
        }
        WM_DRAWITEM => {
            if lparam != 0 {
                let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
                if app.draw_owner_draw_button(draw) {
                    return 1;
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_COMMAND => {
            app.on_command(loword(wparam) as usize, hiword(wparam), lparam as HWND);
            0
        }
        WM_NOTIFY => {
            if lparam != 0 {
                // Safety: WM_NOTIFY lparam points to NMHDR-compatible data.
                let hdr = unsafe { &*(lparam as *const NMHDR) };
                if hdr.hwndFrom == app.tab_hwnd && hdr.code == TCN_SELCHANGE {
                    // Safety: tab_hwnd is a valid tab control.
                    let selected = unsafe { SendMessageW(app.tab_hwnd, TCM_GETCURSEL, 0, 0) };
                    if selected >= 0 {
                        let selected = selected as usize;
                        if selected < app.folder_tabs.len() {
                            app.selected_tab_idx = selected;
                            app.render_current_tab();
                            app.update_menu_state();
                        }
                    }
                }
            }
            0
        }
        WM_CONTEXTMENU => {
            let child = wparam as HWND;
            let (x, y) = point_from_lparam(lparam);
            app.show_button_context_menu(child, x, y);
            0
        }
        WM_DPICHANGED => {
            let dpi = u32::from(loword(wparam).max(96));
            if app.size_move.in_loop {
                app.defer_dpi_change_during_size_move(dpi);
                return 0;
            }
            let suggested = if lparam == 0 {
                None
            } else {
                // Safety: WM_DPICHANGED lparam points to a suggested RECT for
                // the duration of message processing.
                Some(unsafe { &*(lparam as *const RECT) })
            };
            app.apply_dpi_change(dpi, suggested);
            0
        }
        WM_SCAN_COMPLETE => {
            app.complete_scan();
            0
        }
        WM_ICON_COMPLETE => {
            app.process_icon_results();
            0
        }
        WM_CONFIG_SAVE_COMPLETE => {
            app.process_config_save_results();
            0
        }
        WM_BUTTON_DRAG_EVENT => {
            if let Some(event) = ButtonDragEvent::from_wparam(wparam) {
                if app.on_button_drag_event(event, lparam as HWND) {
                    1
                } else {
                    0
                }
            } else {
                0
            }
        }
        WM_CLOSE => {
            app.close();
            0
        }
        WM_DESTROY => {
            // Safety: standard process message-loop shutdown.
            unsafe { PostQuitMessage(0) };
            0
        }
        WM_NCDESTROY => {
            // Safety: app_ptr is the Box leaked before CreateWindowExW and must
            // be reclaimed exactly once when the HWND is finally destroyed.
            unsafe {
                if !(*app_ptr).creation_state.is_null() {
                    (*(*app_ptr).creation_state).destroyed = true;
                }
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(app_ptr));
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        _ => {
            // Safety: default processing for unhandled messages.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

unsafe extern "system" fn button_drag_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    parent_hwnd: usize,
) -> LRESULT {
    let event = match message {
        WM_LBUTTONDOWN => Some(BUTTON_DRAG_EVENT_DOWN),
        WM_MOUSEMOVE => Some(BUTTON_DRAG_EVENT_MOVE),
        WM_LBUTTONUP => Some(BUTTON_DRAG_EVENT_UP),
        WM_CAPTURECHANGED => Some(BUTTON_DRAG_EVENT_CANCEL),
        _ => None,
    };

    if let Some(event) = event {
        let parent = parent_hwnd as HWND;
        let consumed = !parent.is_null()
            && unsafe { SendMessageW(parent, WM_BUTTON_DRAG_EVENT, event, hwnd as LPARAM) } != 0;
        if consumed && matches!(message, WM_MOUSEMOVE | WM_LBUTTONUP) {
            if message == WM_LBUTTONUP {
                // Safety: hwnd is the subclassed button. BM_SETSTATE clears the
                // pressed visual state, and ReleaseCapture releases button mouse capture.
                unsafe {
                    SendMessageW(hwnd, BM_SETSTATE, 0, 0);
                    ReleaseCapture();
                }
            }
            return 0;
        }
    }

    if message == WM_NCDESTROY {
        // Safety: hwnd is currently subclassed with this proc and id.
        unsafe {
            RemoveWindowSubclass(
                hwnd,
                Some(button_drag_subclass_proc),
                BUTTON_DRAG_SUBCLASS_ID,
            )
        };
    }

    // Safety: forward unhandled messages to the next subclass/window procedure.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[derive(Debug)]
struct DialogCreateState<T> {
    state: *mut T,
    destroyed: bool,
}

#[derive(Debug)]
struct ModalDialogState<T> {
    result: RefCell<Option<T>>,
    done: Cell<bool>,
}

impl<T> ModalDialogState<T> {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            result: RefCell::new(None),
            done: Cell::new(false),
        })
    }

    fn is_done(&self) -> bool {
        self.done.get()
    }

    fn finish(&self) {
        self.done.set(true);
    }

    fn set_result(&self, result: T) {
        *self.result.borrow_mut() = Some(result);
    }

    fn take_result(&self) -> Option<T> {
        self.result.borrow_mut().take()
    }
}

#[derive(Debug)]
struct EditDialogState {
    initial: ButtonInfo,
    modal: Rc<ModalDialogState<ButtonInfo>>,
    creation_state: *mut DialogCreateState<EditDialogState>,
    dpi_scale: f64,
}

fn edit_button_dialog(
    owner: HWND,
    hinstance: HINSTANCE,
    initial: ButtonInfo,
    dpi_scale: f64,
) -> Option<ButtonInfo> {
    let modal = ModalDialogState::new();
    let state = Box::new(EditDialogState {
        initial,
        modal: Rc::clone(&modal),
        creation_state: null_mut(),
        dpi_scale,
    });
    let state_ptr = Box::into_raw(state);
    let mut create_state = DialogCreateState {
        state: state_ptr,
        destroyed: false,
    };
    // Safety: state_ptr was produced by Box::into_raw and is either reclaimed
    // below or by the dialog's WM_NCDESTROY handler.
    unsafe { (*state_ptr).creation_state = &mut create_state };
    let class = wide_z(EDIT_CLASS_NAME);
    let title = wide_z("Edit Button");
    // Safety: class/title are valid, and create_state lives through the
    // synchronous CreateWindowExW creation messages.
    let (client_width, client_height) = edit_dialog_min_client_size(dpi_scale);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            client_width,
            client_height,
            owner,
            null_mut(),
            hinstance,
            (&mut create_state as *mut DialogCreateState<EditDialogState>).cast(),
        )
    };
    if hwnd.is_null() {
        if !create_state.destroyed {
            // Safety: if WM_NCDESTROY did not run, no dialog reclaimed state_ptr.
            unsafe { drop(Box::from_raw(state_ptr)) };
        }
        return None;
    }
    // Safety: hwnd is alive and owns state_ptr through GWLP_USERDATA from here.
    unsafe { (*state_ptr).creation_state = null_mut() };
    ensure_window_client_size(hwnd, client_width, client_height);
    run_modal_loop(owner, hwnd, modal.as_ref());
    modal.take_result()
}

fn run_modal_loop<T>(owner: HWND, hwnd: HWND, modal: &ModalDialogState<T>) {
    let owner_was_enabled = if window_is_alive(owner) {
        // Safety: owner is a live HWND checked immediately before this call.
        unsafe { IsWindowEnabled(owner) != 0 }
    } else {
        false
    };
    if owner_was_enabled {
        // Safety: owner is a live HWND and is restored before returning.
        unsafe { EnableWindow(owner, 0) };
    }
    center_window_over_owner(owner, hwnd);
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
    }
    let mut message = MSG::default();
    let mut quit_code = None;
    loop {
        if modal.is_done() {
            break;
        }
        // Safety: message is writable, null hwnd processes all thread messages.
        let code = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if code == 0 {
            quit_code = Some(message.wParam as i32);
            break;
        }
        if code < 0 {
            break;
        }
        if handles_dialog_key(hwnd, &mut message, modal) {
            continue;
        }
        // Safety: message was filled by GetMessageW.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    if window_is_alive(hwnd) {
        // Safety: hwnd is the modal dialog created by this module.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
    if owner_was_enabled && window_is_alive(owner) {
        // Safety: owner is the live window disabled at modal start.
        unsafe { EnableWindow(owner, 1) };
    }
    if window_is_alive(hwnd) {
        // Safety: hwnd is the modal dialog created by this module.
        unsafe { DestroyWindow(hwnd) };
    }
    if owner_was_enabled {
        restore_modal_owner(owner);
    }
    if let Some(exit_code) = quit_code {
        // Safety: preserve the thread quit request consumed by the modal loop.
        unsafe { PostQuitMessage(exit_code) };
    }
}

fn handles_dialog_key<T>(hwnd: HWND, message: &mut MSG, modal: &ModalDialogState<T>) -> bool {
    if message.message == WM_KEYDOWN && message.wParam == usize::from(VK_ESCAPE) {
        complete_modal_dialog(hwnd, modal);
        return true;
    }
    if window_is_alive(hwnd) {
        // Safety: hwnd is the modal dialog for this loop and message was
        // returned by GetMessageW. IsDialogMessageW performs standard dialog
        // keyboard handling such as Tab traversal and default/cancel commands.
        unsafe { IsDialogMessageW(hwnd, message as *mut MSG) != 0 }
    } else {
        false
    }
}

fn complete_modal_dialog<T>(hwnd: HWND, modal: &ModalDialogState<T>) {
    modal.finish();
    if window_is_alive(hwnd) {
        // Safety: hwnd is the modal dialog being completed.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

fn window_is_alive(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    // Safety: hwnd is a handle value supplied by this UI code; IsWindow only
    // tests whether it identifies an existing window.
    unsafe { IsWindow(hwnd) != 0 }
}

fn restore_modal_owner(owner: HWND) {
    if !window_is_alive(owner) {
        return;
    }
    unsafe {
        // Safety: owner is a live top-level window owned by this UI thread.
        ShowWindow(owner, SW_SHOW);
        SetActiveWindow(owner);
        SetForegroundWindow(owner);
        SetFocus(owner);
    }
}

fn center_window_over_owner(owner: HWND, hwnd: HWND) {
    let mut owner_rect = RECT::default();
    let mut dialog_rect = RECT::default();
    let rects_available = unsafe {
        // Safety: owner and hwnd are live window handles during modal setup.
        GetWindowRect(owner, &mut owner_rect) != 0 && GetWindowRect(hwnd, &mut dialog_rect) != 0
    };
    if !rects_available {
        return;
    }

    let (x, y) = centered_window_position(owner_rect, dialog_rect);
    unsafe {
        // Safety: hwnd is a live dialog window. Only position changes; size and z-order are preserved.
        SetWindowPos(
            hwnd,
            null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

fn centered_window_position(owner: RECT, window: RECT) -> (i32, i32) {
    let owner_width = rect_span(owner.left, owner.right);
    let owner_height = rect_span(owner.top, owner.bottom);
    let window_width = rect_span(window.left, window.right);
    let window_height = rect_span(window.top, window.bottom);

    (
        owner.left + (owner_width - window_width) / 2,
        owner.top + (owner_height - window_height) / 2,
    )
}

#[derive(Debug, Clone, Copy)]
struct TabLayoutDefaults {
    rows: u16,
    cols: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TabLayoutSelection {
    rows: u16,
    cols: u16,
}

#[derive(Debug)]
struct TextInputDialogState {
    prompt: String,
    initial: String,
    modal: Rc<ModalDialogState<String>>,
    creation_state: *mut DialogCreateState<TextInputDialogState>,
    dpi_scale: f64,
}

fn text_input_dialog(
    owner: HWND,
    hinstance: HINSTANCE,
    title: &str,
    prompt: &str,
    initial: &str,
    dpi_scale: f64,
) -> Option<String> {
    let modal = ModalDialogState::new();
    let state = Box::new(TextInputDialogState {
        prompt: prompt.to_owned(),
        initial: initial.to_owned(),
        modal: Rc::clone(&modal),
        creation_state: null_mut(),
        dpi_scale,
    });
    let state_ptr = Box::into_raw(state);
    let mut create_state = DialogCreateState {
        state: state_ptr,
        destroyed: false,
    };
    unsafe { (*state_ptr).creation_state = &mut create_state };
    let class = wide_z(TEXT_INPUT_CLASS_NAME);
    let title = wide_z(title);
    let client_width = scale_px(dpi_scale, TEXT_DIALOG_CLIENT_WIDTH);
    let client_height = scale_px(dpi_scale, TEXT_DIALOG_CLIENT_HEIGHT);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            client_width,
            client_height,
            owner,
            null_mut(),
            hinstance,
            (&mut create_state as *mut DialogCreateState<TextInputDialogState>).cast(),
        )
    };
    if hwnd.is_null() {
        if !create_state.destroyed {
            unsafe { drop(Box::from_raw(state_ptr)) };
        }
        return None;
    }
    unsafe { (*state_ptr).creation_state = null_mut() };
    ensure_window_client_size(hwnd, client_width, client_height);
    run_modal_loop(owner, hwnd, modal.as_ref());
    modal.take_result()
}

unsafe extern "system" fn text_input_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let create_state =
                unsafe { (*create).lpCreateParams as *mut DialogCreateState<TextInputDialogState> };
            if !create_state.is_null() {
                let state = unsafe { (*create_state).state };
                if state.is_null() {
                    return 0;
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                // Let DefWindowProc apply lpWindowName and default non-client setup.
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
        }
        return 0;
    }

    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TextInputDialogState };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };
    match message {
        WM_CREATE => {
            create_text_input_controls(hwnd, state);
            0
        }
        WM_COMMAND => match loword(wparam) as i32 {
            ID_TEXT_OK | IDOK => {
                if let Some(text) = read_dialog_text(hwnd, ID_TEXT_INPUT) {
                    state.modal.set_result(text);
                    complete_modal_dialog(hwnd, state.modal.as_ref());
                }
                0
            }
            ID_TEXT_CANCEL | IDCANCEL => {
                complete_modal_dialog(hwnd, state.modal.as_ref());
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        },
        WM_CLOSE => {
            complete_modal_dialog(hwnd, state.modal.as_ref());
            0
        }
        WM_NCDESTROY => unsafe {
            if !(*state_ptr).creation_state.is_null() {
                (*(*state_ptr).creation_state).destroyed = true;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
            DefWindowProcW(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn create_text_input_controls(hwnd: HWND, state: &TextInputDialogState) {
    let scale = state.dpi_scale;
    let left = scale_px(scale, 16);
    let row_height = scale_px(scale, EDIT_DIALOG_ROW_HEIGHT);
    create_label(
        hwnd,
        &state.prompt,
        left,
        scale_px(scale, 16),
        scale_px(scale, 388),
        row_height,
    );
    create_edit(
        hwnd,
        ID_TEXT_INPUT,
        &state.initial,
        left,
        scale_px(scale, 48),
        scale_px(scale, 388),
        row_height,
    );
    create_button(
        hwnd,
        ID_TEXT_OK,
        "OK",
        ControlRect {
            x: scale_px(scale, 244),
            y: scale_px(scale, 96),
            width: scale_px(scale, 76),
            height: row_height,
        },
        true,
    );
    create_button(
        hwnd,
        ID_TEXT_CANCEL,
        "Cancel",
        ControlRect {
            x: scale_px(scale, 330),
            y: scale_px(scale, 96),
            width: scale_px(scale, 76),
            height: row_height,
        },
        false,
    );
    focus_dialog_control(hwnd, ID_TEXT_INPUT);
}

#[derive(Debug)]
struct TabLayoutDialogState {
    rows: u16,
    cols: u16,
    defaults: TabLayoutDefaults,
    modal: Rc<ModalDialogState<TabLayoutSelection>>,
    creation_state: *mut DialogCreateState<TabLayoutDialogState>,
    dpi_scale: f64,
}

fn tab_layout_dialog(
    owner: HWND,
    hinstance: HINSTANCE,
    rows: u16,
    cols: u16,
    defaults: TabLayoutDefaults,
    dpi_scale: f64,
) -> Option<TabLayoutSelection> {
    let modal = ModalDialogState::new();
    let state = Box::new(TabLayoutDialogState {
        rows,
        cols,
        defaults,
        modal: Rc::clone(&modal),
        creation_state: null_mut(),
        dpi_scale,
    });
    let state_ptr = Box::into_raw(state);
    let mut create_state = DialogCreateState {
        state: state_ptr,
        destroyed: false,
    };
    unsafe { (*state_ptr).creation_state = &mut create_state };
    let class = wide_z(TAB_LAYOUT_CLASS_NAME);
    let title = wide_z("Tab Layout");
    let client_width = scale_px(dpi_scale, LAYOUT_DIALOG_CLIENT_WIDTH);
    let client_height = scale_px(dpi_scale, LAYOUT_DIALOG_CLIENT_HEIGHT);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            client_width,
            client_height,
            owner,
            null_mut(),
            hinstance,
            (&mut create_state as *mut DialogCreateState<TabLayoutDialogState>).cast(),
        )
    };
    if hwnd.is_null() {
        if !create_state.destroyed {
            unsafe { drop(Box::from_raw(state_ptr)) };
        }
        return None;
    }
    unsafe { (*state_ptr).creation_state = null_mut() };
    ensure_window_client_size(hwnd, client_width, client_height);
    run_modal_loop(owner, hwnd, modal.as_ref());
    modal.take_result()
}

unsafe extern "system" fn tab_layout_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let create_state =
                unsafe { (*create).lpCreateParams as *mut DialogCreateState<TabLayoutDialogState> };
            if !create_state.is_null() {
                let state = unsafe { (*create_state).state };
                if state.is_null() {
                    return 0;
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                // Let DefWindowProc apply lpWindowName and default non-client setup.
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
        }
        return 0;
    }

    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TabLayoutDialogState };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };
    match message {
        WM_CREATE => {
            create_tab_layout_controls(hwnd, state);
            0
        }
        WM_COMMAND => match loword(wparam) as i32 {
            ID_LAYOUT_APPLY | IDOK => {
                match read_tab_layout_from_dialog(hwnd, state.defaults) {
                    Ok(selection) => {
                        state.modal.set_result(selection);
                        complete_modal_dialog(hwnd, state.modal.as_ref());
                    }
                    Err(error) => {
                        show_message(hwnd, "Tab Layout", &error.message, MB_OK | MB_ICONWARNING);
                        focus_dialog_control(hwnd, error.control_id);
                    }
                }
                0
            }
            ID_LAYOUT_CANCEL | IDCANCEL => {
                complete_modal_dialog(hwnd, state.modal.as_ref());
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        },
        WM_CLOSE => {
            complete_modal_dialog(hwnd, state.modal.as_ref());
            0
        }
        WM_NCDESTROY => unsafe {
            if !(*state_ptr).creation_state.is_null() {
                (*(*state_ptr).creation_state).destroyed = true;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
            DefWindowProcW(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn create_tab_layout_controls(hwnd: HWND, state: &TabLayoutDialogState) {
    let scale = state.dpi_scale;
    let left = scale_px(scale, 16);
    let label_width = scale_px(scale, 70);
    let edit_left = left + label_width + scale_px(scale, 8);
    let row_height = scale_px(scale, EDIT_DIALOG_ROW_HEIGHT);

    create_label(
        hwnd,
        "Rows",
        left,
        scale_px(scale, 18),
        label_width,
        row_height,
    );
    create_edit(
        hwnd,
        ID_LAYOUT_ROWS,
        &state.rows.to_string(),
        edit_left,
        scale_px(scale, 18),
        scale_px(scale, 80),
        row_height,
    );
    create_label(
        hwnd,
        "Cols",
        left,
        scale_px(scale, 56),
        label_width,
        row_height,
    );
    create_edit(
        hwnd,
        ID_LAYOUT_COLS,
        &state.cols.to_string(),
        edit_left,
        scale_px(scale, 56),
        scale_px(scale, 80),
        row_height,
    );
    create_button(
        hwnd,
        ID_LAYOUT_APPLY,
        "Apply",
        ControlRect {
            x: scale_px(scale, 92),
            y: scale_px(scale, 108),
            width: scale_px(scale, 76),
            height: row_height,
        },
        true,
    );
    create_button(
        hwnd,
        ID_LAYOUT_CANCEL,
        "Cancel",
        ControlRect {
            x: scale_px(scale, 178),
            y: scale_px(scale, 108),
            width: scale_px(scale, 76),
            height: row_height,
        },
        false,
    );
    focus_dialog_control(hwnd, ID_LAYOUT_ROWS);
}

#[derive(Debug)]
struct LayoutValidationError {
    message: String,
    control_id: i32,
}

fn read_tab_layout_from_dialog(
    hwnd: HWND,
    defaults: TabLayoutDefaults,
) -> std::result::Result<TabLayoutSelection, LayoutValidationError> {
    let rows_text = read_dialog_text(hwnd, ID_LAYOUT_ROWS).unwrap_or_default();
    let cols_text = read_dialog_text(hwnd, ID_LAYOUT_COLS).unwrap_or_default();
    let rows = parse_layout_value(
        &rows_text,
        defaults.rows,
        MAX_BUTTON_ROWS,
        "Rows",
        ID_LAYOUT_ROWS,
    )?;
    let cols = parse_layout_value(
        &cols_text,
        defaults.cols,
        MAX_BUTTON_COLS,
        "Cols",
        ID_LAYOUT_COLS,
    )?;
    Ok(TabLayoutSelection { rows, cols })
}

fn parse_layout_value(
    raw_value: &str,
    default: u16,
    max_value: u16,
    label: &str,
    control_id: i32,
) -> std::result::Result<u16, LayoutValidationError> {
    let text = raw_value.trim();
    if text.is_empty() {
        return Err(LayoutValidationError {
            message: format!("{label} is required."),
            control_id,
        });
    }
    let parsed = text.parse::<i64>().map_err(|_| LayoutValidationError {
        message: format!("{label} must be a whole number."),
        control_id,
    })?;
    if parsed < 1 {
        return Ok(default);
    }
    let parsed = u16::try_from(parsed).unwrap_or(u16::MAX);
    Ok(parsed.clamp(1, max_value))
}

fn focus_dialog_control(hwnd: HWND, control_id: i32) {
    let child = unsafe { GetDlgItem(hwnd, control_id) };
    if !child.is_null() {
        unsafe { SetFocus(child) };
    }
}

#[derive(Debug)]
struct HiddenItemsDialogState {
    items: Vec<HiddenItem>,
    modal: Rc<ModalDialogState<Vec<String>>>,
    creation_state: *mut DialogCreateState<HiddenItemsDialogState>,
    dpi_scale: f64,
}

fn hidden_items_dialog(
    owner: HWND,
    hinstance: HINSTANCE,
    tab: &LauncherTab,
    dpi_scale: f64,
) -> Option<Vec<String>> {
    let modal = ModalDialogState::new();
    let state = Box::new(HiddenItemsDialogState {
        items: hidden_items_for_tab(tab),
        modal: Rc::clone(&modal),
        creation_state: null_mut(),
        dpi_scale,
    });
    let state_ptr = Box::into_raw(state);
    let mut create_state = DialogCreateState {
        state: state_ptr,
        destroyed: false,
    };
    unsafe { (*state_ptr).creation_state = &mut create_state };
    let class = wide_z(HIDDEN_ITEMS_CLASS_NAME);
    let title = wide_z("Manage Hidden Items");
    let client_width = scale_px(dpi_scale, HIDDEN_DIALOG_CLIENT_WIDTH);
    let client_height = scale_px(dpi_scale, HIDDEN_DIALOG_CLIENT_HEIGHT);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            client_width,
            client_height,
            owner,
            null_mut(),
            hinstance,
            (&mut create_state as *mut DialogCreateState<HiddenItemsDialogState>).cast(),
        )
    };
    if hwnd.is_null() {
        if !create_state.destroyed {
            unsafe { drop(Box::from_raw(state_ptr)) };
        }
        return None;
    }
    unsafe { (*state_ptr).creation_state = null_mut() };
    ensure_window_client_size(hwnd, client_width, client_height);
    run_modal_loop(owner, hwnd, modal.as_ref());
    modal.take_result()
}

unsafe extern "system" fn hidden_items_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let create_state = unsafe {
                (*create).lpCreateParams as *mut DialogCreateState<HiddenItemsDialogState>
            };
            if !create_state.is_null() {
                let state = unsafe { (*create_state).state };
                if state.is_null() {
                    return 0;
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                // Let DefWindowProc apply lpWindowName and default non-client setup.
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
        }
        return 0;
    }

    let state_ptr =
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HiddenItemsDialogState };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };
    match message {
        WM_CREATE => {
            create_hidden_items_controls(hwnd, state);
            0
        }
        WM_COMMAND => match loword(wparam) as i32 {
            ID_HIDDEN_UNHIDE => {
                let selected = selected_hidden_item_ids(hwnd, &state.items);
                if !selected.is_empty() {
                    state.modal.set_result(selected);
                    complete_modal_dialog(hwnd, state.modal.as_ref());
                }
                0
            }
            ID_HIDDEN_CLOSE | IDCANCEL => {
                complete_modal_dialog(hwnd, state.modal.as_ref());
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        },
        WM_CLOSE => {
            complete_modal_dialog(hwnd, state.modal.as_ref());
            0
        }
        WM_NCDESTROY => unsafe {
            if !(*state_ptr).creation_state.is_null() {
                (*(*state_ptr).creation_state).destroyed = true;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
            DefWindowProcW(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn create_hidden_items_controls(hwnd: HWND, state: &HiddenItemsDialogState) {
    let scale = state.dpi_scale;
    let left = scale_px(scale, 10);
    let top = scale_px(scale, 10);
    let row_height = scale_px(scale, EDIT_DIALOG_ROW_HEIGHT);
    if state.items.is_empty() {
        create_label(
            hwnd,
            "No hidden items.",
            left,
            top,
            scale_px(scale, 330),
            row_height,
        );
    } else {
        let listbox = create_listbox(
            hwnd,
            ID_HIDDEN_LIST,
            ControlRect {
                x: left,
                y: top,
                width: scale_px(scale, 340),
                height: scale_px(scale, 220),
            },
        );
        if !listbox.is_null() {
            for item in &state.items {
                let label = wide_z(&item.label);
                unsafe { SendMessageW(listbox, LB_ADDSTRING, 0, label.as_ptr() as LPARAM) };
            }
        }
    }
    create_button(
        hwnd,
        ID_HIDDEN_UNHIDE,
        "Unhide Selected",
        ControlRect {
            x: left,
            y: scale_px(scale, 242),
            width: scale_px(scale, 132),
            height: row_height,
        },
        true,
    );
    create_button(
        hwnd,
        ID_HIDDEN_CLOSE,
        "Close",
        ControlRect {
            x: scale_px(scale, 274),
            y: scale_px(scale, 242),
            width: scale_px(scale, 76),
            height: row_height,
        },
        false,
    );
}

fn create_listbox(hwnd: HWND, id: i32, rect: ControlRect) -> HWND {
    unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("LISTBOX"),
            null(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | (LBS_EXTENDEDSEL as u32)
                | (LBS_NOINTEGRALHEIGHT as u32),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            hwnd,
            id as _,
            null_mut(),
            null(),
        )
    }
}

fn selected_hidden_item_ids(hwnd: HWND, items: &[HiddenItem]) -> Vec<String> {
    let listbox = unsafe { GetDlgItem(hwnd, ID_HIDDEN_LIST) };
    if listbox.is_null() {
        return Vec::new();
    }
    let count = unsafe { SendMessageW(listbox, LB_GETSELCOUNT, 0, 0) };
    if count <= 0 {
        return Vec::new();
    }
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    let mut indices = vec![0i32; count];
    let copied = unsafe {
        SendMessageW(
            listbox,
            LB_GETSELITEMS,
            count,
            indices.as_mut_ptr() as LPARAM,
        )
    };
    if copied <= 0 {
        return Vec::new();
    }
    let copied = usize::try_from(copied).unwrap_or(0).min(indices.len());
    selected_hidden_item_ids_from_indices(
        items,
        indices
            .into_iter()
            .take(copied)
            .filter_map(|index| usize::try_from(index).ok()),
    )
}

#[derive(Debug)]
struct AboutDialogState {
    modal: Rc<ModalDialogState<()>>,
    creation_state: *mut DialogCreateState<AboutDialogState>,
    dpi_scale: f64,
}

fn about_dialog(owner: HWND, hinstance: HINSTANCE, dpi_scale: f64) {
    let modal = ModalDialogState::new();
    let state = Box::new(AboutDialogState {
        modal: Rc::clone(&modal),
        creation_state: null_mut(),
        dpi_scale,
    });
    let state_ptr = Box::into_raw(state);
    let mut create_state = DialogCreateState {
        state: state_ptr,
        destroyed: false,
    };
    unsafe { (*state_ptr).creation_state = &mut create_state };
    let class = wide_z(ABOUT_CLASS_NAME);
    let title = wide_z(&format!("About {APP_DISPLAY_NAME}"));
    let client_width = scale_px(dpi_scale, ABOUT_DIALOG_CLIENT_WIDTH);
    let client_height = scale_px(dpi_scale, ABOUT_DIALOG_CLIENT_HEIGHT);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            client_width,
            client_height,
            owner,
            null_mut(),
            hinstance,
            (&mut create_state as *mut DialogCreateState<AboutDialogState>).cast(),
        )
    };
    if hwnd.is_null() {
        if !create_state.destroyed {
            unsafe { drop(Box::from_raw(state_ptr)) };
        }
        return;
    }
    unsafe { (*state_ptr).creation_state = null_mut() };
    ensure_window_client_size(hwnd, client_width, client_height);
    run_modal_loop(owner, hwnd, modal.as_ref());
}

unsafe extern "system" fn about_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let create_state =
                unsafe { (*create).lpCreateParams as *mut DialogCreateState<AboutDialogState> };
            if !create_state.is_null() {
                let state = unsafe { (*create_state).state };
                if state.is_null() {
                    return 0;
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
        }
        return 0;
    }

    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutDialogState };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };
    match message {
        WM_CREATE => {
            create_about_controls(hwnd, state);
            0
        }
        WM_COMMAND => match loword(wparam) as i32 {
            ID_ABOUT_LINK => {
                if let Err(error) = crate::platform::windows::shell::open_path(APP_AUTHOR_URL) {
                    show_message(
                        hwnd,
                        "About",
                        &format!(
                            "브라우저에서 링크를 열 수 없습니다:\n{}",
                            error.user_message()
                        ),
                        MB_OK | MB_ICONWARNING,
                    );
                }
                0
            }
            ID_ABOUT_CLOSE | IDOK | IDCANCEL => {
                complete_modal_dialog(hwnd, state.modal.as_ref());
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        },
        WM_CLOSE => {
            complete_modal_dialog(hwnd, state.modal.as_ref());
            0
        }
        WM_NCDESTROY => unsafe {
            if !(*state_ptr).creation_state.is_null() {
                (*(*state_ptr).creation_state).destroyed = true;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
            DefWindowProcW(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn create_about_controls(hwnd: HWND, state: &AboutDialogState) {
    let scale = state.dpi_scale;
    let left = scale_px(scale, 18);
    let row_height = scale_px(scale, EDIT_DIALOG_ROW_HEIGHT);
    let content_width = scale_px(scale, ABOUT_DIALOG_CLIENT_WIDTH - 36);
    let close_width = scale_px(scale, 76);
    let close_y = scale_px(
        scale,
        ABOUT_DIALOG_CLIENT_HEIGHT - EDIT_DIALOG_BOTTOM_PADDING - EDIT_DIALOG_ROW_HEIGHT,
    );
    let license_text_y = scale_px(scale, 84);
    let license_text_height = scale_px(
        scale,
        ABOUT_DIALOG_CLIENT_HEIGHT - EDIT_DIALOG_BOTTOM_PADDING - EDIT_DIALOG_ROW_HEIGHT - 18 - 84,
    );

    create_label_with_id(
        hwnd,
        ID_ABOUT_VERSION,
        &format!("Version {APP_VERSION}"),
        left,
        scale_px(scale, 18),
        content_width,
        row_height,
    );
    create_button(
        hwnd,
        ID_ABOUT_LINK,
        APP_AUTHOR_URL,
        ControlRect {
            x: left,
            y: scale_px(scale, 46),
            width: content_width,
            height: row_height,
        },
        false,
    );
    create_readonly_multiline_edit(
        hwnd,
        ID_ABOUT_LICENSES,
        APP_ABOUT_TEXT,
        ControlRect {
            x: left,
            y: license_text_y,
            width: content_width,
            height: license_text_height,
        },
    );
    create_button(
        hwnd,
        ID_ABOUT_CLOSE,
        "Close",
        ControlRect {
            x: scale_px(scale, ABOUT_DIALOG_CLIENT_WIDTH - 18 - 76),
            y: close_y,
            width: close_width,
            height: row_height,
        },
        true,
    );
    focus_dialog_control(hwnd, ID_ABOUT_LINK);
}

unsafe extern "system" fn edit_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            // Safety: WM_NCCREATE lparam points to CREATESTRUCTW.
            let create_state =
                unsafe { (*create).lpCreateParams as *mut DialogCreateState<EditDialogState> };
            if !create_state.is_null() {
                let state = unsafe { (*create_state).state };
                if state.is_null() {
                    return 0;
                }
                // Safety: store state pointer for the dialog lifetime.
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                // Let DefWindowProc apply lpWindowName and default non-client setup.
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
        }
        return 0;
    }

    // Safety: GWLP_USERDATA contains EditDialogState or 0.
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut EditDialogState };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };
    match message {
        WM_CREATE => {
            if create_edit_dialog_controls(hwnd, state) {
                0
            } else {
                show_message(
                    hwnd,
                    "Edit Button",
                    "Could not create the edit fields.",
                    MB_OK | MB_ICONERROR,
                );
                -1
            }
        }
        WM_COMMAND => {
            let command_id = loword(wparam) as i32;
            match command_id {
                ID_EDIT_OK | IDOK => {
                    let auto_enter = state.initial.auto_enter;
                    if let Some(info) = read_button_info_from_dialog(hwnd, auto_enter) {
                        state.modal.set_result(info);
                        complete_modal_dialog(hwnd, state.modal.as_ref());
                    } else {
                        show_message(
                            hwnd,
                            "Edit Button",
                            "Could not read the edit fields.",
                            MB_OK | MB_ICONERROR,
                        );
                    }
                    0
                }
                ID_EDIT_CANCEL | IDCANCEL => {
                    complete_modal_dialog(hwnd, state.modal.as_ref());
                    0
                }
                _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
            }
        }
        WM_CLOSE => {
            complete_modal_dialog(hwnd, state.modal.as_ref());
            0
        }
        WM_NCDESTROY => unsafe {
            if !(*state_ptr).creation_state.is_null() {
                (*(*state_ptr).creation_state).destroyed = true;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
            DefWindowProcW(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn create_edit_dialog_controls(hwnd: HWND, state: &EditDialogState) -> bool {
    let scale = state.dpi_scale;
    let left = scale_px(scale, 16);
    let label_width = scale_px(scale, 70);
    let edit_left = left + label_width + scale_px(scale, 8);
    let edit_width = scale_px(scale, 330);
    let row_height = scale_px(scale, EDIT_DIALOG_ROW_HEIGHT);
    let mut y = scale_px(scale, 18);

    create_label(hwnd, "Name", left, y, label_width, row_height);
    if create_edit(
        hwnd,
        ID_EDIT_NAME,
        &state.initial.name,
        edit_left,
        y,
        edit_width,
        row_height,
    )
    .is_none()
    {
        return false;
    }
    y += scale_px(scale, 36);
    create_label(hwnd, "Path", left, y, label_width, row_height);
    if create_edit(
        hwnd,
        ID_EDIT_PATH,
        &state.initial.path,
        edit_left,
        y,
        edit_width,
        row_height,
    )
    .is_none()
    {
        return false;
    }
    y += scale_px(scale, 36);
    create_label(hwnd, "Params", left, y, label_width, row_height);
    if create_edit(
        hwnd,
        ID_EDIT_PARAMS,
        &state.initial.params,
        edit_left,
        y,
        edit_width,
        row_height,
    )
    .is_none()
    {
        return false;
    }
    y += scale_px(scale, 42);
    create_checkbox(
        hwnd,
        ID_CHECK_ADMIN,
        "Run as administrator",
        ControlRect {
            x: edit_left,
            y,
            width: edit_width,
            height: row_height,
        },
        state.initial.admin,
    );
    y += scale_px(scale, 30);
    create_checkbox(
        hwnd,
        ID_CHECK_COPY,
        "Copy Path + Params",
        ControlRect {
            x: edit_left,
            y,
            width: edit_width,
            height: row_height,
        },
        state.initial.action == 1,
    );

    let button_y = scale_px(scale, EDIT_DIALOG_BUTTON_Y);
    create_button(
        hwnd,
        ID_EDIT_OK,
        "OK",
        ControlRect {
            x: scale_px(scale, 268),
            y: button_y,
            width: scale_px(scale, 76),
            height: row_height,
        },
        true,
    );
    create_button(
        hwnd,
        ID_EDIT_CANCEL,
        "Cancel",
        ControlRect {
            x: scale_px(scale, 354),
            y: button_y,
            width: scale_px(scale, 76),
            height: row_height,
        },
        false,
    );
    focus_dialog_control(hwnd, ID_EDIT_NAME);
    true
}

fn edit_dialog_min_client_size(scale: f64) -> (i32, i32) {
    let required_height =
        EDIT_DIALOG_BUTTON_Y + EDIT_DIALOG_ROW_HEIGHT + EDIT_DIALOG_BOTTOM_PADDING;
    (
        scale_px(scale, EDIT_DIALOG_CLIENT_WIDTH),
        scale_px(scale, EDIT_DIALOG_CLIENT_HEIGHT.max(required_height)),
    )
}

fn ensure_window_client_size(hwnd: HWND, min_client_width: i32, min_client_height: i32) {
    let mut client = RECT::default();
    let mut window = RECT::default();
    let rects_available = unsafe {
        // Safety: hwnd is the dialog created by this module and rect pointers are writable.
        GetClientRect(hwnd, &mut client) != 0 && GetWindowRect(hwnd, &mut window) != 0
    };
    if !rects_available {
        return;
    }

    let client_width = rect_span(client.left, client.right);
    let client_height = rect_span(client.top, client.bottom);
    let width_delta = min_client_width.saturating_sub(client_width);
    let height_delta = min_client_height.saturating_sub(client_height);
    if width_delta == 0 && height_delta == 0 {
        return;
    }

    let window_width = rect_span(window.left, window.right)
        .saturating_add(width_delta)
        .max(1);
    let window_height = rect_span(window.top, window.bottom)
        .saturating_add(height_delta)
        .max(1);
    unsafe {
        // Safety: hwnd is a live dialog window and the new size only expands it
        // enough for its client-area controls.
        MoveWindow(
            hwnd,
            window.left,
            window.top,
            window_width,
            window_height,
            0,
        )
    };
}

fn rect_span(start: i32, end: i32) -> i32 {
    end.saturating_sub(start).max(0)
}

fn inset_rect(rect: RECT, inset: i32) -> RECT {
    let horizontal_inset = inset.min(rect_span(rect.left, rect.right) / 2);
    let vertical_inset = inset.min(rect_span(rect.top, rect.bottom) / 2);
    RECT {
        left: rect.left.saturating_add(horizontal_inset),
        top: rect.top.saturating_add(vertical_inset),
        right: rect.right.saturating_sub(horizontal_inset),
        bottom: rect.bottom.saturating_sub(vertical_inset),
    }
}

#[derive(Clone, Copy)]
struct ButtonContentLayout {
    icon_rect: Option<RECT>,
    text_rect: RECT,
}

fn button_content_layout(
    content_rect: RECT,
    icon_size: Option<i32>,
    icon_gap: i32,
    text_height: i32,
) -> ButtonContentLayout {
    let icon_size = icon_size.filter(|size| *size > 0);
    let text_height = text_height.max(0);
    let gap = if icon_size.is_some() && text_height > 0 {
        icon_gap.max(0)
    } else {
        0
    };
    let group_height = icon_size
        .unwrap_or(0)
        .saturating_add(gap)
        .saturating_add(text_height);
    let content_width = rect_span(content_rect.left, content_rect.right);
    let content_height = rect_span(content_rect.top, content_rect.bottom);
    let group_top = content_rect
        .top
        .saturating_add(content_height.saturating_sub(group_height) / 2);
    let icon_rect = icon_size.map(|size| {
        let icon_left = content_rect
            .left
            .saturating_add(content_width.saturating_sub(size) / 2);
        RECT {
            left: icon_left,
            top: group_top,
            right: icon_left.saturating_add(size),
            bottom: group_top.saturating_add(size),
        }
    });
    let text_top = group_top
        .saturating_add(icon_size.unwrap_or(0))
        .saturating_add(gap);
    ButtonContentLayout {
        icon_rect,
        text_rect: RECT {
            left: content_rect.left,
            top: text_top,
            right: content_rect.right,
            bottom: text_top.saturating_add(text_height),
        },
    }
}

fn measure_button_text_height(hdc: HDC, label: &[u16], width: i32) -> i32 {
    if hdc.is_null() || label.len() <= 1 {
        return 0;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width.max(1),
        bottom: 0,
    };
    // Safety: hdc is valid during WM_DRAWITEM, label is NUL-terminated by wide_z,
    // and rect points to writable stack memory for DrawTextW's synchronous call.
    unsafe {
        DrawTextW(
            hdc,
            label.as_ptr(),
            -1,
            &mut rect,
            DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
        );
    }
    rect_span(rect.top, rect.bottom)
}

fn create_label(hwnd: HWND, text: &str, x: i32, y: i32, width: i32, height: i32) {
    create_label_with_id(hwnd, 0, text, x, y, width, height);
}

fn create_label_with_id(hwnd: HWND, id: i32, text: &str, x: i32, y: i32, width: i32, height: i32) {
    let text = wide_z(text);
    let control_id = if id == 0 { null_mut() } else { id as _ };
    unsafe {
        CreateWindowExW(
            0,
            w!("STATIC"),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            width,
            height,
            hwnd,
            control_id,
            null_mut(),
            null(),
        )
    };
}

fn create_edit(
    hwnd: HWND,
    id: i32,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Option<HWND> {
    let text = wide_z(text);
    let edit = unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | (ES_AUTOHSCROLL as u32),
            x,
            y,
            width,
            height,
            hwnd,
            id as _,
            null_mut(),
            null(),
        )
    };
    if edit.is_null() {
        None
    } else {
        unsafe {
            SendMessageW(edit, EM_SETLIMITTEXT, EDIT_DIALOG_TEXT_LIMIT, 0);
        }
        Some(edit)
    }
}

fn create_readonly_multiline_edit(hwnd: HWND, id: i32, text: &str, rect: ControlRect) -> HWND {
    let text = wide_z(&win32_multiline_text(text));
    unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            text.as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | (ES_MULTILINE as u32)
                | (ES_AUTOVSCROLL as u32)
                | (ES_READONLY as u32),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            hwnd,
            id as _,
            null_mut(),
            null(),
        )
    }
}

fn win32_multiline_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn create_checkbox(hwnd: HWND, id: i32, text: &str, rect: ControlRect, checked: bool) {
    let text = wide_z(text);
    let checkbox = unsafe {
        CreateWindowExW(
            0,
            w!("BUTTON"),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_AUTOCHECKBOX as u32),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            hwnd,
            id as _,
            null_mut(),
            null(),
        )
    };
    if !checkbox.is_null() && checked {
        unsafe { SendMessageW(checkbox, BM_SETCHECK, 1, 0) };
    }
}

fn create_button(hwnd: HWND, id: i32, text: &str, rect: ControlRect, default: bool) {
    let text = wide_z(text);
    let style = if default {
        BS_DEFPUSHBUTTON
    } else {
        BS_PUSHBUTTON
    };
    unsafe {
        CreateWindowExW(
            0,
            w!("BUTTON"),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (style as u32),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            hwnd,
            id as _,
            null_mut(),
            null(),
        )
    };
}

fn read_button_info_from_dialog(hwnd: HWND, auto_enter: bool) -> Option<ButtonInfo> {
    Some(ButtonInfo {
        name: read_dialog_text(hwnd, ID_EDIT_NAME)?,
        path: read_dialog_text(hwnd, ID_EDIT_PATH)?,
        params: read_dialog_text(hwnd, ID_EDIT_PARAMS)?,
        admin: checkbox_checked(hwnd, ID_CHECK_ADMIN),
        action: if checkbox_checked(hwnd, ID_CHECK_COPY) {
            1
        } else {
            0
        },
        auto_enter,
    })
}

fn read_dialog_text(hwnd: HWND, id: i32) -> Option<String> {
    let child = unsafe { GetDlgItem(hwnd, id) };
    if child.is_null() {
        return None;
    }

    unsafe { SetLastError(ERROR_SUCCESS) };
    let text_len = unsafe { GetWindowTextLengthW(child) };
    if text_len == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
        return None;
    }
    let text_len = usize::try_from(text_len).ok()?;
    if text_len == 0 {
        return Some(String::new());
    }
    let buffer_len = text_len.checked_add(2)?;
    if buffer_len > i32::MAX as usize {
        return None;
    }
    let mut buffer = vec![0u16; buffer_len];
    // Safety: hwnd is the dialog, id names an existing edit control, and buffer is writable.
    let read_len = unsafe { GetDlgItemTextW(hwnd, id, buffer.as_mut_ptr(), buffer_len as i32) };
    let read_len = usize::try_from(read_len).ok()?;
    if read_len == 0 || read_len >= buffer_len.saturating_sub(1) {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..read_len]))
}

fn checkbox_checked(hwnd: HWND, id: i32) -> bool {
    // Safety: GetDlgItem returns a borrowed child HWND or null.
    let child = unsafe { GetDlgItem(hwnd, id) };
    if child.is_null() {
        return false;
    }
    // Safety: BM_GETCHECK is valid for button controls.
    unsafe { SendMessageW(child, BM_GETCHECK, 0, 0) == 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn menu_command_ids_round_trip_and_remain_unique() {
        let commands = crate::ui::common::main_menu_items()
            .map(|item| item.command)
            .collect::<Vec<_>>();
        let mut ids = commands
            .iter()
            .map(|command| menu_command_id(*command))
            .collect::<Vec<_>>();
        let original_len = ids.len();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), original_len);
        for command in commands {
            assert_eq!(
                menu_command_from_id(menu_command_id(command)),
                Some(command)
            );
        }
    }

    #[test]
    fn context_menu_command_ids_round_trip_and_remain_unique() {
        let commands = BUTTON_CONTEXT_MENU_ITEMS
            .iter()
            .map(|item| item.command)
            .collect::<Vec<_>>();
        let mut ids = commands
            .iter()
            .map(|command| context_menu_command_id(*command))
            .collect::<Vec<_>>();
        let original_len = ids.len();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), original_len);
        for command in commands {
            assert_eq!(
                context_menu_command_from_id(context_menu_command_id(command)),
                Some(command)
            );
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_context_menu_command_override_supports_known_commands_only() {
        assert_eq!(
            debug_context_menu_command_override_from_env(Some("Edit".into())),
            Some(ButtonContextCommand::Edit)
        );
        assert_eq!(
            debug_context_menu_command_override_from_env(Some("OpenInExplorer".into())),
            Some(ButtonContextCommand::OpenInExplorer)
        );
        assert_eq!(
            debug_context_menu_command_override_from_env(Some("Hide".into())),
            Some(ButtonContextCommand::Hide)
        );
        assert_eq!(
            debug_context_menu_command_override_from_env(Some("Unknown".into())),
            None
        );
        assert_eq!(debug_context_menu_command_override_from_env(None), None);
    }

    #[test]
    fn win32_accelerators_cover_shared_accelerated_menu_commands() {
        assert_eq!(
            main_accelerator_specs(),
            [
                (FVIRTKEY | FCONTROL | FSHIFT, VK_LEFT, ID_MENU_MOVE_LEFT),
                (FVIRTKEY | FCONTROL | FSHIFT, VK_RIGHT, ID_MENU_MOVE_RIGHT),
                (FVIRTKEY | FCONTROL, VK_PRIOR, ID_MENU_SELECT_PREV),
                (FVIRTKEY | FCONTROL, VK_NEXT, ID_MENU_SELECT_NEXT),
                (FVIRTKEY, VK_F5, ID_MENU_SORT),
            ]
        );

        let accelerated_commands = MAIN_MENU_SECTIONS
            .iter()
            .flat_map(|section| section.iter())
            .filter(|item| !item.gtk_accels.is_empty())
            .map(|item| item.command)
            .collect::<Vec<_>>();
        let accelerator_commands = main_accelerator_specs()
            .into_iter()
            .map(|(_, _, command_id)| {
                menu_command_from_id(command_id).expect("accelerator command id")
            })
            .collect::<Vec<_>>();

        assert_eq!(accelerator_commands, accelerated_commands);
    }

    #[test]
    fn size_move_dpi_state_defers_and_resets_pending_dpi() {
        let mut state = SizeMoveDpiState::default();

        state.defer_dpi_change(144);
        assert!(!state.has_pending_dpi_change());

        state.enter();
        state.defer_dpi_change(144);
        assert!(state.has_pending_dpi_change());

        let exit = state.exit();

        assert_eq!(exit.pending_dpi, Some(144));
        assert_eq!(state, SizeMoveDpiState::default());
    }

    #[test]
    fn size_move_dpi_state_keeps_latest_pending_dpi_and_clamps_low_values() {
        let mut state = SizeMoveDpiState::default();

        state.enter();
        state.defer_dpi_change(120);
        state.defer_dpi_change(72);

        assert_eq!(state.exit().pending_dpi, Some(96));
    }

    #[test]
    fn edit_dialog_client_height_keeps_action_buttons_visible() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (_, client_height) = edit_dialog_min_client_size(scale);
            let button_bottom = scale_px(scale, EDIT_DIALOG_BUTTON_Y)
                .saturating_add(scale_px(scale, EDIT_DIALOG_ROW_HEIGHT));
            let required_bottom =
                button_bottom.saturating_add(scale_px(scale, EDIT_DIALOG_BOTTOM_PADDING));

            assert!(client_height >= required_bottom);
        }
    }

    #[test]
    fn about_dialog_client_height_keeps_license_controls_visible() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let client_height = scale_px(scale, ABOUT_DIALOG_CLIENT_HEIGHT);
            let close_y = scale_px(
                scale,
                ABOUT_DIALOG_CLIENT_HEIGHT - EDIT_DIALOG_BOTTOM_PADDING - EDIT_DIALOG_ROW_HEIGHT,
            );
            let licenses_height = scale_px(
                scale,
                ABOUT_DIALOG_CLIENT_HEIGHT
                    - EDIT_DIALOG_BOTTOM_PADDING
                    - EDIT_DIALOG_ROW_HEIGHT
                    - 18
                    - 84,
            );
            let version_bottom =
                scale_px(scale, 18).saturating_add(scale_px(scale, EDIT_DIALOG_ROW_HEIGHT));
            let link_bottom =
                scale_px(scale, 46).saturating_add(scale_px(scale, EDIT_DIALOG_ROW_HEIGHT));
            let licenses_bottom = scale_px(scale, 84).saturating_add(licenses_height);
            let close_bottom = close_y.saturating_add(scale_px(scale, EDIT_DIALOG_ROW_HEIGHT));

            assert!(client_height >= version_bottom);
            assert!(client_height >= link_bottom);
            assert!(client_height >= licenses_bottom);
            assert!(client_height >= close_bottom);
        }
    }

    #[test]
    fn win32_multiline_text_normalizes_line_endings_for_edit_control() {
        assert_eq!(
            win32_multiline_text("one\ntwo\r\nthree"),
            "one\r\ntwo\r\nthree"
        );
    }

    #[test]
    fn modal_loop_escape_key_marks_dialog_done() {
        let mut message = MSG {
            message: WM_KEYDOWN,
            wParam: usize::from(VK_ESCAPE),
            ..MSG::default()
        };
        let modal = ModalDialogState::<()>::new();

        assert!(handles_dialog_key(null_mut(), &mut message, modal.as_ref()));
        assert!(modal.is_done());
    }

    #[test]
    fn tab_layout_value_validation_matches_original_policy() {
        let empty = parse_layout_value(
            "",
            DEFAULT_BUTTON_ROWS,
            MAX_BUTTON_ROWS,
            "Rows",
            ID_LAYOUT_ROWS,
        );
        assert!(empty.is_err());
        assert_eq!(
            parse_layout_value(
                "-5",
                DEFAULT_BUTTON_ROWS,
                MAX_BUTTON_ROWS,
                "Rows",
                ID_LAYOUT_ROWS
            )
            .ok(),
            Some(DEFAULT_BUTTON_ROWS)
        );
        assert_eq!(
            parse_layout_value(
                "999",
                DEFAULT_BUTTON_COLS,
                MAX_BUTTON_COLS,
                "Cols",
                ID_LAYOUT_COLS
            )
            .ok(),
            Some(MAX_BUTTON_COLS)
        );
    }

    #[test]
    fn hidden_item_dialog_labels_use_button_display_name() {
        let tab = LauncherTab {
            id: String::from("tab-1"),
            tab_type: TabType::Folder,
            title: String::from("Tools"),
            folder_path: String::from("C:\\Tools"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: DEFAULT_BUTTON_COLS,
            hidden_item_ids: vec![String::from("item-a"), String::from("missing")],
            slot_positions: BTreeMap::new(),
            buttons: vec![LauncherButton {
                item_id: String::from("item-a"),
                name: String::from("Alpha"),
                source_name: String::from("Alpha.exe"),
                ..LauncherButton::manual_default()
            }],
            scan_signature: None,
            scan_item_order: None,
        };

        let items = hidden_items_for_tab(&tab);

        assert_eq!(
            items,
            vec![
                HiddenItem {
                    item_id: String::from("item-a"),
                    label: String::from("Alpha")
                },
                HiddenItem {
                    item_id: String::from("missing"),
                    label: String::from("missing")
                }
            ]
        );
    }

    #[test]
    fn visible_button_slots_ignores_out_of_bounds_folder_slots() {
        let mut slot_positions = BTreeMap::new();
        slot_positions.insert(String::from("item-a"), 4_294_967_295);
        let tab = LauncherTab {
            id: String::from("tab-1"),
            tab_type: TabType::Folder,
            title: String::from("Tools"),
            folder_path: String::from("C:\\Tools"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: 1,
            hidden_item_ids: Vec::new(),
            slot_positions,
            buttons: vec![LauncherButton {
                item_id: String::from("item-a"),
                name: String::from("Alpha"),
                source_name: String::from("Alpha.exe"),
                ..LauncherButton::manual_default()
            }],
            scan_signature: None,
            scan_item_order: None,
        };

        let slots = visible_button_slots(&tab);

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].button_idx, 0);
        assert_eq!(slots[0].slot_idx, 0);
    }

    #[test]
    fn visible_button_slots_keeps_sparse_valid_folder_slots() {
        let mut slot_positions = BTreeMap::new();
        slot_positions.insert(String::from("item-a"), 128);
        slot_positions.insert(String::from("item-b"), 128);
        slot_positions.insert(String::from("item-hidden"), max_button_slot_index(32));
        slot_positions.insert(String::from("item-c"), max_button_slot_index(32) + 1);
        let button = |item_id: &str, name: &str| LauncherButton {
            item_id: String::from(item_id),
            name: String::from(name),
            source_name: format!("{name}.exe"),
            ..LauncherButton::manual_default()
        };
        let tab = LauncherTab {
            id: String::from("tab-1"),
            tab_type: TabType::Folder,
            title: String::from("Tools"),
            folder_path: String::from("C:\\Tools"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: 32,
            hidden_item_ids: vec![String::from("item-hidden")],
            slot_positions,
            buttons: vec![
                button("item-a", "Alpha"),
                button("item-b", "Beta"),
                button("item-hidden", "Hidden"),
                button("item-c", "Gamma"),
            ],
            scan_signature: None,
            scan_item_order: None,
        };
        let mut slots = Vec::new();
        let mut scratch = VisibleButtonSlotScratch::default();

        collect_visible_button_slots(&tab, &mut slots, &mut scratch);

        let rendered_slots = slots
            .iter()
            .map(|slot| (slot.button_idx, slot.slot_idx))
            .collect::<Vec<_>>();
        assert_eq!(rendered_slots, vec![(1, 0), (3, 1), (0, 128)]);
    }

    #[test]
    fn rect_span_treats_inverted_rects_as_empty() {
        assert_eq!(rect_span(10, 30), 20);
        assert_eq!(rect_span(30, 10), 0);
    }

    #[test]
    fn inset_rect_clamps_to_available_size() {
        let rect = inset_rect(
            RECT {
                left: 0,
                top: 0,
                right: 10,
                bottom: 4,
            },
            3,
        );

        assert_eq!((rect.left, rect.top, rect.right, rect.bottom), (3, 2, 7, 2));
    }

    #[test]
    fn button_content_layout_centers_icon_and_text_as_group() {
        let layout = button_content_layout(
            RECT {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            Some(20),
            4,
            16,
        );
        let icon = layout.icon_rect.unwrap_or_default();

        assert_eq!(
            (icon.left, icon.top, icon.right, icon.bottom),
            (40, 30, 60, 50)
        );
        assert_eq!(
            (
                layout.text_rect.left,
                layout.text_rect.top,
                layout.text_rect.right,
                layout.text_rect.bottom
            ),
            (0, 54, 100, 70)
        );
    }

    #[test]
    fn button_content_layout_centers_text_without_icon() {
        let layout = button_content_layout(
            RECT {
                left: 10,
                top: 20,
                right: 110,
                bottom: 120,
            },
            None,
            4,
            18,
        );

        assert!(layout.icon_rect.is_none());
        assert_eq!(
            (
                layout.text_rect.left,
                layout.text_rect.top,
                layout.text_rect.right,
                layout.text_rect.bottom
            ),
            (10, 61, 110, 79)
        );
    }

    #[test]
    fn centered_window_position_centers_dialog_over_owner() {
        let owner = RECT {
            left: 100,
            top: 50,
            right: 900,
            bottom: 650,
        };
        let dialog = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 200,
        };

        assert_eq!(centered_window_position(owner, dialog), (350, 250));
    }

    #[test]
    fn button_icon_result_index_matches_generation_then_fallback_lookup() {
        let key = test_button_icon_key("same");
        let other_key = test_button_icon_key("other");
        let missing_key = test_button_icon_key("missing");
        let buttons = vec![
            test_button_control(10, Some(key.clone())),
            test_button_control(20, Some(other_key.clone())),
            test_button_control(30, Some(key.clone())),
            test_button_control(40, None),
        ];
        let index = ButtonIconResultButtonIndex::from_buttons(&buttons);

        assert_eq!(index.position_for(30, &key), Some(2));
        assert_eq!(
            index.position_for(30, &key),
            find_icon_result_button_position(&buttons, 30, &key)
        );
        assert_eq!(index.position_for(99, &key), Some(0));
        assert_eq!(
            index.position_for(99, &key),
            find_icon_result_button_position(&buttons, 99, &key)
        );
        assert_eq!(index.position_for(99, &other_key), Some(1));
        assert_eq!(index.position_for(10, &missing_key), None);
    }

    #[test]
    fn icon_path_falls_back_to_existing_source_path()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("source-path")?;
        let source = temp.path().join("tool.exe");
        fs::write(&source, b"icon target")?;
        let button = LauncherButton {
            name: String::from("Tool"),
            path: String::from("missing\\tool.exe"),
            source_path: source.to_string_lossy().into_owned(),
            ..LauncherButton::manual_default()
        };

        let resolved =
            resolve_button_icon_path_with(&button, temp.path(), str::to_owned, None, None);

        assert_eq!(resolved.as_deref(), Some(source.as_path()));
        Ok(())
    }

    #[test]
    fn icon_path_resolves_command_through_path_and_pathext()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("path-command")?;
        let tool = temp.path().join("tool.EXE");
        fs::write(&tool, b"icon target")?;
        let button = LauncherButton {
            name: String::from("Tool"),
            path: String::from("tool"),
            ..LauncherButton::manual_default()
        };

        let resolved = resolve_button_icon_path_with(
            &button,
            Path::new("C:\\unused"),
            str::to_owned,
            Some(temp.path().as_os_str().to_owned()),
            Some(".EXE;.CMD".into()),
        );

        assert_eq!(resolved.as_deref(), Some(tool.as_path()));
        Ok(())
    }

    #[test]
    fn icon_path_resolver_reuses_search_context_for_multiple_commands()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("path-context")?;
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir)?;
        let indexed_tool = bin_dir.join("indexed.CMD");
        fs::write(&indexed_tool, b"icon target")?;
        let mut resolver = ButtonIconPathResolver::from_env(
            Some(bin_dir.as_os_str().to_owned()),
            Some(".EXE;.CMD".into()),
        );

        for index in 0..PATH_DIRECTORY_INDEX_LOOKUP_THRESHOLD {
            let button = LauncherButton {
                name: format!("Missing {index}"),
                path: format!("missing-{index}"),
                ..LauncherButton::manual_default()
            };

            let resolved = resolve_button_icon_path_in_context(
                &button,
                Path::new("C:\\unused"),
                str::to_owned,
                &mut resolver,
            );

            assert!(resolved.is_none());
        }

        let button = LauncherButton {
            name: String::from("Indexed"),
            path: String::from("indexed"),
            ..LauncherButton::manual_default()
        };

        let resolved = resolve_button_icon_path_in_context(
            &button,
            Path::new("C:\\unused"),
            str::to_owned,
            &mut resolver,
        );

        assert_eq!(resolved.as_deref(), Some(indexed_tool.as_path()));
        Ok(())
    }

    #[test]
    fn icon_path_skips_copy_buttons() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("copy")?;
        let tool = temp.path().join("tool.exe");
        fs::write(&tool, b"icon target")?;
        let button = LauncherButton {
            name: String::from("Tool"),
            path: tool.to_string_lossy().into_owned(),
            action: 1,
            ..LauncherButton::manual_default()
        };

        let resolved =
            resolve_button_icon_path_with(&button, temp.path(), str::to_owned, None, None);

        assert!(resolved.is_none());
        Ok(())
    }

    #[test]
    fn button_icon_cache_key_changes_when_path_candidate_changes() {
        let tab = LauncherTab {
            id: String::from("tab-1"),
            tab_type: TabType::Folder,
            title: String::from("Tools"),
            folder_path: String::from("C:\\Tools"),
            rows: DEFAULT_BUTTON_ROWS,
            cols: DEFAULT_BUTTON_COLS,
            hidden_item_ids: Vec::new(),
            slot_positions: BTreeMap::new(),
            buttons: Vec::new(),
            scan_signature: None,
            scan_item_order: None,
        };
        let mut button = LauncherButton {
            item_id: String::from("item-a"),
            name: String::from("Tool"),
            path: String::from("tool"),
            ..LauncherButton::manual_default()
        };
        let base_dir = Path::new("C:\\Tools");

        let first_path_key = button_icon_cache_path(&button, base_dir).unwrap_or_default();
        let first_key = build_button_icon_key(tab.id.as_str(), &button, 0, &first_path_key, 20);
        button.path = String::from("other-tool");
        let second_path_key = button_icon_cache_path(&button, base_dir).unwrap_or_default();
        let second_key = build_button_icon_key(tab.id.as_str(), &button, 0, &second_path_key, 20);

        assert_ne!(first_key, second_key);
    }

    #[test]
    fn button_icon_cache_path_includes_base_dir_for_local_resolution() {
        let button = LauncherButton {
            name: String::from("Tool"),
            path: String::from("tool.exe"),
            ..LauncherButton::manual_default()
        };

        let first = button_icon_cache_path(&button, Path::new("C:\\One")).unwrap_or_default();
        let second = button_icon_cache_path(&button, Path::new("C:\\Two")).unwrap_or_default();

        assert_ne!(first, second);
    }

    fn test_button_icon_key(button_id: &str) -> ButtonIconKey {
        ButtonIconKey::new(
            String::from("tab"),
            button_id.to_owned(),
            String::from("path"),
            20,
        )
    }

    fn test_button_control(icon_generation: u64, icon_key: Option<ButtonIconKey>) -> ButtonControl {
        ButtonControl {
            hwnd: null_mut(),
            tab_idx: 0,
            button_idx: 0,
            slot_idx: 0,
            icon_generation,
            icon_key,
            image_list: None,
        }
    }

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(label: &str) -> std::io::Result<Self> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "j3launcher-native-test-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path)?;
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
