pub const APP_NAME: &str = "io.github.edgarp9.j3Launcher";
pub const APP_DISPLAY_NAME: &str = "j3Launcher";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_LICENSE: &str = "GPL-3.0-or-later";
pub const APP_LICENSE_FILE_NAME: &str = "LICENSE";
pub const APP_ABOUT_FILE_NAME: &str = "about.txt";
pub const APP_ABOUT_NOTICE_LABEL: &str = "About notice text: about.txt";
pub const APP_ABOUT_TEXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/about.txt"));
pub const APP_COPYRIGHT_NOTICE: &str = "Copyright (C) 2026 j3Launcher contributors";
pub const APP_LICENSE_NOTICE: &str = concat!(
    "j3Launcher is free software under the GNU General Public License version 3 ",
    "or later (GPL-3.0-or-later). You may redistribute and/or modify it under ",
    "version 3 of the License, or at your option any later version. ",
    "There is no warranty, to the extent permitted by law. ",
    "See LICENSE for the full license text."
);
pub const APP_LICENSE_FILE_NOTICE: &str = concat!(
    "Project license: GPL-3.0-or-later. Full license text: LICENSE. ",
    "About notice text: about.txt"
);
pub const APP_SOURCE_CODE_NOTICE: &str = concat!(
    "Corresponding Source for distributed binaries is the same-version source ",
    "package j3launcher-",
    env!("CARGO_PKG_VERSION"),
    "-source.zip distributed next to the binary package or through the same ",
    "release channel."
);
pub const APP_WINDOW_TITLE: &str = concat!("j3Launcher v", env!("CARGO_PKG_VERSION"));
pub const APP_AUTHOR_URL: &str = "https://github.com/edgarp9";
pub const APP_LINUX_APPLICATION_ID: &str = APP_NAME;
pub const APP_CONFIG_FILE_NAME: &str = "j3Launcher.json";
pub const APP_WINDOW_ICON_ICO_FILE_NAME: &str = "icon.ico";
pub const APP_WINDOW_ICON_SVG_FILE_NAME: &str = "icon.svg";
pub const APP_WINDOW_ICON_PNG_FILE_NAME: &str = "icon.png";
pub const THIRD_PARTY_NOTICES_FILE_NAME: &str = "THIRD_PARTY_NOTICES.txt";
pub const THIRD_PARTY_NOTICES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/THIRD_PARTY_NOTICES.txt"
));

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
