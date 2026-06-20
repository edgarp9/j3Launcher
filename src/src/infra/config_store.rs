use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(windows))]
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::ser::PrettyFormatter;

use crate::domain::button::{make_item_id, normalize_path_text};
use crate::domain::scan::{FOLDER_SCAN_SIGNATURE_VERSION, ScanSignature};
use crate::domain::tab::{
    DEFAULT_BUTTON_COLS, DEFAULT_BUTTON_ROWS, MANUAL_DEFAULT_BUTTON_COLS,
    MANUAL_DEFAULT_BUTTON_ROWS, MAX_BUTTON_COLS, MAX_BUTTON_ROWS, MAX_TAB_COUNT, build_tab_id,
    normalize_hidden_item_ids, normalize_scan_item_order, normalize_slot_positions, path_basename,
};
use crate::domain::{
    APP_CONFIG_FILE_NAME, DEFAULT_WINDOW_GEOMETRY, LauncherButton, LauncherConfig, LauncherTab,
    TabType,
};
use crate::{LauncherError, Result};

pub const WINDOWS_CONFIG_SEED_FILE_NAME: &str = "j3Launcher_win.json";
pub const CONFIG_SAVE_CONFLICT_MESSAGE: &str = "Configuration file changed on disk after this window loaded it. Your changes were not saved to avoid overwriting changes from another j3Launcher instance. Restart j3Launcher or reload the configuration before saving again.";
pub const CONFIG_SAVE_LOCK_MESSAGE: &str = "Configuration file is currently locked by another j3Launcher instance or cannot be locked. Your changes were not saved to avoid overwriting another update. Close the other instance or check file permissions, then try again.";

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001B3;
const CONFIG_READ_STABILITY_ATTEMPTS: usize = 3;
const MAX_CONFIG_PAYLOAD_BYTES: u64 = 8 * 1024 * 1024;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonInfo {
    pub name: String,
    pub path: String,
    pub params: String,
    pub admin: bool,
    pub action: u8,
    pub auto_enter: bool,
}

impl ButtonInfo {
    fn normalized(mut self) -> Self {
        self.action = if self.action == 1 { 1 } else { 0 };
        self
    }
}

impl From<&LauncherButton> for ButtonInfo {
    fn from(button: &LauncherButton) -> Self {
        Self {
            name: button.name.clone(),
            path: button.path.clone(),
            params: button.params.clone(),
            admin: button.admin,
            action: button.action,
            auto_enter: button.auto_enter,
        }
        .normalized()
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    base_dir: PathBuf,
    path: PathBuf,
    lock_path: PathBuf,
    config: Arc<LauncherConfig>,
    signature: ConfigFileSignature,
}

#[derive(Debug, Clone)]
pub struct ConfigSaveReceipt {
    path: PathBuf,
    signature: ConfigFileSignature,
}

impl ConfigStore {
    pub fn open(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = absolute_base_dir(base_dir.as_ref())?;
        let path = base_dir.join(APP_CONFIG_FILE_NAME);
        Self::open_resolved_path(base_dir, path)
    }

    pub fn open_path(config_path: impl AsRef<Path>) -> Result<Self> {
        let path = absolute_config_path(config_path.as_ref())?;
        Self::open_absolute_path(path)
    }

    fn open_absolute_path(path: PathBuf) -> Result<Self> {
        let base_dir =
            path.parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| LauncherError::Platform {
                    message: format!("설정 파일 폴더를 확인할 수 없습니다: {}", path.display()),
                })?;
        Self::open_resolved_path(base_dir, path)
    }

    fn open_resolved_path(base_dir: PathBuf, path: PathBuf) -> Result<Self> {
        let lock_path = lock_path_for(&path);
        let exists = path
            .try_exists()
            .map_err(|source| LauncherError::ConfigRead {
                path: path.clone(),
                source,
            })?;

        if !exists {
            fs::create_dir_all(&base_dir).map_err(|source| LauncherError::ConfigWrite {
                path: path.clone(),
                source,
            })?;
            let config = load_seed_config(&base_dir)?.unwrap_or_default();
            let mut store = Self {
                base_dir,
                path,
                lock_path,
                config: Arc::new(config),
                signature: ConfigFileSignature::missing(),
            };
            store.write_current_config_snapshot()?;
            return Ok(store);
        }

        let (payload, metadata) = read_config_payload(&path)?;
        let signature = config_signature_from_bytes(&payload, metadata.as_ref());
        match parse_config_payload(&path, &payload) {
            Ok(config) => Ok(Self {
                base_dir,
                path,
                lock_path,
                config: Arc::new(config),
                signature,
            }),
            Err(LauncherError::ConfigParse { .. } | LauncherError::ConfigDecode { .. }) => {
                write_corrupted_config_backup(&path, &payload)?;
                let config = LauncherConfig::default();
                let mut store = Self {
                    base_dir,
                    path,
                    lock_path,
                    config: Arc::new(config),
                    signature,
                };
                store.write_current_config_snapshot()?;
                Ok(store)
            }
            Err(error) => Err(error),
        }
    }

    pub fn open_from_current_dir() -> Result<Self> {
        let base_dir = std::env::current_dir().map_err(|source| LauncherError::ConfigRead {
            path: PathBuf::from("."),
            source,
        })?;
        Self::open(base_dir)
    }

    pub fn open_from_executable_or_current_dir() -> Result<Self> {
        let base_dir = executable_or_current_dir()?;
        Self::open(base_dir)
    }

