use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use crate::app::actions::UserMessage;
use crate::domain::{LauncherConfig, LauncherTab};
use crate::infra::config_store::{ButtonInfo, ConfigSaveReceipt, ConfigStore};
use crate::{LauncherError, Result};

#[derive(Debug)]
pub struct ConfigService {
    store: ConfigStore,
    committed_config: LauncherConfig,
    config_path_notice: Option<UserMessage>,
    save_worker: Option<ConfigSaveWorker>,
    next_save_sequence: u64,
    latest_save_sequence: Option<u64>,
    pending_save_count: usize,
    last_deferred_save_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredConfigSaveStatus {
    pub sequence: u64,
    pub success: bool,
    pub superseded: bool,
    pub rolled_back: bool,
    pub user_message: Option<String>,
}

impl ConfigService {
    pub fn open(base_dir: impl AsRef<Path>) -> Result<Self> {
        ConfigStore::open(base_dir).map(Self::from_store)
    }

    pub fn open_path(config_path: impl AsRef<Path>) -> Result<Self> {
        ConfigStore::open_path(config_path).map(Self::from_store)
    }

    pub fn open_from_current_dir() -> Result<Self> {
        ConfigStore::open_from_current_dir().map(Self::from_store)
    }

    pub fn open_from_executable_or_current_dir() -> Result<Self> {
        ConfigStore::open_from_executable_or_current_dir().map(Self::from_store)
    }

    pub fn open_path_from_executable_or_current_dir(config_path: impl AsRef<Path>) -> Result<Self> {
        ConfigStore::open_path_from_executable_or_current_dir(config_path).map(Self::from_store)
    }

    pub fn from_store(store: ConfigStore) -> Self {
        let committed_config = store.config().clone();
        Self {
            store,
            committed_config,
            config_path_notice: None,
            save_worker: None,
            next_save_sequence: 0,
            latest_save_sequence: None,
            pending_save_count: 0,
            last_deferred_save_error: None,
        }
    }

    pub fn with_config_path_notice(mut self, notice: Option<UserMessage>) -> Self {
        self.config_path_notice = notice;
        self
    }

    pub fn base_dir(&self) -> &Path {
        self.store.base_dir()
    }

    pub fn config_path(&self) -> &Path {
        self.store.config_path()
    }

    pub fn config_path_notice(&self) -> Option<&UserMessage> {
        self.config_path_notice.as_ref()
    }

    pub fn config(&self) -> &LauncherConfig {
        self.store.config()
    }

    pub fn reload(&mut self) -> Result<()> {
        self.finish_deferred_save_work_or_fail()?;
        self.store.reload()?;
        self.committed_config = self.store.config().clone();
        Ok(())
    }

    pub fn save(&mut self) -> Result<()> {
        self.finish_deferred_save_work_or_fail()?;
        self.store.save()?;
        self.committed_config = self.store.config().clone();
        Ok(())
    }

    pub fn get_window_geometry(&self) -> String {
        self.store.get_window_geometry()
    }

    pub fn get_window_geometry_for_dpi(&self, current_dpi_scale: Option<f64>) -> String {
        self.store.get_window_geometry_for_dpi(current_dpi_scale)
    }

    pub fn dark_theme(&self) -> bool {
        self.store.dark_theme()
    }

    pub fn set_dark_theme(&mut self, enabled: bool) -> Result<()> {
        self.finish_deferred_save_work_or_fail()?;
        self.store.set_dark_theme(enabled)?;
        self.committed_config = self.store.config().clone();
        Ok(())
    }

    pub fn save_window_geometry(&mut self, geo_str: impl Into<String>) -> Result<()> {
        self.save_window_geometry_with_dpi(geo_str, None)
    }

    pub fn save_window_geometry_with_dpi(
        &mut self,
        geo_str: impl Into<String>,
        dpi_scale: Option<f64>,
    ) -> Result<()> {
        self.finish_deferred_save_work_or_fail()?;
        self.store
            .save_window_geometry_with_dpi(geo_str, dpi_scale)?;
        self.committed_config = self.store.config().clone();
        Ok(())
    }

    pub fn get_folder_tabs(&self) -> Vec<LauncherTab> {
        self.folder_tabs().to_vec()
    }

