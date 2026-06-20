use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW};

use crate::Result;
use crate::domain::{FolderScanResult, ScanItem, ScanSignature};
use crate::infra::folder_scan::{
    FolderScanOptions, ScanCancelToken, cancel_scan, new_scan_cancel_token,
    scan_folder_items_with_options,
};

const SCAN_COMPLETE_POST_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub(super) struct ScanWorker {
    cancel_token: ScanCancelToken,
    result_slot: Arc<Mutex<Option<ScanCompleteMessage>>>,
    join: Option<JoinHandle<()>>,
}

impl ScanWorker {
    pub(super) fn spawn(
        mut request: ScanRequest,
        hwnd_value: usize,
        completion_message: u32,
    ) -> std::io::Result<Self> {
        let token = new_scan_cancel_token();
        let result_slot = Arc::new(Mutex::new(None));
        let options = request.take_options(token.clone());
        let folder_path = request.folder_path();
        let worker_request = request;
        let worker_result_slot = Arc::clone(&result_slot);
        let join = thread::Builder::new()
            .name(String::from("folder-scan-worker"))
            .spawn(move || {
                let result = scan_folder_items_with_options(folder_path, options);
                let message = ScanCompleteMessage {
                    request: worker_request,
                    result,
                };
                if let Ok(mut slot) = worker_result_slot.lock() {
                    *slot = Some(message);
                }
                post_scan_complete_with_retry(hwnd_value, completion_message);
            })?;

        Ok(Self {
            cancel_token: token,
            result_slot,
            join: Some(join),
        })
    }

    pub(super) fn cancel(&self) {
        cancel_scan(&self.cancel_token);
    }

    pub(super) fn cancel_without_join(self) {
        self.cancel();
    }

    pub(super) fn join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    pub(super) fn take_result(&self) -> Option<ScanCompleteMessage> {
        self.result_slot.lock().ok()?.take()
    }
}

fn post_scan_complete_with_retry(hwnd_value: usize, completion_message: u32) {
    let hwnd = hwnd_value as HWND;
    loop {
        // Safety: hwnd_value was captured from the UI window when the scan
        // started. The message only asks the UI thread to drain the result slot.
        if unsafe { PostMessageW(hwnd, completion_message, 0, 0) } != 0 {
            return;
        }
        // Safety: hwnd is used only as an opaque borrowed window handle. If the
        // window is gone, there is no UI owner left to drain the scan result.
        if unsafe { IsWindow(hwnd) } == 0 {
            return;
        }
        thread::sleep(SCAN_COMPLETE_POST_RETRY_DELAY);
    }
}

#[derive(Debug)]
pub(super) enum ScanRequest {
    AddFolder {
        folder_path: String,
    },
    SetFolder {
        tab_id: String,
        folder_path: String,
        known_signature: Option<ScanSignature>,
        known_items: Option<Vec<ScanItem>>,
    },
    Refresh {
        tab_id: String,
        folder_path: String,
        known_signature: Option<ScanSignature>,
    },
    Reset {
        tab_id: String,
        folder_path: String,
        known_signature: Option<ScanSignature>,
        known_items: Option<Vec<ScanItem>>,
    },
}

impl ScanRequest {
    fn folder_path(&self) -> PathBuf {
        match self {
            Self::AddFolder { folder_path }
            | Self::SetFolder { folder_path, .. }
            | Self::Refresh { folder_path, .. }
            | Self::Reset { folder_path, .. } => PathBuf::from(folder_path),
        }
    }

    fn take_options(&mut self, cancel_token: ScanCancelToken) -> FolderScanOptions {
        match self {
            Self::AddFolder { .. } => FolderScanOptions {
                cancel_token: Some(cancel_token),
                known_signature: None,
                known_items: None,
                allow_signature_only_unchanged: false,
            },
            Self::SetFolder {
                known_signature,
                known_items,
                ..
            }
            | Self::Reset {
                known_signature,
                known_items,
                ..
            } => FolderScanOptions {
                cancel_token: Some(cancel_token),
                known_signature: known_signature.take(),
                known_items: known_items.take(),
                allow_signature_only_unchanged: false,
            },
            Self::Refresh {
                known_signature, ..
            } => FolderScanOptions {
                cancel_token: Some(cancel_token),
                known_signature: known_signature.take(),
                known_items: None,
                allow_signature_only_unchanged: true,
            },
        }
    }
}