    pub fn open_path_from_executable_or_current_dir(config_path: impl AsRef<Path>) -> Result<Self> {
        let base_dir = executable_or_current_dir()?;
        let path = resolve_config_path_from_base(&base_dir, config_path.as_ref())?;
        Self::open_absolute_path(path)
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn config_path(&self) -> &Path {
        &self.path
    }

    pub fn config(&self) -> &LauncherConfig {
        self.config.as_ref()
    }

    pub fn reload(&mut self) -> Result<()> {
        let (config, signature) = read_config_file(&self.path)?;
        self.config = Arc::new(config);
        self.signature = signature;
        Ok(())
    }

    pub fn save(&mut self) -> Result<()> {
        self.write_current_config_snapshot().map(|_| ())
    }

    pub fn save_config_snapshot(&mut self, snapshot: LauncherConfig) -> Result<()> {
        self.persist_config(snapshot)
    }

    pub fn write_config_snapshot_ref(
        &mut self,
        snapshot: &LauncherConfig,
    ) -> Result<ConfigSaveReceipt> {
        self.write_config_snapshot(snapshot)
    }

    pub fn replace_in_memory_config(&mut self, config: LauncherConfig) {
        self.config = Arc::new(config);
    }

    pub fn replace_in_memory_config_snapshot(&mut self, config: Arc<LauncherConfig>) {
        self.config = config;
    }

    pub fn adopt_saved_signature_from(&mut self, saved_store: &ConfigStore) {
        if self.path == saved_store.path {
            self.signature = saved_store.signature.clone();
        }
    }

    pub fn adopt_saved_signature(&mut self, receipt: &ConfigSaveReceipt) {
        if self.path == receipt.path {
            self.signature = receipt.signature.clone();
        }
    }

    pub fn get_window_geometry(&self) -> String {
        self.get_window_geometry_for_dpi(None)
    }

    pub fn get_window_geometry_for_dpi(&self, current_dpi_scale: Option<f64>) -> String {
        let ratio = geometry_scale_ratio(self.config.window.dpi_scale, current_dpi_scale);
        scale_window_geometry(&self.config.window.geometry, ratio)
    }

    pub fn dark_theme(&self) -> bool {
        self.config.window.dark_theme
    }

    pub fn set_dark_theme(&mut self, enabled: bool) -> Result<()> {
        let previous_window = self.config.window.clone();
        {
            let config = Arc::make_mut(&mut self.config);
            config.window.dark_theme = enabled;
            normalize_window_config(config);
        }
        self.write_current_config_snapshot()
            .map(|_| ())
            .inspect_err(|_| {
                Arc::make_mut(&mut self.config).window = previous_window;
            })
    }

    pub fn prepare_dark_theme_config(
        &self,
        config: &LauncherConfig,
        enabled: bool,
    ) -> Result<LauncherConfig> {
        let mut target = config.clone();
        target.window.dark_theme = enabled;
        if self.is_current_config(config) {
            normalize_window_config(&mut target);
            Ok(target)
        } else {
            normalize_config(target, &self.path)
        }
    }

    pub fn save_window_geometry(&mut self, geo_str: impl Into<String>) -> Result<()> {
        self.save_window_geometry_with_dpi(geo_str, None)
    }

    pub fn save_window_geometry_with_dpi(
        &mut self,
        geo_str: impl Into<String>,
        dpi_scale: Option<f64>,
    ) -> Result<()> {
        let previous_window = self.config.window.clone();
        {
            let config = Arc::make_mut(&mut self.config);
            config.window.geometry = geo_str.into();
            if let Some(scale) = normalize_dpi_scale(dpi_scale) {
                config.window.dpi_scale = Some(scale);
            }
            normalize_window_config(config);
        }
        self.write_current_config_snapshot()
            .map(|_| ())
            .inspect_err(|_| {
                Arc::make_mut(&mut self.config).window = previous_window;
            })
    }

    pub fn prepare_window_geometry_config(
        &self,
        config: &LauncherConfig,
        geo_str: impl Into<String>,
        dpi_scale: Option<f64>,
    ) -> Result<LauncherConfig> {
        let mut target = config.clone();
        target.window.geometry = geo_str.into();
        if let Some(scale) = normalize_dpi_scale(dpi_scale) {
            target.window.dpi_scale = Some(scale);
        }
        if self.is_current_config(config) {
            normalize_window_config(&mut target);
            Ok(target)
        } else {
            normalize_config(target, &self.path)
        }
    }

    pub fn folder_tabs(&self) -> &[LauncherTab] {
        &self.config.folder_tabs
    }

    pub fn get_folder_tabs(&self) -> Vec<LauncherTab> {
        self.folder_tabs().to_vec()
    }

    pub fn set_folder_tabs(&mut self, folder_tabs: Vec<LauncherTab>) -> Result<()> {
        let target = self.prepare_folder_tabs_config(self.config.as_ref(), folder_tabs)?;
        self.persist_config(target)
    }

    pub fn prepare_folder_tabs_config(
        &self,
        config: &LauncherConfig,
        folder_tabs: Vec<LauncherTab>,
    ) -> Result<LauncherConfig> {
        let target = LauncherConfig {
            window: config.window.clone(),
            folder_tabs,
            extra: config.extra.clone(),
        };
        normalize_config(target, &self.path)
    }

    pub fn get_button_info(&self, tab_idx: usize, btn_idx: usize) -> ButtonInfo {
        let Some(tab) = self.config.folder_tabs.get(tab_idx) else {
            return ButtonInfo::default();
        };
        let Some(button) = tab.buttons.get(btn_idx) else {
            return ButtonInfo::default();
        };
        ButtonInfo::from(button)
    }

    pub fn set_button_info(
        &mut self,
        tab_idx: usize,
        btn_idx: usize,
        info: ButtonInfo,
    ) -> Result<()> {
        validate_button_info_update(self.config.as_ref(), tab_idx, btn_idx)?;
        let previous_tab = self.config.folder_tabs[tab_idx].clone();
        {
            let config = Arc::make_mut(&mut self.config);
            apply_button_info_update(&mut config.folder_tabs, tab_idx, btn_idx, info)?;
            normalize_tab_after_button_update(&mut config.folder_tabs, tab_idx);
        }
        self.write_current_config_snapshot()
            .map(|_| ())
            .inspect_err(|_| {
                if let Some(tab) = Arc::make_mut(&mut self.config).folder_tabs.get_mut(tab_idx) {
                    *tab = previous_tab;
                }
            })
    }

    pub fn prepare_button_info_config(
        &self,
        config: &LauncherConfig,
        tab_idx: usize,
        btn_idx: usize,
        info: ButtonInfo,
    ) -> Result<LauncherConfig> {
        validate_button_info_update(config, tab_idx, btn_idx)?;
        let mut target = config.clone();
        apply_button_info_update(&mut target.folder_tabs, tab_idx, btn_idx, info)?;
        if self.is_current_config(config) {
            normalize_window_config(&mut target);
            normalize_tab_after_button_update(&mut target.folder_tabs, tab_idx);
            Ok(target)
        } else {
            normalize_config(target, &self.path)
        }
    }

    fn is_current_config(&self, config: &LauncherConfig) -> bool {
        std::ptr::eq(config, self.config.as_ref())
    }

    fn persist_config(&mut self, target: LauncherConfig) -> Result<()> {
        self.write_config_snapshot(&target)?;
        self.config = Arc::new(target);
        Ok(())
    }

    fn write_config_snapshot(&mut self, snapshot: &LauncherConfig) -> Result<ConfigSaveReceipt> {
        write_config_snapshot_parts(
            &self.base_dir,
            &self.path,
            &self.lock_path,
            &mut self.signature,
            snapshot,
        )
    }

    fn write_current_config_snapshot(&mut self) -> Result<ConfigSaveReceipt> {
        write_config_snapshot_parts(
            &self.base_dir,
            &self.path,
            &self.lock_path,
            &mut self.signature,
            self.config.as_ref(),
        )
    }
}

fn validate_button_info_update(
    config: &LauncherConfig,
    tab_idx: usize,
    btn_idx: usize,
) -> Result<()> {
    let tab = config
        .folder_tabs
        .get(tab_idx)
        .ok_or_else(|| invalid_index("tab_idx out of range"))?;
    if tab.tab_type != TabType::Manual {
        return if btn_idx < tab.buttons.len() {
            Ok(())
        } else {
            Err(invalid_index("btn_idx out of range"))
        };
    }

    let rows = normalize_button_dimension(tab.rows, MANUAL_DEFAULT_BUTTON_ROWS, MAX_BUTTON_ROWS);
    let cols = normalize_button_dimension(tab.cols, MANUAL_DEFAULT_BUTTON_COLS, MAX_BUTTON_COLS);
    let required_slots = usize::from(rows) * usize::from(cols);
    if btn_idx < required_slots {
        Ok(())
    } else {
        Err(invalid_index("btn_idx out of range"))
    }
}

fn apply_button_info_update(
    folder_tabs: &mut [LauncherTab],
    tab_idx: usize,
    btn_idx: usize,
    info: ButtonInfo,
) -> Result<()> {
    let tab = folder_tabs
        .get_mut(tab_idx)
        .ok_or_else(|| invalid_index("tab_idx out of range"))?;
    let is_manual = tab.tab_type == TabType::Manual;
    if btn_idx >= tab.buttons.len() {
        if !is_manual {
            return Err(invalid_index("btn_idx out of range"));
        }
        while tab.buttons.len() <= btn_idx {
            tab.buttons.push(LauncherButton::manual_default());
        }
    }

    let button = tab
        .buttons
        .get_mut(btn_idx)
        .ok_or_else(|| invalid_index("btn_idx out of range"))?;
    let info = info.normalized();
    button.name = info.name;
    button.path = info.path;
    button.params = info.params;
    button.admin = info.admin;
    button.action = info.action;
    button.auto_enter = info.auto_enter;
    if is_manual {
        sync_manual_button_source_metadata(button);
    }
    Ok(())
}

fn normalize_tab_after_button_update(folder_tabs: &mut [LauncherTab], tab_idx: usize) {
    let mut seen_tab_ids = HashSet::with_capacity(tab_idx.saturating_add(1));
    for tab in folder_tabs.iter().take(tab_idx) {
        seen_tab_ids.insert(tab.id.clone());
    }
    if let Some(tab) = folder_tabs.get_mut(tab_idx) {
        normalize_tab(tab, tab_idx, &mut seen_tab_ids);
    }
}

fn write_config_snapshot_parts(
    base_dir: &Path,
    path: &Path,
    lock_path: &Path,
    signature: &mut ConfigFileSignature,
    snapshot: &LauncherConfig,
) -> Result<ConfigSaveReceipt> {
    fs::create_dir_all(base_dir).map_err(|source| LauncherError::ConfigWrite {
        path: path.to_path_buf(),
        source,
    })?;

    let _lock = ConfigSaveLock::acquire(lock_path)?;
    let (signatures_match, _) = config_file_signature_matches(path, signature)?;
    if !signatures_match {
        return Err(LauncherError::ConfigSaveConflict {
            path: path.to_path_buf(),
        });
    }

    let payload = serialize_config(snapshot, path)?;
    let temp_path = write_temp_config_file(base_dir, path, &payload)?;
    if let Err(source) = crate::platform::replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(LauncherError::ConfigWrite {
            path: path.to_path_buf(),
            source,
        });
    }

