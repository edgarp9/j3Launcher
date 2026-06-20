pub mod button;
pub mod config;
pub mod metadata;
pub mod scan;
pub mod settings;
pub mod tab;

pub use button::LauncherButton;
pub use config::{DEFAULT_WINDOW_GEOMETRY, LauncherConfig, WindowConfig};
pub use metadata::{
    APP_AUTHOR_URL, APP_CONFIG_FILE_NAME, APP_DISPLAY_NAME, APP_LINUX_APPLICATION_ID, APP_NAME,
    APP_VERSION, APP_WINDOW_ICON_ICO_FILE_NAME, APP_WINDOW_ICON_PNG_FILE_NAME,
    APP_WINDOW_ICON_SVG_FILE_NAME, APP_WINDOW_TITLE, AppMetadata,
};
pub use scan::{
    FOLDER_SCAN_SIGNATURE_VERSION, FolderScanResult, ScanFailure, ScanItem, ScanSignature,
    scan_signatures_match,
};
pub use settings::{LauncherSettings, SettingsDocument};
pub use tab::{
    DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, LauncherTab, MANUAL_DEFAULT_BUTTON_COLS,
    MANUAL_DEFAULT_BUTTON_ROWS, MAX_BUTTON_COLS, MAX_BUTTON_ROWS, MAX_TAB_COUNT, TAB_TYPE_FOLDER,
    TAB_TYPE_MANUAL, TabType,
};
