use std::ffi::OsString;
use std::path::PathBuf;

use crate::LauncherError;
use crate::Result;
use crate::domain::AppMetadata;
use crate::infra::desktop_entry;
use crate::ui::{WindowSpec, run_window};

pub mod actions;
pub mod button_layout;
pub mod config_service;
pub mod folder_tabs;
pub mod tab_actions;

pub use actions::{
    ActionFailure, ActionFailureKind, AdminLaunchResult, AdminLaunchStatus, ButtonActionInput,
    ButtonActionKind, ButtonActionRequest, ExplorerOpenFeedback, LauncherActionService,
    LauncherPlatform, SystemLauncherPlatform, UserMessage,
};
pub use config_service::ConfigService;

#[derive(Debug, Clone)]
pub struct LauncherApp {
    metadata: AppMetadata,
    config_path: Option<PathBuf>,
}

impl LauncherApp {
    pub fn new(metadata: AppMetadata) -> Self {
        Self {
            metadata,
            config_path: None,
        }
    }

    pub fn with_config_path(mut self, config_path: Option<PathBuf>) -> Self {
        self.config_path = config_path;
        self
    }

    pub fn run(&self) -> Result<()> {
        let window =
            WindowSpec::from_metadata(self.metadata).with_config_path(self.config_path.clone());
        run_window(window)
    }
}

impl Default for LauncherApp {
    fn default() -> Self {
        Self::new(AppMetadata::current())
    }
}

pub fn run() -> Result<()> {
    run_with_args(std::env::args_os().skip(1))
}