    let metadata = fs::metadata(path).ok();
    *signature = config_signature_from_bytes(&payload, metadata.as_ref());
    Ok(ConfigSaveReceipt {
        path: path.to_path_buf(),
        signature: signature.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigFileSignature {
    exists: bool,
    size: u64,
    modified: Option<SystemTime>,
    hash: Option<u64>,
}

impl ConfigFileSignature {
    fn missing() -> Self {
        Self {
            exists: false,
            size: 0,
            modified: None,
            hash: None,
        }
    }
}

struct ConfigSaveLock {
    #[cfg(windows)]
    path: PathBuf,
    file: Option<File>,
}

impl ConfigSaveLock {
    fn acquire(path: &Path) -> Result<Self> {
        let mut file = open_lock_file(path).map_err(|source| LauncherError::ConfigLock {
            path: path.to_path_buf(),
            source,
        })?;

        let payload = format!("pid={}\n", process::id());
        if let Err(source) = file.set_len(0) {
            drop(file);
            cleanup_lock_file_after_acquire_error(path);
            return Err(LauncherError::ConfigLock {
                path: path.to_path_buf(),
                source,
            });
        }
        if let Err(source) = file.write_all(payload.as_bytes()) {
            drop(file);
            cleanup_lock_file_after_acquire_error(path);
            return Err(LauncherError::ConfigLock {
                path: path.to_path_buf(),
                source,
            });
        }
        if let Err(source) = file.flush() {
            drop(file);
            cleanup_lock_file_after_acquire_error(path);
            return Err(LauncherError::ConfigLock {
                path: path.to_path_buf(),
                source,
            });
        }
        if let Err(source) = file.sync_all() {
            drop(file);
            cleanup_lock_file_after_acquire_error(path);
            return Err(LauncherError::ConfigLock {
                path: path.to_path_buf(),
                source,
            });
        }

        Ok(Self {
            #[cfg(windows)]
            path: path.to_path_buf(),
            file: Some(file),
        })
    }
}

impl Drop for ConfigSaveLock {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            drop(file);
        }
        #[cfg(windows)]
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
fn cleanup_lock_file_after_acquire_error(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(not(windows))]
fn cleanup_lock_file_after_acquire_error(_path: &Path) {}

#[cfg(windows)]
fn open_lock_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
}

#[cfg(not(windows))]
fn open_lock_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock_exclusive()?;
    Ok(file)
}

fn load_seed_config(base_dir: &Path) -> Result<Option<LauncherConfig>> {
    let seed_path = base_dir.join(WINDOWS_CONFIG_SEED_FILE_NAME);
    let exists = seed_path
        .try_exists()
        .map_err(|source| LauncherError::ConfigRead {
            path: seed_path.clone(),
            source,
        })?;
    if !exists {
        return Ok(None);
    }

    match read_config_file(&seed_path) {
        Ok((config, _)) => Ok(Some(config)),
        Err(LauncherError::ConfigParse { .. } | LauncherError::ConfigDecode { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_config_file(path: &Path) -> Result<(LauncherConfig, ConfigFileSignature)> {
    let (payload, metadata) = read_config_payload(path)?;
    let signature = config_signature_from_bytes(&payload, metadata.as_ref());
    let config = parse_config_payload(path, &payload)?;
    Ok((config, signature))
}

fn read_config_payload(path: &Path) -> Result<(Vec<u8>, Option<Metadata>)> {
    for _ in 0..CONFIG_READ_STABILITY_ATTEMPTS {
        let file = File::open(path).map_err(|source| LauncherError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let before = file
            .metadata()
            .map_err(|source| LauncherError::ConfigRead {
                path: path.to_path_buf(),
                source,
            })?;
        validate_config_payload_size(path, before.len())?;
        let mut payload = Vec::with_capacity(before.len() as usize);
        let mut limited_file = file.take(MAX_CONFIG_PAYLOAD_BYTES + 1);
        limited_file
            .read_to_end(&mut payload)
            .map_err(|source| LauncherError::ConfigRead {
                path: path.to_path_buf(),
                source,
            })?;
        let file = limited_file.into_inner();
        validate_config_payload_size(path, usize_to_u64(payload.len()))?;
        let after = file
            .metadata()
            .map_err(|source| LauncherError::ConfigRead {
                path: path.to_path_buf(),
                source,
            })?;
        validate_config_payload_size(path, after.len())?;

        if file_metadata_matches(&before, &after) && after.len() == usize_to_u64(payload.len()) {
            return Ok((payload, Some(after)));
        }
    }

    Err(LauncherError::ConfigRead {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::InvalidData,
            "configuration file changed while it was being read",
        ),
    })
}

fn validate_config_payload_size(path: &Path, size: u64) -> Result<()> {
    if size <= MAX_CONFIG_PAYLOAD_BYTES {
        return Ok(());
    }

    Err(LauncherError::ConfigRead {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "configuration file is too large: {size} bytes (max {MAX_CONFIG_PAYLOAD_BYTES} bytes)"
            ),
        ),
    })
}

fn parse_config_payload(path: &Path, payload: &[u8]) -> Result<LauncherConfig> {
    let payload = if payload.starts_with(UTF8_BOM) {
        &payload[UTF8_BOM.len()..]
    } else {
        payload
    };
    let text = std::str::from_utf8(payload).map_err(|source| LauncherError::ConfigDecode {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(text).map_err(|source| LauncherError::ConfigParse {
        path: path.to_path_buf(),
        source,
    })
}

fn read_config_file_signature(path: &Path) -> Result<ConfigFileSignature> {
    match read_config_payload(path) {
        Ok((payload, metadata)) => Ok(config_signature_from_bytes(&payload, metadata.as_ref())),
        Err(error) => {
            if let LauncherError::ConfigRead { source, .. } = &error
                && source.kind() == io::ErrorKind::NotFound
            {
                return Ok(ConfigFileSignature::missing());
            }
            Err(error)
        }
    }
}

fn read_config_file_metadata(path: &Path) -> Result<ConfigFileSignature> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(config_signature_from_metadata(&metadata)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            Ok(ConfigFileSignature::missing())
        }
        Err(source) => Err(LauncherError::ConfigRead {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn config_file_signature_matches(
    path: &Path,
    expected_signature: &ConfigFileSignature,
) -> Result<(bool, ConfigFileSignature)> {
    let metadata = read_config_file_metadata(path)?;
    if config_metadata_matches(expected_signature, &metadata) {
        if expected_signature.hash.is_none() {
            return Ok((true, metadata));
        }

        let current_signature = read_config_file_signature(path)?;
        return Ok((
            config_content_matches(&current_signature, expected_signature),
            current_signature,
        ));
    }

    if !expected_signature.exists || !metadata.exists {
        return Ok((
            config_content_matches(&metadata, expected_signature),
            metadata,
        ));
    }

    let current_signature = read_config_file_signature(path)?;
    Ok((
        config_content_matches(&current_signature, expected_signature),
        current_signature,
    ))
}

fn config_metadata_matches(
    expected_signature: &ConfigFileSignature,
    metadata: &ConfigFileSignature,
) -> bool {
    expected_signature.exists
        && metadata.exists
        && expected_signature.size == metadata.size
        && expected_signature.modified.is_some()
        && expected_signature.modified == metadata.modified
}

fn config_content_matches(left: &ConfigFileSignature, right: &ConfigFileSignature) -> bool {
    if left.exists != right.exists {
        return false;
    }
    if !left.exists {
        return true;
    }
    left.size == right.size && left.hash.is_some() && left.hash == right.hash
}

fn config_signature_from_bytes(payload: &[u8], metadata: Option<&Metadata>) -> ConfigFileSignature {
    ConfigFileSignature {
        exists: true,
        size: usize_to_u64(payload.len()),
        modified: metadata.and_then(|metadata| metadata.modified().ok()),
        hash: Some(hash_bytes(payload)),
    }
}

fn config_signature_from_metadata(metadata: &Metadata) -> ConfigFileSignature {
    ConfigFileSignature {
        exists: true,
        size: metadata.len(),
        modified: metadata.modified().ok(),
        hash: None,
    }
}

fn file_metadata_matches(left: &Metadata, right: &Metadata) -> bool {
    left.len() == right.len() && left.modified().ok() == right.modified().ok()
}

fn serialize_config(config: &LauncherConfig, path: &Path) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut payload, formatter);
    config
        .serialize(&mut serializer)
        .map_err(|source| LauncherError::ConfigSerialize {
            path: path.to_path_buf(),
            source,
        })?;
    payload.push(b'\n');
    Ok(payload)
}

fn write_temp_config_file(base_dir: &Path, target_path: &Path, payload: &[u8]) -> Result<PathBuf> {
    for _ in 0..100 {
        let temp_path = next_temp_path(base_dir);
        let file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(LauncherError::ConfigWrite {
                    path: target_path.to_path_buf(),
                    source,
                });
            }
        };
        return write_payload_to_temp(temp_path, file, target_path, payload);
    }

    Err(LauncherError::ConfigWrite {
        path: target_path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique temporary config file",
        ),
    })
}

fn write_payload_to_temp(
    temp_path: PathBuf,
    mut file: File,
    target_path: &Path,
    payload: &[u8],
) -> Result<PathBuf> {
    let write_result = file
        .write_all(payload)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all());

    match write_result {
        Ok(()) => Ok(temp_path),
        Err(source) => {
            let _ = fs::remove_file(&temp_path);
            Err(LauncherError::ConfigWrite {
                path: target_path.to_path_buf(),
                source,
            })
        }
    }
}

fn write_corrupted_config_backup(config_path: &Path, payload: &[u8]) -> Result<PathBuf> {
    let backup_base = backup_path_with_suffix(config_path, ".bak");
    for suffix in 0..1000 {
        let backup_path = if suffix == 0 {
            backup_base.clone()
        } else {
            backup_path_with_suffix(config_path, &format!(".bak.{suffix}"))
        };
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup_path)
        {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(LauncherError::ConfigWrite {
                    path: backup_path,
                    source,
                });
            }
        };
        let write_result = file
            .write_all(payload)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all());
        return match write_result {
            Ok(()) => Ok(backup_path),
            Err(source) => {
                let _ = fs::remove_file(&backup_path);
                Err(LauncherError::ConfigWrite {
                    path: backup_path,
                    source,
                })
            }
        };
    }

    Err(LauncherError::ConfigWrite {
        path: backup_base,
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique corrupted config backup file",
        ),
    })
}