    pub fn folder_tabs(&self) -> &[LauncherTab] {
        self.store.folder_tabs()
    }

    pub fn set_folder_tabs(&mut self, folder_tabs: Vec<LauncherTab>) -> Result<()> {
        self.finish_deferred_save_work_or_fail()?;
        self.store.set_folder_tabs(folder_tabs)?;
        self.committed_config = self.store.config().clone();
        Ok(())
    }

    pub fn set_folder_tabs_deferred<N>(
        &mut self,
        folder_tabs: Vec<LauncherTab>,
        notifier: N,
    ) -> Result<u64>
    where
        N: Fn() + Send + 'static,
    {
        let target = Arc::new(
            self.store
                .prepare_folder_tabs_config(self.store.config(), folder_tabs)?,
        );
        let sequence = self.next_deferred_save_sequence();
        self.ensure_save_worker(notifier)?;
        let request = ConfigSaveRequest {
            sequence,
            snapshot: Arc::clone(&target),
        };
        let queued = self
            .save_worker
            .as_ref()
            .is_some_and(|worker| worker.request(request));
        if !queued {
            return Err(platform_error("설정 저장 작업을 큐에 넣을 수 없습니다."));
        }

        self.pending_save_count += 1;
        self.latest_save_sequence = Some(sequence);
        self.store.replace_in_memory_config_snapshot(target);
        Ok(sequence)
    }

    pub fn get_button_info(&self, tab_idx: usize, btn_idx: usize) -> ButtonInfo {
        self.store.get_button_info(tab_idx, btn_idx)
    }

    pub fn set_button_info(
        &mut self,
        tab_idx: usize,
        btn_idx: usize,
        info: ButtonInfo,
    ) -> Result<()> {
        self.finish_deferred_save_work_or_fail()?;
        self.store.set_button_info(tab_idx, btn_idx, info)?;
        self.committed_config = self.store.config().clone();
        Ok(())
    }

    pub fn drain_deferred_save_results(&mut self) -> Vec<DeferredConfigSaveStatus> {
        let mut statuses = Vec::new();
        while let Some(result) = self
            .save_worker
            .as_ref()
            .and_then(ConfigSaveWorker::try_recv)
        {
            statuses.push(self.finalize_deferred_save_result(result));
        }
        statuses
    }

    pub fn has_pending_deferred_saves(&self) -> bool {
        self.pending_save_count > 0
    }

    pub fn last_deferred_save_error(&self) -> Option<&str> {
        self.last_deferred_save_error.as_deref()
    }

    pub fn shutdown_deferred_save_worker(&mut self) -> Vec<DeferredConfigSaveStatus> {
        if let Some(worker) = self.save_worker.as_mut() {
            let _ = worker.shutdown();
        }
        let statuses = self.drain_deferred_save_results();
        self.save_worker = None;
        if self.pending_save_count == 0 {
            self.latest_save_sequence = None;
        }
        statuses
    }

    pub fn into_config_path(mut self) -> PathBuf {
        let _ = self.shutdown_deferred_save_worker();
        self.store.config_path().to_path_buf()
    }

    pub fn finish_deferred_save_work(&mut self) -> Vec<DeferredConfigSaveStatus> {
        if self.save_worker.is_some() {
            self.shutdown_deferred_save_worker()
        } else {
            Vec::new()
        }
    }

    fn next_deferred_save_sequence(&mut self) -> u64 {
        self.next_save_sequence = self.next_save_sequence.wrapping_add(1);
        if self.next_save_sequence == 0 {
            self.next_save_sequence = 1;
        }
        self.next_save_sequence
    }

    fn ensure_save_worker<N>(&mut self, notifier: N) -> Result<()>
    where
        N: Fn() + Send + 'static,
    {
        if self.save_worker.is_none() {
            self.save_worker = Some(ConfigSaveWorker::spawn(self.store.clone(), notifier)?);
        }
        Ok(())
    }

    fn finish_deferred_save_work_or_fail(&mut self) -> Result<()> {
        let statuses = self.finish_deferred_save_work();
        deferred_save_statuses_result(&statuses)
    }