#[derive(Debug)]
pub(super) struct ScanCompleteMessage {
    pub(super) request: ScanRequest,
    pub(super) result: Result<FolderScanResult>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn scan_worker_cancel_without_join_returns_before_worker_exit()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let token = new_scan_cancel_token();
        let result_slot = Arc::new(Mutex::new(None));
        let (started_tx, started_rx) = mpsc::channel();
        let (observed_cancel_tx, observed_cancel_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let worker_token = token.clone();
        let join = thread::Builder::new()
            .name(String::from("scan-worker-detach-test"))
            .spawn(move || {
                let _ = started_tx.send(());
                while !worker_token.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(5));
                }
                let _ = observed_cancel_tx.send(());
                let _ = release_rx.recv();
                let _ = finished_tx.send(());
            })?;
        started_rx.recv_timeout(Duration::from_secs(1))?;
        let worker = ScanWorker {
            cancel_token: token,
            result_slot,
            join: Some(join),
        };
        let (returned_tx, returned_rx) = mpsc::channel();
        let canceler = thread::Builder::new()
            .name(String::from("scan-worker-cancel-detach-test"))
            .spawn(move || {
                worker.cancel_without_join();
                let _ = returned_tx.send(());
            })?;

        observed_cancel_rx.recv_timeout(Duration::from_secs(1))?;
        returned_rx.recv_timeout(Duration::from_secs(1))?;
        assert!(
            finished_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );
        release_tx.send(())?;
        finished_rx.recv_timeout(Duration::from_secs(1))?;
        canceler
            .join()
            .map_err(|_| std::io::Error::other("canceler thread panicked"))?;
        Ok(())
    }

    #[test]
    fn scan_request_take_options_moves_known_scan_state() {
        let signature = ScanSignature::new("C:\\Tools", 1, 2, 3);
        let item = ScanItem::new("tool-id", "Tool.exe", "C:\\Tools\\Tool.exe", false);
        let token = new_scan_cancel_token();
        let mut request = ScanRequest::SetFolder {
            tab_id: String::from("tab-1"),
            folder_path: String::from("C:\\Tools"),
            known_signature: Some(signature.clone()),
            known_items: Some(vec![item.clone()]),
        };

        let options = request.take_options(token.clone());

        assert!(
            options
                .cancel_token
                .as_ref()
                .is_some_and(|candidate| Arc::ptr_eq(candidate, &token))
        );
        assert_eq!(options.known_signature, Some(signature));
        assert_eq!(options.known_items, Some(vec![item]));
        assert!(!options.allow_signature_only_unchanged);
        match request {
            ScanRequest::SetFolder {
                known_signature,
                known_items,
                ..
            } => {
                assert!(known_signature.is_none());
                assert!(known_items.is_none());
            }
            _ => panic!("expected set folder request"),
        }
    }

    #[test]
    fn refresh_request_uses_signature_only_unchanged_without_known_items() {
        let signature = ScanSignature::new("C:\\Tools", 1, 2, 3);
        let token = new_scan_cancel_token();
        let mut request = ScanRequest::Refresh {
            tab_id: String::from("tab-1"),
            folder_path: String::from("C:\\Tools"),
            known_signature: Some(signature.clone()),
        };

        let options = request.take_options(token.clone());

        assert!(
            options
                .cancel_token
                .as_ref()
                .is_some_and(|candidate| Arc::ptr_eq(candidate, &token))
        );
        assert_eq!(options.known_signature, Some(signature));
        assert!(options.known_items.is_none());
        assert!(options.allow_signature_only_unchanged);
        match request {
            ScanRequest::Refresh {
                known_signature, ..
            } => assert!(known_signature.is_none()),
            _ => panic!("expected refresh request"),
        }
    }
}