fn backup_path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut backup_path = OsString::from(path.as_os_str());
    backup_path.push(suffix);
    PathBuf::from(backup_path)
}

fn next_temp_path(base_dir: &Path) -> PathBuf {
    let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(_) => 0,
    };
    base_dir.join(format!(
        ".j3launcher-config-{}-{timestamp}-{sequence}.tmp",
        process::id()
    ))
}

fn normalize_config(mut config: LauncherConfig, _path: &Path) -> Result<LauncherConfig> {
    normalize_window_config(&mut config);
    normalize_folder_tabs(&mut config.folder_tabs);
    Ok(config)
}

fn normalize_window_config(config: &mut LauncherConfig) {
    let geometry = config.window.geometry.trim();
    if geometry.is_empty() {
        config.window.geometry = DEFAULT_WINDOW_GEOMETRY.to_owned();
    } else if geometry.len() != config.window.geometry.len() {
        config.window.geometry = geometry.to_owned();
    }
    config.window.dpi_scale = normalize_dpi_scale(config.window.dpi_scale);
}

fn normalize_folder_tabs(folder_tabs: &mut Vec<LauncherTab>) {
    folder_tabs.truncate(MAX_TAB_COUNT);
    let mut seen_tab_ids = HashSet::with_capacity(folder_tabs.len());
    for (tab_index, tab) in folder_tabs.iter_mut().enumerate() {
        normalize_tab(tab, tab_index, &mut seen_tab_ids);
    }
}

fn normalize_tab(tab: &mut LauncherTab, tab_index: usize, seen_tab_ids: &mut HashSet<String>) {
    let mut id = tab.id.trim().to_owned();
    if id.is_empty() || seen_tab_ids.contains(&id) {
        id = build_tab_id(seen_tab_ids);
    }
    seen_tab_ids.insert(id.clone());
    tab.id = id;

    let is_manual = tab.tab_type == TabType::Manual;
    let rows_default = if is_manual {
        MANUAL_DEFAULT_BUTTON_ROWS
    } else {
        DEFAULT_BUTTON_ROWS
    };
    let cols_default = if is_manual {
        MANUAL_DEFAULT_BUTTON_COLS
    } else {
        DEFAULT_BUTTON_COLS
    };
    tab.rows = normalize_button_dimension(tab.rows, rows_default, MAX_BUTTON_ROWS);
    tab.cols = normalize_button_dimension(tab.cols, cols_default, MAX_BUTTON_COLS);
    if tab.title.is_empty() {
        tab.title = format!("Tab {}", tab_index + 1);
    }

    normalize_tab_buttons(&mut tab.buttons, tab_index, is_manual);
    if is_manual {
        let required_slots = usize::from(tab.rows) * usize::from(tab.cols);
        tab.buttons.truncate(required_slots);
        while tab.buttons.len() < required_slots {
            tab.buttons.push(LauncherButton::manual_default());
        }
        tab.folder_path.clear();
        tab.hidden_item_ids.clear();
        tab.slot_positions.clear();
        tab.scan_signature = None;
        tab.scan_item_order = None;
        return;
    }

    tab.folder_path = normalize_path_text(&tab.folder_path);
    tab.hidden_item_ids = normalize_hidden_item_ids(std::mem::take(&mut tab.hidden_item_ids));
    let valid_button_ids = tab
        .buttons
        .iter()
        .map(|button| button.item_id.as_str())
        .collect::<HashSet<_>>();
    tab.slot_positions = normalize_slot_positions(
        std::mem::take(&mut tab.slot_positions),
        &valid_button_ids,
        tab.cols,
    );
    tab.scan_signature = normalize_scan_signature(tab.scan_signature.take(), &tab.folder_path);
    tab.scan_item_order = tab
        .scan_signature
        .as_ref()
        .and_then(|_| normalize_scan_item_order(tab.scan_item_order.take(), &valid_button_ids));
}