    fn finalize_deferred_save_result(
        &mut self,
        result: ConfigSaveWorkerResult,
    ) -> DeferredConfigSaveStatus {
        self.pending_save_count = self.pending_save_count.saturating_sub(1);
        if result.superseded {
            return DeferredConfigSaveStatus {
                sequence: result.sequence,
                success: true,
                superseded: true,
                rolled_back: false,
                user_message: None,
            };
        }

        let Some(save_result) = result.result else {
            return DeferredConfigSaveStatus {
                sequence: result.sequence,
                success: false,
                superseded: false,
                rolled_back: false,
                user_message: Some(String::from("설정 저장 결과가 비어 있습니다.")),
            };
        };

        match save_result {
            Ok(save_receipt) => {
                let Some(snapshot) = result.snapshot else {
                    return DeferredConfigSaveStatus {
                        sequence: result.sequence,
                        success: false,
                        superseded: false,
                        rolled_back: false,
                        user_message: Some(String::from("설정 저장 스냅샷이 비어 있습니다.")),
                    };
                };
                self.store.adopt_saved_signature(&save_receipt);
                self.committed_config = snapshot.as_ref().clone();
                if self.latest_save_sequence == Some(result.sequence) {
                    self.latest_save_sequence = None;
                }
                self.last_deferred_save_error = None;
                DeferredConfigSaveStatus {
                    sequence: result.sequence,
                    success: true,
                    superseded: false,
                    rolled_back: false,
                    user_message: None,
                }
            }
            Err(error) => {
                let user_message = error.user_message();
                self.last_deferred_save_error = Some(user_message.clone());
                let rolled_back = self.latest_save_sequence == Some(result.sequence);
                if rolled_back {
                    self.store
                        .replace_in_memory_config(self.committed_config.clone());
                    self.latest_save_sequence = None;
                }
                DeferredConfigSaveStatus {
                    sequence: result.sequence,
                    success: false,
                    superseded: false,
                    rolled_back,
                    user_message: Some(user_message),
                }
            }
        }
    }
}

impl Drop for ConfigService {
    fn drop(&mut self) {
        let _ = self.shutdown_deferred_save_worker();
    }
}

#[derive(Debug)]
struct ConfigSaveWorker {
    queue: ConfigSaveQueue,
    result_rx: mpsc::Receiver<ConfigSaveWorkerResult>,
    join: Option<JoinHandle<()>>,
}

impl ConfigSaveWorker {
    fn spawn<N>(mut store: ConfigStore, notifier: N) -> Result<Self>
    where
        N: Fn() + Send + 'static,
    {
        let queue = ConfigSaveQueue::new();
        let worker_queue = queue.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name(String::from("config-save-worker"))
            .spawn(move || {
                while let Some((request, superseded_sequences, exit_after_request)) =
                    worker_queue.take_next()
                {
                    for sequence in superseded_sequences {
                        queue_config_save_worker_result(
                            &result_tx,
                            &notifier,
                            ConfigSaveWorkerResult::superseded(sequence),
                        );
                    }

                    let snapshot = request.snapshot;
                    let save_result = store.write_config_snapshot_ref(snapshot.as_ref());
                    queue_config_save_worker_result(
                        &result_tx,
                        &notifier,
                        ConfigSaveWorkerResult {
                            sequence: request.sequence,
                            snapshot: Some(snapshot),
                            result: Some(save_result),
                            superseded: false,
                        },
                    );
                    if exit_after_request {
                        break;
                    }
                }
            })
            .map_err(|source| LauncherError::Platform {
                message: format!("설정 저장 worker를 시작할 수 없습니다: {source}"),
            })?;

        Ok(Self {
            queue,
            result_rx,
            join: Some(join),
        })
    }

    fn request(&self, request: ConfigSaveRequest) -> bool {
        self.queue.request(request)
    }

    fn try_recv(&self) -> Option<ConfigSaveWorkerResult> {
        self.result_rx.try_recv().ok()
    }

