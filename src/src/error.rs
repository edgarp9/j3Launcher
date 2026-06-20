use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::PathBuf;
use std::str::Utf8Error;

pub type Result<T> = std::result::Result<T, LauncherError>;

#[derive(Debug)]
pub enum LauncherError {
    ConfigRead {
        path: PathBuf,
        source: io::Error,
    },
    ConfigDecode {
        path: PathBuf,
        source: Utf8Error,
    },
    ConfigParse {
        path: PathBuf,
        source: serde_json::Error,
    },
    ConfigSerialize {
        path: PathBuf,
        source: serde_json::Error,
    },
    ConfigWrite {
        path: PathBuf,
        source: io::Error,
    },
    ConfigLock {
        path: PathBuf,
        source: io::Error,
    },
    ConfigSaveConflict {
        path: PathBuf,
    },
    ConfigInvalidIndex {
        message: String,
    },
    FolderScanInvalid {
        path: PathBuf,
        message: String,
    },
    FolderScanRead {
        path: PathBuf,
        source: io::Error,
    },
    Platform {
        message: String,
    },
    UnsupportedPlatform {
        platform: &'static str,
    },
}

impl LauncherError {
    pub fn user_message(&self) -> String {
        match self {
            Self::ConfigRead { path, .. } => {
                format!("설정 파일을 읽을 수 없습니다: {}", path.display())
            }
            Self::ConfigDecode { path, .. } => {
                format!("설정 파일이 UTF-8 형식이 아닙니다: {}", path.display())
            }
            Self::ConfigParse { path, .. } => {
                format!("설정 파일 형식이 올바르지 않습니다: {}", path.display())
            }
            Self::ConfigSerialize { path, .. } => {
                format!(
                    "설정 파일 내용을 JSON으로 변환할 수 없습니다: {}",
                    path.display()
                )
            }
            Self::ConfigWrite { path, .. } => {
                format!("설정 파일을 저장할 수 없습니다: {}", path.display())
            }
            Self::ConfigLock { path, .. } => {
                format!("설정 파일 저장 잠금을 얻을 수 없습니다: {}", path.display())
            }
            Self::ConfigSaveConflict { path } => {
                format!(
                    "설정 파일이 다른 프로세스에서 변경되어 저장을 중단했습니다: {}",
                    path.display()
                )
            }
            Self::ConfigInvalidIndex { message } => message.clone(),
            Self::FolderScanInvalid { path, message } => {
                format!("폴더를 스캔할 수 없습니다: {} ({message})", path.display())
            }
            Self::FolderScanRead { path, .. } => {
                format!("폴더를 읽을 수 없습니다: {}", path.display())
            }
            Self::Platform { message } => message.clone(),
            Self::UnsupportedPlatform { platform } => {
                format!("지원하지 않는 플랫폼입니다: {platform}")
            }
        }
    }
}

impl Display for LauncherError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigRead { path, source } => {
                write!(
                    formatter,
                    "failed to read configuration file '{}': {source}",
                    path.display()
                )
            }
            Self::ConfigDecode { path, source } => {
                write!(
                    formatter,
                    "failed to decode configuration file '{}' as UTF-8: {source}",
                    path.display()
                )
            }
            Self::ConfigParse { path, source } => {
                write!(
                    formatter,
                    "failed to parse configuration file '{}': {source}",
                    path.display()
                )
            }
            Self::ConfigSerialize { path, source } => {
                write!(
                    formatter,
                    "failed to serialize configuration file '{}': {source}",
                    path.display()
                )
            }
            Self::ConfigWrite { path, source } => {
                write!(
                    formatter,
                    "failed to write configuration file '{}': {source}",
                    path.display()
                )
            }
            Self::ConfigLock { path, source } => {
                write!(
                    formatter,
                    "failed to acquire configuration save lock '{}': {source}",
                    path.display()
                )
            }
            Self::ConfigSaveConflict { path } => {
                write!(
                    formatter,
                    "refusing to save stale configuration '{}'",
                    path.display()
                )
            }
            Self::ConfigInvalidIndex { message } => {
                write!(formatter, "invalid configuration index: {message}")
            }
            Self::FolderScanInvalid { path, message } => {
                write!(
                    formatter,
                    "invalid scan folder '{}': {message}",
                    path.display()
                )
            }
            Self::FolderScanRead { path, source } => {
                write!(
                    formatter,
                    "failed to scan folder '{}': {source}",
                    path.display()
                )
            }
            Self::Platform { message } => write!(formatter, "platform error: {message}"),
            Self::UnsupportedPlatform { platform } => {
                write!(formatter, "unsupported platform: {platform}")
            }
        }
    }
}

impl Error for LauncherError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ConfigRead { source, .. } => Some(source),
            Self::ConfigDecode { source, .. } => Some(source),
            Self::ConfigParse { source, .. } => Some(source),
            Self::ConfigSerialize { source, .. } => Some(source),
            Self::ConfigWrite { source, .. } => Some(source),
            Self::ConfigLock { source, .. } => Some(source),
            Self::FolderScanRead { source, .. } => Some(source),
            Self::ConfigSaveConflict { .. }
            | Self::ConfigInvalidIndex { .. }
            | Self::FolderScanInvalid { .. }
            | Self::Platform { .. }
            | Self::UnsupportedPlatform { .. } => None,
        }
    }
}
