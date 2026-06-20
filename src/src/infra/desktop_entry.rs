use std::path::PathBuf;

use crate::Result;
use crate::domain::AppMetadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntryInstallSummary {
    pub desktop_entry_path: PathBuf,
    pub icon_path: PathBuf,
    pub desktop_entry_changed: bool,
    pub icon_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntryUninstallSummary {
    pub desktop_entry_path: PathBuf,
    pub icon_path: PathBuf,
    pub desktop_entry_removed: bool,
    pub icon_removed: bool,
}

pub fn install(metadata: AppMetadata) -> Result<DesktopEntryInstallSummary> {
    imp::install(metadata)
}

pub fn uninstall(metadata: AppMetadata) -> Result<DesktopEntryUninstallSummary> {
    imp::uninstall(metadata)
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::OsString;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use super::{DesktopEntryInstallSummary, DesktopEntryUninstallSummary};
    use crate::domain::AppMetadata;
    use crate::{LauncherError, Result};

    const HICOLOR_PNG_ICON_SIZE_DIR: &str = "256x256";
    const DESKTOP_ENTRY_MATCH_ALIAS_IDS: &[&str] = &["io.github.edgarp9.j3launcher"];
    const LEGACY_APPLICATION_IDS: &[&str] =
        &["dev.j3launcher.J3Launcher", "j3Launcher", "j3launcher"];

    pub fn install(metadata: AppMetadata) -> Result<DesktopEntryInstallSummary> {
        let executable_path = current_executable_path()?;
        let icon_source = find_icon_source_path(
            metadata.window_icon_svg_file_name,
            metadata.window_icon_png_file_name,
        )?;
        let data_home = xdg_data_home()?;
        let mut summary = install_into(metadata, &executable_path, &icon_source, &data_home)?;
        let legacy_removed = remove_legacy_entries(&data_home, metadata.linux_application_id)?;
        summary.desktop_entry_changed |= legacy_removed.desktop_entry_removed;
        summary.icon_changed |= legacy_removed.icon_removed;
        refresh_desktop_caches(&data_home);
        Ok(summary)
    }

    pub fn uninstall(metadata: AppMetadata) -> Result<DesktopEntryUninstallSummary> {
        let data_home = xdg_data_home()?;
        let paths = DesktopEntryPaths::new(&data_home, metadata.linux_application_id);
        let desktop_entry_removed = remove_file_if_exists(&paths.desktop_entry_path)?;
        let icon_removed = remove_file_if_exists(&paths.icon_svg_path)?
            | remove_file_if_exists(&paths.icon_png_path)?;
        let legacy_removed = remove_legacy_entries(&data_home, metadata.linux_application_id)?;
        let alias_removed = remove_match_alias_entries(&data_home, metadata.linux_application_id)?;
        refresh_desktop_caches(&data_home);
        Ok(DesktopEntryUninstallSummary {
            desktop_entry_path: paths.desktop_entry_path,
            icon_path: paths.icon_svg_path,
            desktop_entry_removed: desktop_entry_removed
                || legacy_removed.desktop_entry_removed
                || alias_removed.desktop_entry_removed,
            icon_removed: icon_removed || legacy_removed.icon_removed || alias_removed.icon_removed,
        })
    }

    fn current_executable_path() -> Result<PathBuf> {
        std::env::current_exe().map_err(|source| LauncherError::Platform {
            message: format!("실행 파일 경로를 확인할 수 없습니다: {source}"),
        })
    }

    fn xdg_data_home() -> Result<PathBuf> {
        if let Some(data_home) = non_empty_env_path("XDG_DATA_HOME")
            && data_home.is_absolute()
        {
            return Ok(data_home);
        }
        let Some(home) = non_empty_env_path("HOME") else {
            return Err(LauncherError::Platform {
                message: String::from(
                    "HOME 경로를 확인할 수 없어 desktop entry를 설치할 수 없습니다.",
                ),
            });
        };
        Ok(home.join(".local/share"))
    }

    fn non_empty_env_path(key: &str) -> Option<PathBuf> {
        std::env::var_os(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    fn find_icon_source_path(svg_file_name: &str, png_file_name: &str) -> Result<IconSource> {
        let mut candidates = Vec::new();
        if let Ok(exe_path) = std::env::current_exe()
            && let Some(exe_dir) = exe_path.parent()
        {
            candidates.push(exe_dir.to_path_buf());
        }
        if let Ok(current_dir) = std::env::current_dir() {
            candidates.push(current_dir);
        }
        for candidate in &candidates {
            let candidate = candidate.join(svg_file_name);
            if candidate.is_file() {
                return Ok(IconSource {
                    path: candidate,
                    format: IconFormat::Svg,
                });
            }
        }
        for candidate in &candidates {
            let candidate = candidate.join(png_file_name);
            if candidate.is_file() {
                return Ok(IconSource {
                    path: candidate,
                    format: IconFormat::Png,
                });
            }
        }
        Err(LauncherError::Platform {
            message: format!(
                "아이콘 파일을 찾을 수 없습니다: {svg_file_name} 또는 {png_file_name}"
            ),
        })
    }

    fn install_into(
        metadata: AppMetadata,
        executable_path: &Path,
        icon_source: &IconSource,
        data_home: &Path,
    ) -> Result<DesktopEntryInstallSummary> {
        let paths = DesktopEntryPaths::new(data_home, metadata.linux_application_id);
        fs::create_dir_all(&paths.applications_dir).map_err(|source| {
            LauncherError::ConfigWrite {
                path: paths.applications_dir.clone(),
                source,
            }
        })?;
        let icon_dir = icon_source.format.icon_dir(&paths);
        fs::create_dir_all(icon_dir).map_err(|source| LauncherError::ConfigWrite {
            path: icon_dir.to_path_buf(),
            source,
        })?;

        let desktop_entry = desktop_entry_content(
            metadata,
            executable_path,
            metadata.linux_application_id,
            metadata.linux_application_id,
            false,
        );
        let desktop_entry_changed =
            write_text_if_changed(&paths.desktop_entry_path, &desktop_entry)?;
        let icon_path = icon_source.format.icon_path(&paths);
        let installed_icon_path = icon_path.to_path_buf();
        let mut icon_changed = copy_file_if_changed(&icon_source.path, icon_path)?;
        if icon_source.format == IconFormat::Svg {
            icon_changed |= remove_file_if_exists(&paths.icon_png_path)?;
        }
        let alias_changed =
            install_match_aliases(metadata, executable_path, icon_source, data_home)?;

        Ok(DesktopEntryInstallSummary {
            desktop_entry_path: paths.desktop_entry_path,
            icon_path: installed_icon_path,
            desktop_entry_changed: desktop_entry_changed || alias_changed.desktop_entry_changed,
            icon_changed: icon_changed || alias_changed.icon_changed,
        })
    }

    fn install_match_aliases(
        metadata: AppMetadata,
        executable_path: &Path,
        icon_source: &IconSource,
        data_home: &Path,
    ) -> Result<DesktopEntryInstallSummary> {
        let mut desktop_entry_changed = false;
        let mut icon_changed = false;
        let current_paths = DesktopEntryPaths::new(data_home, metadata.linux_application_id);
        for alias_id in DESKTOP_ENTRY_MATCH_ALIAS_IDS
            .iter()
            .copied()
            .filter(|alias_id| *alias_id != metadata.linux_application_id)
        {
            let paths = DesktopEntryPaths::new(data_home, alias_id);
            let desktop_entry =
                desktop_entry_content(metadata, executable_path, alias_id, alias_id, true);
            desktop_entry_changed |=
                write_text_if_changed(&paths.desktop_entry_path, &desktop_entry)?;

            let alias_icon_path = icon_source.format.icon_path(&paths);
            icon_changed |= copy_file_if_changed(&icon_source.path, alias_icon_path)?;
            if icon_source.format == IconFormat::Svg {
                icon_changed |= remove_file_if_exists(&paths.icon_png_path)?;
            } else {
                icon_changed |= remove_file_if_exists(&paths.icon_svg_path)?;
            }
        }

        Ok(DesktopEntryInstallSummary {
            desktop_entry_path: current_paths.desktop_entry_path,
            icon_path: current_paths.icon_svg_path,
            desktop_entry_changed,
            icon_changed,
        })
    }

    fn desktop_entry_content(
        metadata: AppMetadata,
        executable_path: &Path,
        entry_id: &str,
        icon_id: &str,
        no_display: bool,
    ) -> String {
        let no_display_entry = if no_display { "NoDisplay=true\n" } else { "" };
        format!(
            "# Managed by {} --install\n\
             [Desktop Entry]\n\
             Type=Application\n\
             Name={}\n\
             Comment={}\n\
             Exec={}\n\
             Icon={icon_id}\n\
             Terminal=false\n\
             Categories=Utility;\n\
             StartupNotify=true\n\
             StartupWMClass={entry_id}\n\
             {no_display_entry}",
            metadata.name,
            metadata.name,
            metadata.name,
            desktop_exec_path(executable_path),
        )
    }

    fn desktop_exec_path(path: &Path) -> String {
        let value = path.to_string_lossy();
        if value
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\\' | '$' | '`'))
        {
            return value.into_owned();
        }

        let mut escaped = String::from("\"");
        for ch in value.chars() {
            match ch {
                '"' | '\\' | '$' | '`' => {
                    escaped.push('\\');
                    escaped.push(ch);
                }
                _ => escaped.push(ch),
            }
        }
        escaped.push('"');
        escaped
    }

    fn write_text_if_changed(path: &Path, content: &str) -> Result<bool> {
        if fs::read_to_string(path).is_ok_and(|existing| existing == content) {
            return Ok(false);
        }
        fs::write(path, content).map_err(|source| LauncherError::ConfigWrite {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(true)
    }

    fn copy_file_if_changed(source_path: &Path, destination_path: &Path) -> Result<bool> {
        let source = fs::read(source_path).map_err(|source| LauncherError::ConfigRead {
            path: source_path.to_path_buf(),
            source,
        })?;
        if fs::read(destination_path).is_ok_and(|existing| existing == source) {
            return Ok(false);
        }
        fs::write(destination_path, source).map_err(|source| LauncherError::ConfigWrite {
            path: destination_path.to_path_buf(),
            source,
        })?;
        Ok(true)
    }

    fn remove_file_if_exists(path: &Path) -> Result<bool> {
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(LauncherError::ConfigWrite {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    fn remove_legacy_entries(
        data_home: &Path,
        current_application_id: &str,
    ) -> Result<DesktopEntryUninstallSummary> {
        let mut desktop_entry_removed = false;
        let mut icon_removed = false;
        remove_entries_for_ids(
            data_home,
            current_application_id,
            LEGACY_APPLICATION_IDS.iter().copied(),
            &mut desktop_entry_removed,
            &mut icon_removed,
        )?;
        let current_paths = DesktopEntryPaths::new(data_home, current_application_id);
        Ok(DesktopEntryUninstallSummary {
            desktop_entry_path: current_paths.desktop_entry_path,
            icon_path: current_paths.icon_svg_path,
            desktop_entry_removed,
            icon_removed,
        })
    }

    fn remove_match_alias_entries(
        data_home: &Path,
        current_application_id: &str,
    ) -> Result<DesktopEntryUninstallSummary> {
        let mut desktop_entry_removed = false;
        let mut icon_removed = false;
        remove_entries_for_ids(
            data_home,
            current_application_id,
            DESKTOP_ENTRY_MATCH_ALIAS_IDS.iter().copied(),
            &mut desktop_entry_removed,
            &mut icon_removed,
        )?;
        let current_paths = DesktopEntryPaths::new(data_home, current_application_id);
        Ok(DesktopEntryUninstallSummary {
            desktop_entry_path: current_paths.desktop_entry_path,
            icon_path: current_paths.icon_svg_path,
            desktop_entry_removed,
            icon_removed,
        })
    }

    fn remove_entries_for_ids<'a>(
        data_home: &Path,
        current_application_id: &str,
        ids: impl IntoIterator<Item = &'a str>,
        desktop_entry_removed: &mut bool,
        icon_removed: &mut bool,
    ) -> Result<()> {
        for legacy_id in ids
            .into_iter()
            .filter(|legacy_id| *legacy_id != current_application_id)
        {
            let paths = DesktopEntryPaths::new(data_home, legacy_id);
            *desktop_entry_removed |= remove_file_if_exists(&paths.desktop_entry_path)?;
            *icon_removed |= remove_file_if_exists(&paths.icon_svg_path)?;
            *icon_removed |= remove_file_if_exists(&paths.icon_png_path)?;
        }
        Ok(())
    }

    fn refresh_desktop_caches(data_home: &Path) {
        let applications_dir = data_home.join("applications");
        let hicolor_dir = data_home.join("icons/hicolor");
        run_cache_command(
            "update-desktop-database",
            &[applications_dir.into_os_string()],
        );
        run_cache_command(
            "gtk-update-icon-cache",
            &[
                OsString::from("-f"),
                OsString::from("-t"),
                hicolor_dir.into_os_string(),
            ],
        );
        if !run_cache_command("kbuildsycoca6", &[OsString::from("--noincremental")]) {
            run_cache_command("kbuildsycoca5", &[OsString::from("--noincremental")]);
        }
    }

    fn run_cache_command(program: &str, args: &[OsString]) -> bool {
        Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[derive(Debug, Clone)]
    struct DesktopEntryPaths {
        applications_dir: PathBuf,
        scalable_icon_dir: PathBuf,
        png_icon_dir: PathBuf,
        desktop_entry_path: PathBuf,
        icon_svg_path: PathBuf,
        icon_png_path: PathBuf,
    }

    impl DesktopEntryPaths {
        fn new(data_home: &Path, application_id: &str) -> Self {
            let applications_dir = data_home.join("applications");
            let scalable_icon_dir = data_home.join("icons/hicolor/scalable/apps");
            let png_icon_dir = data_home
                .join("icons/hicolor")
                .join(HICOLOR_PNG_ICON_SIZE_DIR)
                .join("apps");
            Self {
                desktop_entry_path: applications_dir.join(format!("{application_id}.desktop")),
                icon_svg_path: scalable_icon_dir.join(format!("{application_id}.svg")),
                icon_png_path: png_icon_dir.join(format!("{application_id}.png")),
                applications_dir,
                scalable_icon_dir,
                png_icon_dir,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct IconSource {
        path: PathBuf,
        format: IconFormat,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum IconFormat {
        Svg,
        Png,
    }

    impl IconFormat {
        fn icon_dir(self, paths: &DesktopEntryPaths) -> &Path {
            match self {
                Self::Svg => &paths.scalable_icon_dir,
                Self::Png => &paths.png_icon_dir,
            }
        }

        fn icon_path(self, paths: &DesktopEntryPaths) -> &Path {
            match self {
                Self::Svg => &paths.icon_svg_path,
                Self::Png => &paths.icon_png_path,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::error::Error;
        use std::process;
        use std::sync::atomic::{AtomicU64, Ordering};

        use super::*;

        #[test]
        fn install_into_is_idempotent() -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("install-idempotent")?;
            let executable = temp.path().join("j3 launcher");
            let icon = temp.path().join("icon.svg");
            fs::write(&executable, b"binary")?;
            fs::write(&icon, b"svg")?;
            let icon_source = IconSource {
                path: icon,
                format: IconFormat::Svg,
            };

            let first = install_into(
                AppMetadata::current(),
                &executable,
                &icon_source,
                temp.path(),
            )?;
            let second = install_into(
                AppMetadata::current(),
                &executable,
                &icon_source,
                temp.path(),
            )?;

            assert!(first.desktop_entry_changed);
            assert!(first.icon_changed);
            assert!(!second.desktop_entry_changed);
            assert!(!second.icon_changed);
            assert_eq!(
                fs::read_to_string(&first.desktop_entry_path)?,
                desktop_entry_content(
                    AppMetadata::current(),
                    &executable,
                    AppMetadata::current().linux_application_id,
                    AppMetadata::current().linux_application_id,
                    false,
                )
            );
            assert_eq!(
                first.icon_path,
                DesktopEntryPaths::new(temp.path(), AppMetadata::current().linux_application_id)
                    .icon_svg_path
            );
            assert_eq!(fs::read(&first.icon_path)?, b"svg");
            Ok(())
        }

        #[test]
        fn install_into_creates_lowercase_match_alias_for_kde_taskbar()
        -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("install-alias")?;
            let executable = temp.path().join("j3launcher");
            let icon = temp.path().join("icon.svg");
            fs::write(&executable, b"binary")?;
            fs::write(&icon, b"svg")?;
            let icon_source = IconSource {
                path: icon,
                format: IconFormat::Svg,
            };

            install_into(
                AppMetadata::current(),
                &executable,
                &icon_source,
                temp.path(),
            )?;

            let alias_paths = DesktopEntryPaths::new(temp.path(), DESKTOP_ENTRY_MATCH_ALIAS_IDS[0]);
            let alias_entry = fs::read_to_string(&alias_paths.desktop_entry_path)?;
            assert!(alias_entry.contains("NoDisplay=true\n"));
            assert!(alias_entry.contains("Icon=io.github.edgarp9.j3launcher\n"));
            assert!(alias_entry.contains("StartupWMClass=io.github.edgarp9.j3launcher\n"));
            assert_eq!(fs::read(&alias_paths.icon_svg_path)?, b"svg");
            Ok(())
        }

        #[test]
        fn svg_install_updates_stale_entry_and_removes_stale_png()
        -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("install-update")?;
            let paths =
                DesktopEntryPaths::new(temp.path(), AppMetadata::current().linux_application_id);
            fs::create_dir_all(&paths.applications_dir)?;
            fs::create_dir_all(&paths.scalable_icon_dir)?;
            fs::create_dir_all(&paths.png_icon_dir)?;
            fs::write(&paths.desktop_entry_path, "old")?;
            fs::write(&paths.icon_svg_path, b"old")?;
            fs::write(&paths.icon_png_path, b"old-png")?;
            let executable = temp.path().join("j3launcher");
            let icon = temp.path().join("icon.svg");
            fs::write(&executable, b"binary")?;
            fs::write(&icon, b"new-svg")?;
            let icon_source = IconSource {
                path: icon,
                format: IconFormat::Svg,
            };

            let summary = install_into(
                AppMetadata::current(),
                &executable,
                &icon_source,
                temp.path(),
            )?;

            assert!(summary.desktop_entry_changed);
            assert!(summary.icon_changed);
            assert!(fs::read_to_string(&summary.desktop_entry_path)?.contains("Exec="));
            assert_eq!(fs::read(&summary.icon_path)?, b"new-svg");
            assert!(!paths.icon_png_path.exists());
            Ok(())
        }

        #[test]
        fn png_install_is_kept_as_fallback_when_svg_is_unavailable()
        -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("install-png")?;
            let executable = temp.path().join("j3launcher");
            let icon = temp.path().join("icon.png");
            fs::write(&executable, b"binary")?;
            fs::write(&icon, b"png")?;
            let icon_source = IconSource {
                path: icon,
                format: IconFormat::Png,
            };

            let summary = install_into(
                AppMetadata::current(),
                &executable,
                &icon_source,
                temp.path(),
            )?;

            assert!(summary.icon_changed);
            assert_eq!(
                summary.icon_path,
                DesktopEntryPaths::new(temp.path(), AppMetadata::current().linux_application_id)
                    .icon_png_path
            );
            assert_eq!(fs::read(&summary.icon_path)?, b"png");
            Ok(())
        }

        #[test]
        fn uninstall_removes_files_and_treats_missing_as_success()
        -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("uninstall")?;
            let paths =
                DesktopEntryPaths::new(temp.path(), AppMetadata::current().linux_application_id);
            fs::create_dir_all(&paths.applications_dir)?;
            fs::create_dir_all(&paths.scalable_icon_dir)?;
            fs::create_dir_all(&paths.png_icon_dir)?;
            fs::write(&paths.desktop_entry_path, "desktop")?;
            fs::write(&paths.icon_svg_path, b"svg")?;
            fs::write(&paths.icon_png_path, b"png")?;

            assert!(remove_file_if_exists(&paths.desktop_entry_path)?);
            assert!(remove_file_if_exists(&paths.icon_svg_path)?);
            assert!(remove_file_if_exists(&paths.icon_png_path)?);
            assert!(!remove_file_if_exists(&paths.desktop_entry_path)?);
            assert!(!remove_file_if_exists(&paths.icon_svg_path)?);
            assert!(!remove_file_if_exists(&paths.icon_png_path)?);
            Ok(())
        }

        #[test]
        fn legacy_entries_are_removed_during_install_cleanup()
        -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("legacy-cleanup")?;
            for legacy_id in LEGACY_APPLICATION_IDS {
                let legacy_paths = DesktopEntryPaths::new(temp.path(), legacy_id);
                fs::create_dir_all(&legacy_paths.applications_dir)?;
                fs::create_dir_all(&legacy_paths.scalable_icon_dir)?;
                fs::create_dir_all(&legacy_paths.png_icon_dir)?;
                fs::write(&legacy_paths.desktop_entry_path, "legacy")?;
                fs::write(&legacy_paths.icon_svg_path, b"legacy-svg")?;
                fs::write(&legacy_paths.icon_png_path, b"legacy-png")?;
            }

            let removed =
                remove_legacy_entries(temp.path(), AppMetadata::current().linux_application_id)?;

            assert!(removed.desktop_entry_removed);
            assert!(removed.icon_removed);
            for legacy_id in LEGACY_APPLICATION_IDS {
                let legacy_paths = DesktopEntryPaths::new(temp.path(), legacy_id);
                assert!(!legacy_paths.desktop_entry_path.exists());
                assert!(!legacy_paths.icon_svg_path.exists());
                assert!(!legacy_paths.icon_png_path.exists());
            }
            Ok(())
        }

        #[test]
        fn match_alias_entries_are_removed_during_uninstall_cleanup()
        -> std::result::Result<(), Box<dyn Error>> {
            let temp = TestDir::new("alias-cleanup")?;
            for alias_id in DESKTOP_ENTRY_MATCH_ALIAS_IDS {
                let alias_paths = DesktopEntryPaths::new(temp.path(), alias_id);
                fs::create_dir_all(&alias_paths.applications_dir)?;
                fs::create_dir_all(&alias_paths.scalable_icon_dir)?;
                fs::create_dir_all(&alias_paths.png_icon_dir)?;
                fs::write(&alias_paths.desktop_entry_path, "alias")?;
                fs::write(&alias_paths.icon_svg_path, b"alias-svg")?;
                fs::write(&alias_paths.icon_png_path, b"alias-png")?;
            }

            let removed = remove_match_alias_entries(
                temp.path(),
                AppMetadata::current().linux_application_id,
            )?;

            assert!(removed.desktop_entry_removed);
            assert!(removed.icon_removed);
            for alias_id in DESKTOP_ENTRY_MATCH_ALIAS_IDS {
                let alias_paths = DesktopEntryPaths::new(temp.path(), alias_id);
                assert!(!alias_paths.desktop_entry_path.exists());
                assert!(!alias_paths.icon_svg_path.exists());
                assert!(!alias_paths.icon_png_path.exists());
            }
            Ok(())
        }

        #[test]
        fn desktop_exec_path_quotes_spaces_and_special_chars() {
            let path = Path::new("/tmp/j3 launcher/$build`dir`/j3\"launcher");

            assert_eq!(
                desktop_exec_path(path),
                "\"/tmp/j3 launcher/\\$build\\`dir\\`/j3\\\"launcher\""
            );
        }

        static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(1);

        struct TestDir {
            path: PathBuf,
        }

        impl TestDir {
            fn new(label: &str) -> io::Result<Self> {
                let base = std::env::temp_dir();
                for _ in 0..100 {
                    let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
                    let candidate = base.join(format!(
                        "j3launcher-desktop-entry-{}-{sequence}-{label}",
                        process::id()
                    ));
                    match fs::create_dir(&candidate) {
                        Ok(()) => return Ok(Self { path: candidate }),
                        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
                        Err(source) => return Err(source),
                    }
                }
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "could not reserve test directory",
                ))
            }

            fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TestDir {
            fn drop(&mut self) {
                let temp_dir = std::env::temp_dir();
                if self.path.starts_with(&temp_dir) {
                    let _ = fs::remove_dir_all(&self.path);
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{DesktopEntryInstallSummary, DesktopEntryUninstallSummary};
    use crate::domain::AppMetadata;
    use crate::{LauncherError, Result};

    pub fn install(_metadata: AppMetadata) -> Result<DesktopEntryInstallSummary> {
        Err(LauncherError::UnsupportedPlatform {
            platform: std::env::consts::OS,
        })
    }

    pub fn uninstall(_metadata: AppMetadata) -> Result<DesktopEntryUninstallSummary> {
        Err(LauncherError::UnsupportedPlatform {
            platform: std::env::consts::OS,
        })
    }
}