    fn shutdown(&mut self) -> bool {
        self.queue.shutdown();
        if let Some(join) = self.join.take() {
            join.join().is_ok()
        } else {
            true
        }
    }
}

impl Drop for ConfigSaveWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[derive(Debug)]
struct ConfigSaveRequest {
    sequence: u64,
    snapshot: Arc<LauncherConfig>,
}

#[derive(Debug, Clone)]
struct ConfigSaveQueue {
    shared: Arc<ConfigSaveQueueShared>,
}

impl ConfigSaveQueue {
    fn new() -> Self {
        Self {
            shared: Arc::new(ConfigSaveQueueShared {
                state: Mutex::new(ConfigSaveQueueState::default()),
                available: Condvar::new(),
            }),
        }
    }

    fn request(&self, request: ConfigSaveRequest) -> bool {
        let Ok(mut state) = self.shared.state.lock() else {
            return false;
        };
        if state.shutdown {
            return false;
        }
        if let Some(previous) = state.pending.replace(request) {
            state.superseded_sequences.push(previous.sequence);
        }
        self.shared.available.notify_one();
        true
    }

    fn take_next(&self) -> Option<(ConfigSaveRequest, Vec<u64>, bool)> {
        let mut state = self.shared.state.lock().ok()?;
        loop {
            if let Some(request) = state.pending.take() {
                let superseded_sequences = std::mem::take(&mut state.superseded_sequences);
                return Some((request, superseded_sequences, state.shutdown));
            }
            if state.shutdown {
                return None;
            }
            state = self.shared.available.wait(state).ok()?;
        }
    }

    fn shutdown(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.shutdown = true;
            self.shared.available.notify_one();
        }
    }
}

#[derive(Debug)]
struct ConfigSaveQueueShared {
    state: Mutex<ConfigSaveQueueState>,
    available: Condvar,
}

#[derive(Debug, Default)]
struct ConfigSaveQueueState {
    pending: Option<ConfigSaveRequest>,
    superseded_sequences: Vec<u64>,
    shutdown: bool,
}

#[derive(Debug)]
struct ConfigSaveWorkerResult {
    sequence: u64,
    snapshot: Option<Arc<LauncherConfig>>,
    result: Option<Result<ConfigSaveReceipt>>,
    superseded: bool,
}

impl ConfigSaveWorkerResult {
    fn superseded(sequence: u64) -> Self {
        Self {
            sequence,
            snapshot: None,
            result: None,
            superseded: true,
        }
    }
}

fn queue_config_save_worker_result<N>(
    result_tx: &mpsc::Sender<ConfigSaveWorkerResult>,
    notifier: &N,
    result: ConfigSaveWorkerResult,
) where
    N: Fn(),
{
    if result_tx.send(result).is_ok() {
        notifier();
    }
}

fn deferred_save_statuses_result(statuses: &[DeferredConfigSaveStatus]) -> Result<()> {
    if let Some(status) = statuses
        .iter()
        .find(|status| !status.success && !status.superseded)
    {
        let message = status
            .user_message
            .as_deref()
            .unwrap_or("설정 저장에 실패했습니다.");
        return Err(platform_error(message));
    }
    Ok(())
}

