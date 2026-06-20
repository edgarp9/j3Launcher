use std::path::PathBuf;

use crate::Result;
use crate::domain::AppMetadata;

pub mod common;

#[cfg(target_os = "linux")]
pub mod gtk4;

#[cfg(windows)]
pub mod win32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSpec {
    pub title: &'static str,
    pub icon_ico_file_name: &'static str,
    pub icon_svg_file_name: &'static str,
    pub icon_png_file_name: &'static str,
    pub config_path: Option<PathBuf>,
}

impl WindowSpec {
    pub fn from_metadata(metadata: AppMetadata) -> Self {
        Self {
            title: metadata.window_title,
            icon_ico_file_name: metadata.window_icon_ico_file_name,
            icon_svg_file_name: metadata.window_icon_svg_file_name,
            icon_png_file_name: metadata.window_icon_png_file_name,
            config_path: None,
        }
    }

    pub fn with_config_path(mut self, config_path: Option<PathBuf>) -> Self {
        self.config_path = config_path;
        self
    }
}

pub fn run_window(spec: WindowSpec) -> Result<()> {
    run_window_impl(spec)
}

#[cfg(windows)]
fn run_window_impl(spec: WindowSpec) -> Result<()> {
    win32::run_window(spec)
}

#[cfg(target_os = "linux")]
fn run_window_impl(spec: WindowSpec) -> Result<()> {
    gtk4::run_window(spec)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn run_window_impl(_spec: WindowSpec) -> Result<()> {
    Err(crate::LauncherError::UnsupportedPlatform {
        platform: std::env::consts::OS,
    })
}