fn normalize_button_dimension(value: u16, default: u16, max_value: u16) -> u16 {
    if value == 0 {
        default
    } else {
        value.clamp(1, max_value)
    }
}

fn normalize_tab_buttons(buttons: &mut Vec<LauncherButton>, tab_index: usize, is_manual: bool) {
    if is_manual {
        for (button_index, button) in buttons.iter_mut().enumerate() {
            normalize_button(button, tab_index, button_index, true);
        }
        return;
    }

    let mut seen_button_ids = HashSet::new();
    let mut button_index = 0;
    buttons.retain_mut(|button| {
        normalize_button(button, tab_index, button_index, false);
        button_index += 1;
        if button.item_id.is_empty() || seen_button_ids.contains(button.item_id.as_str()) {
            return false;
        }
        seen_button_ids.insert(button.item_id.clone());
        true
    });
}

fn normalize_button(
    button: &mut LauncherButton,
    tab_index: usize,
    button_index: usize,
    is_manual: bool,
) {
    let source_path = if button.source_path.trim().is_empty() {
        normalize_path_text(&button.path)
    } else {
        normalize_path_text(&button.source_path)
    };
    if button.path.is_empty() {
        button.path = source_path.clone();
    }

    if is_manual {
        button.item_id.clear();
    } else {
        let mut item_id = button.item_id.trim().to_owned();
        if item_id.is_empty() {
            let fallback_seed = format!("{tab_index}-{button_index}");
            item_id = make_item_id(&source_path)
                .or_else(|| make_item_id(&button.path))
                .unwrap_or_else(|| format!("legacy-item-{fallback_seed}"));
        }
        button.item_id = item_id;
    }

    let mut source_name = button.source_name.trim().to_owned();
    if source_name.is_empty() {
        source_name = if source_path.is_empty() {
            button.name.clone()
        } else {
            path_basename(&source_path).unwrap_or_default()
        };
    }
    button.source_name = source_name;
    button.source_path = source_path;
    button.action = if button.action == 1 { 1 } else { 0 };
}

fn normalize_scan_signature(
    signature: Option<ScanSignature>,
    folder_path: &str,
) -> Option<ScanSignature> {
    let signature = signature?;
    if folder_path.is_empty()
        || signature.version != FOLDER_SCAN_SIGNATURE_VERSION
        || signature.mtime_ns > i64::MAX as u64
        || signature.ctime_ns > i64::MAX as u64
        || signature.size > i64::MAX as u64
    {
        return None;
    }

    let signature_path = normalize_path_text(&signature.path);
    if signature_path.is_empty() || make_item_id(&signature_path) != make_item_id(folder_path) {
        return None;
    }

    Some(ScanSignature::new(
        folder_path.to_owned(),
        signature.mtime_ns,
        signature.ctime_ns,
        signature.size,
    ))
}

fn sync_manual_button_source_metadata(button: &mut LauncherButton) {
    if button.action != 0 || button.path.trim().is_empty() {
        button.source_path.clear();
        button.source_name.clear();
        return;
    }

    let source_path = normalize_path_text(&button.path);
    button.source_path = source_path.clone();
    button.source_name = match path_basename(&source_path) {
        Some(name) => name,
        None => button.path.clone(),
    };
}

fn normalize_dpi_scale(value: Option<f64>) -> Option<f64> {
    value.filter(|scale| scale.is_finite() && *scale > 0.0)
}

fn geometry_scale_ratio(saved_dpi_scale: Option<f64>, current_dpi_scale: Option<f64>) -> f64 {
    let Some(current_scale) = normalize_dpi_scale(current_dpi_scale) else {
        return 1.0;
    };
    let Some(saved_scale) = normalize_dpi_scale(saved_dpi_scale) else {
        return current_scale;
    };
    current_scale / saved_scale
}

fn scale_window_geometry(geometry: &str, scale: f64) -> String {
    if (scale - 1.0).abs() < 0.005 {
        return geometry.trim().to_owned();
    }

    let trimmed = geometry.trim();
    let Some(separator_index) = trimmed.find(['x', 'X']) else {
        return geometry.to_owned();
    };
    let width_text = &trimmed[..separator_index];
    if width_text.is_empty() || !width_text.chars().all(|ch| ch.is_ascii_digit()) {
        return geometry.to_owned();
    }

    let height_and_position = &trimmed[separator_index + 1..];
    let height_len: usize = height_and_position
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum();
    if height_len == 0 {
        return geometry.to_owned();
    }

    let height_text = &height_and_position[..height_len];
    let position_text = &height_and_position[height_len..];
    if !is_valid_geometry_position(position_text) {
        return geometry.to_owned();
    }
    let width = match width_text.parse::<u32>() {
        Ok(width) => width,
        Err(_) => return geometry.to_owned(),
    };
    let height = match height_text.parse::<u32>() {
        Ok(height) => height,
        Err(_) => return geometry.to_owned(),
    };

    format!(
        "{}x{}{}",
        scaled_dimension(width, scale),
        scaled_dimension(height, scale),
        position_text
    )
}

fn scaled_dimension(value: u32, scale: f64) -> u32 {
    let scaled = (f64::from(value) * scale).round();
    if !scaled.is_finite() || scaled < 1.0 {
        1
    } else if scaled > f64::from(u32::MAX) {
        u32::MAX
    } else {
        scaled as u32
    }
}

fn is_valid_geometry_position(position: &str) -> bool {
    if position.is_empty() {
        return true;
    }
    let Some(next_index) = consume_signed_digits(position, 0) else {
        return false;
    };
    let Some(end_index) = consume_signed_digits(position, next_index) else {
        return false;
    };
    end_index == position.len()
}

fn consume_signed_digits(value: &str, start_index: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let sign = *bytes.get(start_index)?;
    if sign != b'+' && sign != b'-' {
        return None;
    }

    let mut index = start_index + 1;
    let digit_start = index;
    while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
        index += 1;
    }
    (index > digit_start).then_some(index)
}

fn absolute_base_dir(base_dir: &Path) -> Result<PathBuf> {
    reject_windows_only_config_path(base_dir)?;
    if base_dir.is_absolute() {
        return Ok(base_dir.to_path_buf());
    }

    let current_dir = std::env::current_dir().map_err(|source| LauncherError::ConfigRead {
        path: PathBuf::from("."),
        source,
    })?;
    Ok(current_dir.join(base_dir))
}

fn absolute_config_path(config_path: &Path) -> Result<PathBuf> {
    if config_path.as_os_str().is_empty() {
        return Err(LauncherError::Platform {
            message: String::from("설정 파일 인자가 비어 있습니다."),
        });
    }
    reject_windows_only_config_path(config_path)?;
    if config_path.is_absolute() {
        return Ok(config_path.to_path_buf());
    }

    let current_dir = std::env::current_dir().map_err(|source| LauncherError::ConfigRead {
        path: PathBuf::from("."),
        source,
    })?;
    Ok(current_dir.join(config_path))
}