fn platform_error(message: impl Into<String>) -> LauncherError {
    LauncherError::Platform {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::app::folder_tabs::add_manual_tab;
    use crate::domain::DEFAULT_WINDOW_GEOMETRY;

    use super::*;

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> std::io::Result<Self> {
            let base = std::env::temp_dir();
            for _ in 0..100 {
                let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
                let candidate = base.join(format!(
                    "j3launcher-config-service-{}-{sequence}-{label}",
                    std::process::id()
                ));
                match fs::create_dir(&candidate) {
                    Ok(()) => return Ok(Self { path: candidate }),
                    Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(source) => return Err(source),
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "could not reserve test directory",
            ))
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
    fn config_service_wraps_config_store_without_exposing_io_to_ui_boundary()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = TestDir::new("open")?;
        let mut service = ConfigService::open(&dir.path)?;

        assert_eq!(service.get_window_geometry(), DEFAULT_WINDOW_GEOMETRY);
        assert!(!service.dark_theme());
        service.save_window_geometry("900x700")?;
        assert_eq!(service.get_window_geometry(), "900x700");
        assert_eq!(service.config_path(), dir.path.join("j3Launcher.json"));
        assert!(service.config_path_notice().is_none());
        Ok(())
    }

    #[test]
    fn config_service_saves_dark_theme() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = TestDir::new("dark-theme")?;
        let mut service = ConfigService::open(&dir.path)?;

        service.set_dark_theme(true)?;

        assert!(service.dark_theme());
        let reloaded = ConfigService::open(&dir.path)?;
        assert!(reloaded.dark_theme());
        Ok(())
    }

    #[test]
    fn save_queue_keeps_only_latest_pending_snapshot_before_shutdown() {
        let queue = ConfigSaveQueue::new();
        assert!(queue.request(save_request(1, "first")));
        assert!(queue.request(save_request(2, "second")));
        assert!(queue.request(save_request(3, "third")));
        queue.shutdown();

        let Some((latest, superseded_sequences, exit_after_request)) = queue.take_next() else {
            panic!("save queue did not yield pending request");
        };

        assert_eq!(latest.sequence, 3);
        assert_eq!(superseded_sequences, vec![1, 2]);
        assert!(exit_after_request);
        assert!(queue.take_next().is_none());
        assert!(!queue.request(save_request(4, "after shutdown")));
    }

    #[test]
    fn deferred_save_shutdown_flushes_pending_latest_snapshot()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = TestDir::new("deferred-shutdown")?;
        let mut service = ConfigService::open(&dir.path)?;

        service.set_folder_tabs_deferred(manual_tabs("First"), || {})?;
        service.set_folder_tabs_deferred(manual_tabs("Second"), || {})?;

        let statuses = service.shutdown_deferred_save_worker();
        assert!(statuses.iter().any(|status| status.success));
        assert!(!service.has_pending_deferred_saves());

        let reloaded = ConfigService::open(&dir.path)?;
        let tabs = reloaded.get_folder_tabs();
        assert_eq!(tabs.first().map(|tab| tab.title.as_str()), Some("Second"));
        Ok(())
    }

    #[test]
    fn deferred_save_failure_rolls_back_latest_optimistic_state()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = TestDir::new("deferred-rollback")?;
        let mut service = ConfigService::open(&dir.path)?;
        let mut competing_store = ConfigStore::open(&dir.path)?;
        competing_store.save_window_geometry("900x700")?;

        service.set_folder_tabs_deferred(manual_tabs("Unsaved"), || {})?;
        assert_eq!(
            service
                .get_folder_tabs()
                .first()
                .map(|tab| tab.title.as_str()),
            Some("Unsaved")
        );

        let statuses = service.shutdown_deferred_save_worker();

        assert!(statuses.iter().any(|status| status.rolled_back));
        assert!(service.get_folder_tabs().is_empty());
        Ok(())
    }

    #[test]
    fn synchronous_save_reports_deferred_failure_before_applying_change()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = TestDir::new("deferred-sync-failure")?;
        let mut service = ConfigService::open(&dir.path)?;
        let mut competing_store = ConfigStore::open(&dir.path)?;
        competing_store.save_window_geometry("900x700")?;

        service.set_folder_tabs_deferred(manual_tabs("Unsaved"), || {})?;

        let error = match service.set_dark_theme(true) {
            Ok(()) => {
                panic!("set_dark_theme succeeded after deferred save failure");
            }
            Err(error) => error,
        };

        assert_eq!(
            error.user_message(),
            format!(
                "설정 파일이 다른 프로세스에서 변경되어 저장을 중단했습니다: {}",
                service.config_path().display()
            )
        );
        assert!(!service.dark_theme());
        assert!(service.get_folder_tabs().is_empty());
        Ok(())
    }

    fn manual_tabs(title: &str) -> Vec<LauncherTab> {
        let mut tabs = Vec::new();
        assert!(add_manual_tab(&mut tabs).is_ok());
        if let Some(tab) = tabs.first_mut() {
            tab.title = title.to_owned();
        }
        tabs
    }

    fn save_request(sequence: u64, title: &str) -> ConfigSaveRequest {
        ConfigSaveRequest {
            sequence,
            snapshot: Arc::new(LauncherConfig {
                folder_tabs: manual_tabs(title),
                ..LauncherConfig::default()
            }),
        }
    }
}
