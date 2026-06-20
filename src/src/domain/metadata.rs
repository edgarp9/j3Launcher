pub const APP_NAME: &str = "io.github.edgarp9.j3Launcher";
pub const APP_DISPLAY_NAME: &str = "j3Launcher";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_WINDOW_TITLE: &str = concat!("j3Launcher v", env!("CARGO_PKG_VERSION"));
pub const APP_AUTHOR_URL: &str = "https://github.com/edgarp9";
pub const APP_LINUX_APPLICATION_ID: &str = APP_NAME;
pub const APP_CONFIG_FILE_NAME: &str = "j3Launcher.json";
pub const APP_WINDOW_ICON_ICO_FILE_NAME: &str = "icon.ico";
pub const APP_WINDOW_ICON_SVG_FILE_NAME: &str = "icon.svg";
pub const APP_WINDOW_ICON_PNG_FILE_NAME: &str = "icon.png";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppMetadata {
    pub name: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub window_title: &'static str,
    pub author_url: &'static str,
    pub linux_application_id: &'static str,
    pub config_file_name: &'static str,
    pub window_icon_ico_file_name: &'static str,
    pub window_icon_svg_file_name: &'static str,
    pub window_icon_png_file_name: &'static str,
}

impl AppMetadata {
    pub const fn current() -> Self {
        Self {
            name: APP_NAME,
            display_name: APP_DISPLAY_NAME,
            version: APP_VERSION,
            window_title: APP_WINDOW_TITLE,
            author_url: APP_AUTHOR_URL,
            linux_application_id: APP_LINUX_APPLICATION_ID,
            config_file_name: APP_CONFIG_FILE_NAME,
            window_icon_ico_file_name: APP_WINDOW_ICON_ICO_FILE_NAME,
            window_icon_svg_file_name: APP_WINDOW_ICON_SVG_FILE_NAME,
            window_icon_png_file_name: APP_WINDOW_ICON_PNG_FILE_NAME,
        }
    }
}