fn executable_or_current_dir() -> Result<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| LauncherError::Platform {
            message: String::from("설정 기준 폴더를 확인할 수 없습니다."),
        })
}

fn resolve_config_path_from_base(base_dir: &Path, config_path: &Path) -> Result<PathBuf> {
    if config_path.as_os_str().is_empty() {
        return Err(LauncherError::Platform {
            message: String::from("설정 파일 인자가 비어 있습니다."),
        });
    }
    reject_windows_only_config_path(config_path)?;
    if config_path.is_absolute() {
        Ok(config_path.to_path_buf())
    } else {
        Ok(base_dir.join(config_path))
    }
}

#[cfg(not(windows))]
fn reject_windows_only_config_path(config_path: &Path) -> Result<()> {
    let value = config_path.to_string_lossy();
    if has_windows_path_syntax_text(&value) || has_windows_percent_env_reference(&value) {
        return Err(LauncherError::Platform {
            message: String::from(
                "Windows 전용 설정 파일 경로는 Linux에서 직접 사용할 수 없습니다.",
            ),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn reject_windows_only_config_path(_config_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn has_windows_path_syntax_text(value: &str) -> bool {
    is_windows_absolute_path_text(value) || is_windows_drive_qualified_path_text(value)
}

#[cfg(not(windows))]
fn is_windows_absolute_path_text(value: &str) -> bool {
    if value.starts_with("\\\\") {
        return true;
    }
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(drive), Some(':'), Some('\\' | '/')) if drive.is_ascii_alphabetic()
    )
}

#[cfg(not(windows))]
fn is_windows_drive_qualified_path_text(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic()
    )
}

#[cfg(not(windows))]
fn has_windows_percent_env_reference(value: &str) -> bool {
    let value = value.trim();
    if value.starts_with('/') && !value.contains('\\') && !has_windows_path_syntax_text(value) {
        return false;
    }
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('%') else {
            return false;
        };
        if !after_start[..end].is_empty() {
            return true;
        }
        rest = &after_start[end + 1..];
    }
    false
}

fn lock_path_for(path: &Path) -> PathBuf {
    let mut lock_path = OsString::from(path.as_os_str());
    lock_path.push(".lock");
    PathBuf::from(lock_path)
}

fn hash_bytes(payload: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in payload {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn invalid_index(message: &str) -> LauncherError {
    LauncherError::ConfigInvalidIndex {
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;

    use super::*;
    use crate::domain::DEFAULT_WINDOW_GEOMETRY;

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(1);
    const WINDOWS_FIXTURE: &str = include_str!("../../tests/fixtures/j3Launcher_win.json");

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> io::Result<Self> {
            let base = std::env::temp_dir();
            for _ in 0..100 {
                let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
                let candidate = base.join(format!(
                    "j3launcher-config-store-{}-{sequence}-{label}",
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

    #[test]
    fn missing_file_creates_default_config() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("missing")?;
        let store = ConfigStore::open(dir.path())?;
        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);

        assert!(config_path.is_file());
        assert_eq!(store.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        assert!(store.get_folder_tabs().is_empty());

        let payload = fs::read_to_string(config_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload)?;
        assert_eq!(parsed["Window"]["Geometry"], DEFAULT_WINDOW_GEOMETRY);
        assert_eq!(parsed["FolderTabs"], json!([]));
        Ok(())
    }

    #[test]
    fn seed_config_is_used_when_config_is_missing() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("seed")?;
        let seed_path = dir.path().join(WINDOWS_CONFIG_SEED_FILE_NAME);
        let mut payload = UTF8_BOM.to_vec();
        payload.extend_from_slice(WINDOWS_FIXTURE.as_bytes());
        fs::write(seed_path, payload)?;

        let store = ConfigStore::open(dir.path())?;
        let tabs = store.get_folder_tabs();
        let tab = match tabs.first() {
            Some(tab) => tab,
            None => return Err(test_error("seed tab was not loaded")),
        };

        assert_eq!(store.get_window_geometry(), "631x324+943+1873");
        assert_eq!(tabs.len(), 2);
        assert_eq!(tab.title, "Home");
        assert_eq!(tab.tab_type, TabType::Manual);
        assert_eq!(tab.buttons.len(), 50);
        Ok(())
    }

    #[test]
    fn open_path_uses_selected_config_file() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("selected-file")?;
        let selected_path = dir.path().join("111.json");
        let mut store = ConfigStore::open_path(&selected_path)?;

        store.save_window_geometry("900x700")?;

        assert_eq!(store.config_path(), selected_path);
        assert!(selected_path.is_file());
        assert!(!dir.path().join(APP_CONFIG_FILE_NAME).exists());
        let payload = fs::read_to_string(selected_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload)?;
        assert_eq!(parsed["Window"]["Geometry"], "900x700");
        Ok(())
    }

    #[test]
    fn relative_selected_config_path_is_resolved_against_base_dir() {
        let base_dir = Path::new("C:/launcher");
        let config_path = Path::new("profiles/222.json");

        let resolved =
            resolve_config_path_from_base(base_dir, config_path).expect("path should resolve");

        assert_eq!(resolved, PathBuf::from("C:/launcher/profiles/222.json"));
    }

    #[cfg(not(windows))]
    #[test]
    fn windows_only_config_paths_are_rejected_before_linux_resolution() {
        let base_dir = Path::new("/launcher");
        for config_path in [
            Path::new("C:\\Users\\me\\j3Launcher.json"),
            Path::new("C:Users\\me\\j3Launcher.json"),
            Path::new("\\\\server\\share\\j3Launcher.json"),
            Path::new("%USERPROFILE%\\j3Launcher.json"),
        ] {
            let error = resolve_config_path_from_base(base_dir, config_path)
                .expect_err("Windows-only paths should not become Linux-relative paths");
            assert_eq!(
                error.user_message(),
                "Windows 전용 설정 파일 경로는 Linux에서 직접 사용할 수 없습니다."
            );

            let error = absolute_config_path(config_path)
                .expect_err("Windows-only paths should not resolve against cwd");
            assert_eq!(
                error.user_message(),
                "Windows 전용 설정 파일 경로는 Linux에서 직접 사용할 수 없습니다."
            );
        }

        for base_dir in [
            Path::new("C:\\Users\\me"),
            Path::new("C:Users\\me"),
            Path::new("\\\\server\\share"),
            Path::new("%USERPROFILE%\\Launcher"),
        ] {
            let error = absolute_base_dir(base_dir)
                .expect_err("Windows-only base dirs should not resolve against cwd");
            assert_eq!(
                error.user_message(),
                "Windows 전용 설정 파일 경로는 Linux에서 직접 사용할 수 없습니다."
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_literal_percent_config_paths_are_not_rejected_on_linux() {
        let config_path = Path::new("/tmp/%profile%/j3Launcher.json");

        let resolved = resolve_config_path_from_base(Path::new("/launcher"), config_path)
            .expect("POSIX literal percent path should resolve");

        assert_eq!(resolved, PathBuf::from(config_path));
    }

    #[test]
    fn save_replaces_config_without_leftover_temp_file() -> std::result::Result<(), Box<dyn Error>>
    {
        let dir = TestDir::new("atomic")?;
        let mut store = ConfigStore::open(dir.path())?;

        store.save_window_geometry("1024x768")?;

        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);
        let payload = fs::read_to_string(config_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload)?;
        assert_eq!(parsed["Window"]["Geometry"], "1024x768");

        for entry in fs::read_dir(dir.path())? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            assert!(
                !(name.starts_with(".j3launcher-config-") && name.ends_with(".tmp")),
                "temporary config file was not cleaned up: {name}"
            );
        }
        Ok(())
    }

    #[test]
    fn save_failure_rolls_back_memory_state() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("rollback")?;
        let mut store = ConfigStore::open(dir.path())?;
        let lock_path = lock_path_for(&dir.path().join(APP_CONFIG_FILE_NAME));
        let lock = ConfigSaveLock::acquire(&lock_path)?;

        let result = store.save_window_geometry("900x700");

        assert!(matches!(result, Err(LauncherError::ConfigLock { .. })));
        assert_eq!(store.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        drop(lock);
        #[cfg(windows)]
        assert!(!lock_path.exists());
        #[cfg(not(windows))]
        assert!(lock_path.exists());
        Ok(())
    }

    #[test]
    fn stale_store_save_is_rejected() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("stale")?;
        let mut stale_store = ConfigStore::open(dir.path())?;
        let mut fresh_store = ConfigStore::open(dir.path())?;

        fresh_store.save_window_geometry("900x700")?;
        let result = stale_store.save_window_geometry("700x500");

        assert!(matches!(
            result,
            Err(LauncherError::ConfigSaveConflict { .. })
        ));
        assert_eq!(stale_store.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        stale_store.reload()?;
        assert_eq!(stale_store.get_window_geometry(), "900x700");
        Ok(())
    }

    #[test]
    fn same_metadata_save_conflict_checks_hash() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("same-metadata")?;
        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);
        let previous_payload = b"previous";
        let current_payload = b"external";
        fs::write(&config_path, current_payload)?;

        let mut expected_signature = read_config_file_metadata(&config_path)?;
        expected_signature.hash = Some(hash_bytes(previous_payload));
        let current_metadata = read_config_file_metadata(&config_path)?;
        assert!(config_metadata_matches(
            &expected_signature,
            &current_metadata
        ));

        let (matches, current_signature) =
            config_file_signature_matches(&config_path, &expected_signature)?;

        assert!(!matches);
        assert_eq!(current_signature.hash, Some(hash_bytes(current_payload)));
        Ok(())
    }

    #[test]
    fn oversized_config_payload_is_rejected_without_parsing()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("oversized")?;
        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);
        let file = File::create(&config_path)?;
        file.set_len(MAX_CONFIG_PAYLOAD_BYTES + 1)?;

        let error = match read_config_payload(&config_path) {
            Ok(_) => return Err(test_error("oversized config payload was accepted")),
            Err(error) => error,
        };
        let source = match error {
            LauncherError::ConfigRead { source, .. } => source,
            _ => return Err(test_error("oversized config returned an unexpected error")),
        };

        assert_eq!(source.kind(), io::ErrorKind::InvalidData);
        assert!(source.to_string().contains("too large"));
        Ok(())
    }

    #[test]
    fn corrupt_json_is_backed_up_and_replaced_with_default()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("corrupt")?;
        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);
        fs::write(&config_path, b"{ broken json")?;

        let store = ConfigStore::open(dir.path())?;

        assert_eq!(store.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        assert!(dir.path().join("j3Launcher.json.bak").is_file());
        let payload = fs::read_to_string(config_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload)?;
        assert_eq!(parsed["Window"]["Geometry"], DEFAULT_WINDOW_GEOMETRY);
        assert_eq!(parsed["FolderTabs"], json!([]));
        Ok(())
    }

    #[test]
    fn invalid_utf8_config_is_backed_up_and_replaced_with_default()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("corrupt-utf8")?;
        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);
        let corrupted_payload = b"{\xFF broken utf8";
        fs::write(&config_path, corrupted_payload)?;

        let store = ConfigStore::open(dir.path())?;

        assert_eq!(store.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        let backup_path = dir.path().join("j3Launcher.json.bak");
        assert_eq!(fs::read(backup_path)?, corrupted_payload);
        let payload = fs::read_to_string(config_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload)?;
        assert_eq!(parsed["Window"]["Geometry"], DEFAULT_WINDOW_GEOMETRY);
        assert_eq!(parsed["FolderTabs"], json!([]));
        Ok(())
    }

    #[test]
    fn corrupted_backup_skips_claimed_base_candidate() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("corrupt-backup-collision")?;
        let config_path = dir.path().join(APP_CONFIG_FILE_NAME);
        let claimed_backup_path = dir.path().join("j3Launcher.json.bak");
        fs::write(&claimed_backup_path, b"claimed")?;

        let backup_path = write_corrupted_config_backup(&config_path, b"{ broken json")?;

        assert_eq!(backup_path, dir.path().join("j3Launcher.json.bak.1"));
        assert_eq!(fs::read(&claimed_backup_path)?, b"claimed");
        assert_eq!(fs::read(backup_path)?, b"{ broken json");
        Ok(())
    }

    #[test]
    fn non_object_json_payload_is_config_parse_error() {
        let result = parse_config_payload(Path::new("j3Launcher.json"), b"[\"not a config\"]");

        assert!(matches!(result, Err(LauncherError::ConfigParse { .. })));
    }

    #[test]
    fn invalid_seed_config_falls_back_to_default_when_config_is_missing()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("invalid-seed")?;
        fs::write(
            dir.path().join(WINDOWS_CONFIG_SEED_FILE_NAME),
            b"{ broken json",
        )?;

        let store = ConfigStore::open(dir.path())?;

        assert_eq!(store.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        assert!(store.get_folder_tabs().is_empty());
        Ok(())
    }

    #[test]
    fn existing_unheld_lock_file_does_not_block_future_saves()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("stale-lock")?;
        let mut store = ConfigStore::open(dir.path())?;
        let lock_path = lock_path_for(&dir.path().join(APP_CONFIG_FILE_NAME));
        fs::write(&lock_path, b"pid=1\n")?;

        store.save_window_geometry("900x700")?;

        assert_eq!(store.get_window_geometry(), "900x700");
        #[cfg(windows)]
        assert!(!lock_path.exists());
        #[cfg(not(windows))]
        assert!(lock_path.exists());
        Ok(())
    }

    #[test]
    fn window_geometry_dpi_scaling_preserves_valid_position_and_skips_malformed() {
        assert_eq!(
            scale_window_geometry("800x600 trailing", 2.0),
            "800x600 trailing"
        );
        assert_eq!(scale_window_geometry("800X600", 2.0), "1600x1200");
        assert_eq!(
            scale_window_geometry("800x600+10+20", 2.0),
            "1600x1200+10+20"
        );
        assert_eq!(
            scale_window_geometry("800x600-10+20", 2.0),
            "1600x1200-10+20"
        );
    }

    #[test]
    fn normalize_config_matches_value_roundtrip_for_typed_snapshot()
    -> std::result::Result<(), Box<dyn Error>> {
        let mut config = LauncherConfig::default();
        config.window.geometry = String::from(" 1024x768 ");
        config.window.dpi_scale = Some(-1.0);
        config
            .window
            .extra
            .insert(String::from("Pinned"), json!(true));
        config.extra.insert(String::from("Custom"), json!("kept"));

        config.folder_tabs = vec![
            LauncherTab {
                id: String::from(" duplicate "),
                tab_type: TabType::Manual,
                title: String::new(),
                folder_path: String::from("C:/ignored"),
                rows: 0,
                cols: 0,
                hidden_item_ids: vec![String::from("ignored")],
                slot_positions: BTreeMap::from([(String::from("ignored"), 1)]),
                buttons: vec![LauncherButton {
                    item_id: String::from("manual-id"),
                    source_name: String::from(" "),
                    source_path: String::new(),
                    is_dir: false,
                    name: String::from("Manual Button"),
                    path: String::from("C:/Tools/app.exe"),
                    params: String::new(),
                    admin: false,
                    action: 2,
                    auto_enter: false,
                }],
                scan_signature: Some(ScanSignature::new("C:/ignored", 1, 2, 3)),
                scan_item_order: Some(Vec::new()),
            },
            LauncherTab {
                id: String::from(" duplicate "),
                tab_type: TabType::Folder,
                title: String::from("Folder"),
                folder_path: String::from(" C:/Folder "),
                rows: 600,
                cols: 0,
                hidden_item_ids: vec![String::from(" app "), String::from(""), String::from("app")],
                slot_positions: BTreeMap::from([
                    (String::from(" app "), 2),
                    (String::from("missing"), 3),
                ]),
                buttons: vec![
                    LauncherButton {
                        item_id: String::from(" app "),
                        source_name: String::new(),
                        source_path: String::from(" C:/Folder/app.exe "),
                        is_dir: false,
                        name: String::from("App"),
                        path: String::new(),
                        params: String::new(),
                        admin: false,
                        action: 1,
                        auto_enter: true,
                    },
                    LauncherButton {
                        item_id: String::from("app"),
                        source_name: String::new(),
                        source_path: String::from(" C:/Folder/duplicate.exe "),
                        is_dir: false,
                        name: String::from("Duplicate"),
                        path: String::new(),
                        params: String::new(),
                        admin: false,
                        action: 0,
                        auto_enter: false,
                    },
                ],
                scan_signature: Some(ScanSignature::new("C:/Folder", 1, 2, 3)),
                scan_item_order: Some(vec![String::from(" app ")]),
            },
        ];

        let expected = LauncherConfig::from_value(config.to_value()?);
        let normalized = normalize_config(config, Path::new("j3Launcher.json"))?;

        assert_eq!(normalized, expected);
        Ok(())
    }

    #[test]
    fn normalize_config_truncates_manual_buttons_to_layout_slots()
    -> std::result::Result<(), Box<dyn Error>> {
        let config = LauncherConfig {
            folder_tabs: vec![LauncherTab {
                id: String::from("manual"),
                tab_type: TabType::Manual,
                title: String::from("Manual"),
                folder_path: String::new(),
                rows: 1,
                cols: 1,
                hidden_item_ids: Vec::new(),
                slot_positions: BTreeMap::new(),
                buttons: vec![
                    LauncherButton {
                        item_id: String::new(),
                        source_name: String::new(),
                        source_path: String::new(),
                        is_dir: false,
                        name: String::from("Visible"),
                        path: String::new(),
                        params: String::new(),
                        admin: false,
                        action: 0,
                        auto_enter: false,
                    },
                    LauncherButton {
                        item_id: String::new(),
                        source_name: String::new(),
                        source_path: String::new(),
                        is_dir: false,
                        name: String::from("Hidden"),
                        path: String::new(),
                        params: String::new(),
                        admin: false,
                        action: 0,
                        auto_enter: false,
                    },
                ],
                scan_signature: None,
                scan_item_order: None,
            }],
            ..LauncherConfig::default()
        };

        let normalized = normalize_config(config, Path::new("j3Launcher.json"))?;

        let tab = match normalized.folder_tabs.first() {
            Some(tab) => tab,
            None => return Err(test_error("manual tab was not normalized")),
        };
        assert_eq!(tab.buttons.len(), 1);
        assert_eq!(tab.buttons[0].name, "Visible");
        Ok(())
    }

    #[test]
    fn manual_button_info_rejects_index_outside_layout_slots()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("manual-button-index")?;
        let store = ConfigStore::open(dir.path())?;
        let config = LauncherConfig {
            folder_tabs: LauncherConfig::from_value(json!({
                "FolderTabs": [{
                    "id": "manual",
                    "tab_type": "manual",
                    "title": "Manual",
                    "rows": 1,
                    "cols": 1,
                    "buttons": []
                }]
            }))
            .folder_tabs,
            ..LauncherConfig::default()
        };

        let result =
            store.prepare_button_info_config(&config, 0, usize::MAX, ButtonInfo::default());

        assert!(matches!(
            result,
            Err(LauncherError::ConfigInvalidIndex { .. })
        ));
        Ok(())
    }

    #[test]
    fn manual_button_info_extends_missing_slots_within_layout()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("manual-button-fill")?;
        let store = ConfigStore::open(dir.path())?;
        let config = LauncherConfig {
            folder_tabs: LauncherConfig::from_value(json!({
                "FolderTabs": [{
                    "id": "manual",
                    "tab_type": "manual",
                    "title": "Manual",
                    "rows": 2,
                    "cols": 2,
                    "buttons": [{}]
                }]
            }))
            .folder_tabs,
            ..LauncherConfig::default()
        };

        let target = store.prepare_button_info_config(
            &config,
            0,
            3,
            ButtonInfo {
                name: String::from("Fourth"),
                path: String::from("C:/Tools/fourth.exe"),
                ..ButtonInfo::default()
            },
        )?;

        let tab = match target.folder_tabs.first() {
            Some(tab) => tab,
            None => return Err(test_error("manual tab was not prepared")),
        };
        assert_eq!(tab.buttons.len(), 4);
        let button = match tab.buttons.get(3) {
            Some(button) => button,
            None => return Err(test_error("manual button slot was not prepared")),
        };
        assert_eq!(button.name, "Fourth");
        Ok(())
    }

    #[test]
    fn button_info_is_updated_and_persisted() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("button")?;
        let mut store = ConfigStore::open(dir.path())?;
        let tabs = LauncherConfig::from_value(json!({
            "FolderTabs": [{
                "id": "manual",
                "tab_type": "manual",
                "title": "Manual",
                "rows": 1,
                "cols": 1,
                "buttons": [{}]
            }]
        }))
        .folder_tabs;
        store.set_folder_tabs(tabs)?;

        let info = ButtonInfo {
            name: String::from("Tool"),
            path: String::from("C:/Tools/app.exe"),
            params: String::from("--fast"),
            admin: true,
            action: 0,
            auto_enter: true,
        };
        store.set_button_info(0, 0, info.clone())?;

        assert_eq!(store.get_button_info(0, 0), info);
        let reloaded = ConfigStore::open(dir.path())?;
        assert_eq!(reloaded.get_button_info(0, 0), info);

        let tabs = reloaded.get_folder_tabs();
        let tab = match tabs.first() {
            Some(tab) => tab,
            None => return Err(test_error("manual tab was not persisted")),
        };
        let button = match tab.buttons.first() {
            Some(button) => button,
            None => return Err(test_error("manual button was not persisted")),
        };
        assert_eq!(button.source_path, "C:\\Tools\\app.exe");
        assert_eq!(button.source_name, "app.exe");
        Ok(())
    }

    #[test]
    fn dark_theme_is_updated_and_persisted() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("dark-theme")?;
        let mut store = ConfigStore::open(dir.path())?;

        assert!(!store.dark_theme());
        store.set_dark_theme(true)?;

        assert!(store.dark_theme());
        let payload = fs::read_to_string(dir.path().join(APP_CONFIG_FILE_NAME))?;
        let parsed: serde_json::Value = serde_json::from_str(&payload)?;
        assert_eq!(parsed["Window"]["DarkTheme"], true);

        let reloaded = ConfigStore::open(dir.path())?;
        assert!(reloaded.dark_theme());
        Ok(())
    }

    fn test_error(message: &str) -> Box<dyn Error> {
        Box::new(io::Error::other(message.to_owned()))
    }
}