pub fn run_with_args<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match cli_command_from_args(args)? {
        CliCommand::Run { config_path } => {
            LauncherApp::default().with_config_path(config_path).run()
        }
        CliCommand::Install => {
            let summary = desktop_entry::install(AppMetadata::current())?;
            if summary.desktop_entry_changed || summary.icon_changed {
                println!(
                    "desktop entry를 설치했습니다: {}",
                    summary.desktop_entry_path.display()
                );
            } else {
                println!(
                    "desktop entry가 이미 최신 상태입니다: {}",
                    summary.desktop_entry_path.display()
                );
            }
            Ok(())
        }
        CliCommand::Uninstall => {
            let summary = desktop_entry::uninstall(AppMetadata::current())?;
            if summary.desktop_entry_removed || summary.icon_removed {
                println!(
                    "desktop entry를 제거했습니다: {}",
                    summary.desktop_entry_path.display()
                );
            } else {
                println!(
                    "desktop entry가 이미 제거된 상태입니다: {}",
                    summary.desktop_entry_path.display()
                );
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliCommand {
    Run { config_path: Option<PathBuf> },
    Install,
    Uninstall,
}

fn cli_command_from_args<I, S>(args: I) -> Result<CliCommand>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let Some(config_path) = args.next() else {
        return Ok(CliCommand::Run { config_path: None });
    };
    if config_path == "--install" {
        ensure_no_extra_args(args)?;
        return Ok(CliCommand::Install);
    }
    if config_path == "--uninstall" {
        ensure_no_extra_args(args)?;
        return Ok(CliCommand::Uninstall);
    }
    if config_path
        .to_str()
        .is_some_and(|arg| arg.starts_with("--"))
    {
        return Err(LauncherError::Platform {
            message: format!("알 수 없는 인자입니다: {}", config_path.to_string_lossy()),
        });
    }
    let config_path = config_path_from_single_arg(config_path, args)?;
    Ok(CliCommand::Run { config_path })
}

fn ensure_no_extra_args(mut args: impl Iterator<Item = OsString>) -> Result<()> {
    if args.next().is_some() {
        return Err(LauncherError::Platform {
            message: String::from("--install/--uninstall 명령에는 추가 인자를 지정할 수 없습니다."),
        });
    }
    Ok(())
}

fn config_path_from_single_arg(
    config_path: OsString,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Option<PathBuf>> {
    if args.next().is_some() {
        return Err(LauncherError::Platform {
            message: String::from("설정 파일 인자는 하나만 지정할 수 있습니다."),
        });
    }
    if config_path.is_empty() {
        return Err(LauncherError::Platform {
            message: String::from("설정 파일 인자가 비어 있습니다."),
        });
    }
    let config_path = PathBuf::from(config_path);
    #[cfg(not(windows))]
    {
        let config_path_text = config_path.to_string_lossy();
        if actions::has_windows_path_syntax(&config_path_text)
            || actions::has_unresolved_windows_env_reference(&config_path_text)
        {
            return Err(LauncherError::Platform {
                message: String::from(
                    "Windows 전용 설정 파일 경로는 Linux에서 직접 사용할 수 없습니다.",
                ),
            });
        }
    }
    Ok(Some(config_path))
}

#[cfg(test)]
fn config_path_from_args<I, S>(args: I) -> Result<Option<PathBuf>>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match cli_command_from_args(args)? {
        CliCommand::Run { config_path } => Ok(config_path),
        CliCommand::Install | CliCommand::Uninstall => Err(LauncherError::Platform {
            message: String::from("--install/--uninstall은 설정 파일 인자가 아닙니다."),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_from_args_uses_default_when_arg_is_absent() {
        let path = config_path_from_args(Vec::<OsString>::new()).expect("args should parse");

        assert_eq!(path, None);
    }

    #[test]
    fn config_path_from_args_accepts_single_config_file() {
        let path = config_path_from_args([OsString::from("111.json")]).expect("args should parse");

        assert_eq!(path, Some(PathBuf::from("111.json")));
    }

    #[test]
    fn config_path_from_args_rejects_multiple_config_files() {
        let error =
            match config_path_from_args([OsString::from("111.json"), OsString::from("222.json")]) {
                Ok(path) => panic!("args unexpectedly parsed as {path:?}"),
                Err(error) => error,
            };

        assert_eq!(
            error.user_message(),
            "설정 파일 인자는 하나만 지정할 수 있습니다."
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn config_path_from_args_rejects_windows_only_paths_on_linux() {
        for value in [
            "C:\\Users\\me\\j3Launcher.json",
            "C:Users\\me\\j3Launcher.json",
            "\\\\server\\share\\j3Launcher.json",
            "%USERPROFILE%\\j3Launcher.json",
        ] {
            let error = match config_path_from_args([OsString::from(value)]) {
                Ok(path) => panic!("args unexpectedly parsed as {path:?}"),
                Err(error) => error,
            };

            assert_eq!(
                error.user_message(),
                "Windows 전용 설정 파일 경로는 Linux에서 직접 사용할 수 없습니다."
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn config_path_from_args_accepts_posix_literal_percent_path_on_linux() {
        let path = config_path_from_args([OsString::from("/tmp/%profile%/j3Launcher.json")])
            .expect("literal percent path should parse");

        assert_eq!(path, Some(PathBuf::from("/tmp/%profile%/j3Launcher.json")));
    }

    #[test]
    fn cli_command_from_args_accepts_install_and_uninstall() {
        assert_eq!(
            cli_command_from_args([OsString::from("--install")]).expect("install should parse"),
            CliCommand::Install
        );
        assert_eq!(
            cli_command_from_args([OsString::from("--uninstall")]).expect("uninstall should parse"),
            CliCommand::Uninstall
        );
    }

    #[test]
    fn cli_command_from_args_rejects_extra_install_args() {
        let error = match cli_command_from_args([
            OsString::from("--install"),
            OsString::from("111.json"),
        ]) {
            Ok(command) => panic!("args unexpectedly parsed as {command:?}"),
            Err(error) => error,
        };

        assert_eq!(
            error.user_message(),
            "--install/--uninstall 명령에는 추가 인자를 지정할 수 없습니다."
        );
    }

    #[test]
    fn cli_command_from_args_rejects_unknown_options() {
        let error = match cli_command_from_args([OsString::from("--missing")]) {
            Ok(command) => panic!("args unexpectedly parsed as {command:?}"),
            Err(error) => error,
        };

        assert_eq!(error.user_message(), "알 수 없는 인자입니다: --missing");
    }
}
