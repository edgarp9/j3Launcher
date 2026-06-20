use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::button::{make_item_id, normalize_path_text};
use crate::domain::{
    FolderScanResult, ScanFailure, ScanItem, ScanSignature, scan_signatures_match,
};
use crate::{LauncherError, Result};

pub type ScanCancelToken = Arc<AtomicBool>;

#[derive(Debug, Clone, Default)]
pub struct FolderScanOptions {
    pub cancel_token: Option<ScanCancelToken>,
    pub known_signature: Option<ScanSignature>,
    pub known_items: Option<Vec<ScanItem>>,
    pub allow_signature_only_unchanged: bool,
}

pub fn new_scan_cancel_token() -> ScanCancelToken {
    Arc::new(AtomicBool::new(false))
}

pub fn cancel_scan(token: &ScanCancelToken) {
    token.store(true, Ordering::SeqCst);
}

pub fn scan_folder_items(folder_path: impl AsRef<Path>) -> Result<FolderScanResult> {
    scan_folder_items_with_options(folder_path, FolderScanOptions::default())
}

pub fn scan_folder_items_with_options(
    folder_path: impl AsRef<Path>,
    options: FolderScanOptions,
) -> Result<FolderScanResult> {
    let FolderScanOptions {
        cancel_token,
        known_signature,
        known_items,
        allow_signature_only_unchanged,
    } = options;

    if scan_cancel_requested(cancel_token.as_ref()) {
        return Ok(FolderScanResult::cancelled());
    }

    let folder = normalized_folder_path(folder_path.as_ref())?;
    if !folder.is_dir() {
        return Err(LauncherError::FolderScanInvalid {
            path: folder,
            message: String::from("folder not found"),
        });
    }

    let initial_signature = build_folder_scan_signature(&folder).ok();
    if scan_signatures_match(known_signature.as_ref(), initial_signature.as_ref()) {
        if scan_cancel_requested(cancel_token.as_ref()) {
            return Ok(FolderScanResult::cancelled());
        }
        verify_read_dir_available(&folder)?;
        if scan_cancel_requested(cancel_token.as_ref()) {
            return Ok(FolderScanResult::cancelled());
        }
        if let Some(items) = take_known_scan_items(known_items) {
            return Ok(FolderScanResult {
                items,
                failures: Vec::new(),
                cancelled: false,
                signature: initial_signature,
                unchanged: true,
            });
        }
        if allow_signature_only_unchanged {
            return Ok(FolderScanResult {
                items: Vec::new(),
                failures: Vec::new(),
                cancelled: false,
                signature: initial_signature,
                unchanged: true,
            });
        }
    }

    let mut folder_items = Vec::new();
    let mut file_items = Vec::new();
    let mut failures = Vec::new();
    let entries = fs::read_dir(&folder).map_err(|source| scan_read_error(&folder, source))?;

    for entry_result in entries {
        if scan_cancel_requested(cancel_token.as_ref()) {
            return Ok(FolderScanResult::cancelled());
        }

        match entry_result {
            Ok(entry) => scan_entry(entry, &mut folder_items, &mut file_items, &mut failures),
            Err(source) => failures.push(ScanFailure::new(
                "<unknown>",
                format_scan_failure_detail(&source),
            )),
        }
    }

    if scan_cancel_requested(cancel_token.as_ref()) {
        return Ok(FolderScanResult::cancelled());
    }
    sort_scan_item_entries(&mut folder_items);
    if scan_cancel_requested(cancel_token.as_ref()) {
        return Ok(FolderScanResult::cancelled());
    }
    sort_scan_item_entries(&mut file_items);

    let mut items = Vec::with_capacity(folder_items.len() + file_items.len());
    items.extend(folder_items.into_iter().map(|entry| entry.item));
    items.extend(file_items.into_iter().map(|entry| entry.item));

    if scan_cancel_requested(cancel_token.as_ref()) {
        return Ok(FolderScanResult::cancelled());
    }
    let final_signature = build_folder_scan_signature(&folder).ok();
    let stable_signature =
        if scan_signatures_match(initial_signature.as_ref(), final_signature.as_ref()) {
            final_signature
        } else {
            None
        };

    Ok(FolderScanResult {
        items,
        failures,
        cancelled: false,
        signature: stable_signature,
        unchanged: false,
    })
}

pub fn build_folder_scan_signature(folder_path: impl AsRef<Path>) -> Result<ScanSignature> {
    let folder = normalized_folder_path(folder_path.as_ref())?;
    let metadata = fs::metadata(&folder).map_err(|source| scan_read_error(&folder, source))?;
    Ok(ScanSignature::new(
        normalize_path_text(&folder.to_string_lossy()),
        metadata.modified().ok().map(system_time_to_ns).unwrap_or(0),
        metadata.created().ok().map(system_time_to_ns).unwrap_or(0),
        metadata.len(),
    ))
}

#[derive(Debug)]
struct SortableScanItem {
    sort_name: String,
    index: usize,
    item: ScanItem,
}

fn scan_entry(
    entry: fs::DirEntry,
    folder_items: &mut Vec<SortableScanItem>,
    file_items: &mut Vec<SortableScanItem>,
    failures: &mut Vec<ScanFailure>,
) {
    let name = entry.file_name().to_string_lossy().into_owned();
    let file_type = match entry.file_type() {
        Ok(file_type) => file_type,
        Err(source) => {
            failures.push(ScanFailure::new(name, format_scan_failure_detail(&source)));
            return;
        }
    };
    let is_dir = file_type.is_dir();
    let path = normalize_path_text(&entry.path().to_string_lossy());
    let Some(item_id) = make_item_id(&path) else {
        failures.push(ScanFailure::new(name, "Invalid item id"));
        return;
    };
    let item = ScanItem::new(item_id, name.clone(), path, is_dir);
    let sort_entry = SortableScanItem {
        sort_name: name.to_lowercase(),
        index: if is_dir {
            folder_items.len()
        } else {
            file_items.len()
        },
        item,
    };
    if is_dir {
        folder_items.push(sort_entry);
    } else {
        file_items.push(sort_entry);
    }
}

fn sort_scan_item_entries(entries: &mut [SortableScanItem]) {
    entries.sort_by(|left, right| {
        left.sort_name
            .cmp(&right.sort_name)
            .then_with(|| left.index.cmp(&right.index))
    });
}

fn verify_read_dir_available(folder: &Path) -> Result<()> {
    fs::read_dir(folder)
        .map(|_| ())
        .map_err(|source| scan_read_error(folder, source))
}

fn take_known_scan_items(known_items: Option<Vec<ScanItem>>) -> Option<Vec<ScanItem>> {
    let mut items = known_items?;
    for item in &mut items {
        let item_id = item.item_id.trim();
        let path = item.path.trim();
        if item_id.is_empty() || item.name.is_empty() || path.is_empty() {
            return None;
        }
        let trimmed_item_id = (item.item_id.len() != item_id.len()).then(|| item_id.to_owned());
        let trimmed_path = (item.path.len() != path.len()).then(|| path.to_owned());
        if let Some(trimmed_item_id) = trimmed_item_id {
            item.item_id = trimmed_item_id;
        }
        if let Some(trimmed_path) = trimmed_path {
            item.path = trimmed_path;
        }
    }
    Some(items)
}

fn scan_cancel_requested(cancel_token: Option<&ScanCancelToken>) -> bool {
    cancel_token.is_some_and(|token| token.load(Ordering::SeqCst))
}

fn normalized_folder_path(path: &Path) -> Result<PathBuf> {
    let normalized = normalize_path_text(&path.to_string_lossy());
    if normalized.is_empty() {
        return Err(LauncherError::FolderScanInvalid {
            path: PathBuf::new(),
            message: String::from("folder path is empty"),
        });
    }
    Ok(PathBuf::from(normalized))
}

fn format_scan_failure_detail(error: &io::Error) -> String {
    let detail = error.to_string();
    if detail.trim().is_empty() {
        format!("{:?}", error.kind())
    } else {
        format!("{:?}: {detail}", error.kind())
    }
}

fn scan_read_error(path: &Path, source: io::Error) -> LauncherError {
    LauncherError::FolderScanRead {
        path: path.to_path_buf(),
        source,
    }
}

fn system_time_to_ns(time: SystemTime) -> u64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let nanos = duration.as_nanos();
            if nanos > u128::from(u64::MAX) {
                u64::MAX
            } else {
                nanos as u64
            }
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::process;
    use std::sync::atomic::AtomicU64;

    use super::*;

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
                    "j3launcher-folder-scan-{}-{sequence}-{label}",
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
    fn scans_folders_before_files_with_case_insensitive_sort()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("sort")?;
        fs::create_dir(dir.path().join("Beta"))?;
        fs::create_dir(dir.path().join("alpha"))?;
        fs::write(dir.path().join("Gamma.txt"), b"g")?;
        fs::write(dir.path().join("beta.txt"), b"b")?;

        let result = scan_folder_items(dir.path())?;
        let names: Vec<&str> = result.items.iter().map(|item| item.name.as_str()).collect();

        assert_eq!(names, vec!["alpha", "Beta", "beta.txt", "Gamma.txt"]);
        assert!(result.is_complete());
        Ok(())
    }

    #[test]
    fn scan_can_be_cancelled_before_io() -> std::result::Result<(), Box<dyn Error>> {
        let token = new_scan_cancel_token();
        cancel_scan(&token);

        let result = scan_folder_items_with_options(
            Path::new(""),
            FolderScanOptions {
                cancel_token: Some(token),
                known_signature: None,
                known_items: None,
                allow_signature_only_unchanged: false,
            },
        )?;

        assert!(result.cancelled);
        assert!(result.items.is_empty());
        Ok(())
    }

    #[test]
    fn unchanged_signature_reuses_known_items() -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("unchanged")?;
        fs::write(dir.path().join("tool.exe"), b"tool")?;
        let initial = scan_folder_items(dir.path())?;
        let signature = match initial.signature {
            Some(signature) => signature,
            None => return Err(test_error("initial scan signature was not stable")),
        };
        let known_items = vec![ScanItem::new(
            "known-id",
            "Known Tool",
            "C:\\Known\\tool.exe",
            false,
        )];

        let result = scan_folder_items_with_options(
            dir.path(),
            FolderScanOptions {
                cancel_token: None,
                known_signature: Some(signature.clone()),
                known_items: Some(known_items.clone()),
                allow_signature_only_unchanged: false,
            },
        )?;

        assert!(result.unchanged);
        assert_eq!(result.signature, Some(signature));
        assert_eq!(result.items, known_items);
        Ok(())
    }

    #[test]
    fn matching_signature_without_known_items_scans_folder()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("known-items-missing")?;
        fs::write(dir.path().join("tool.exe"), b"tool")?;
        let initial = scan_folder_items(dir.path())?;
        let signature = match initial.signature {
            Some(signature) => signature,
            None => return Err(test_error("initial scan signature was not stable")),
        };

        let result = scan_folder_items_with_options(
            dir.path(),
            FolderScanOptions {
                cancel_token: None,
                known_signature: Some(signature),
                known_items: None,
                allow_signature_only_unchanged: false,
            },
        )?;

        assert!(!result.unchanged);
        let names: Vec<&str> = result.items.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, vec!["tool.exe"]);
        Ok(())
    }

    #[test]
    fn matching_signature_can_return_unchanged_without_known_items()
    -> std::result::Result<(), Box<dyn Error>> {
        let dir = TestDir::new("signature-only-unchanged")?;
        fs::write(dir.path().join("tool.exe"), b"tool")?;
        let initial = scan_folder_items(dir.path())?;
        let signature = match initial.signature {
            Some(signature) => signature,
            None => return Err(test_error("initial scan signature was not stable")),
        };

        let result = scan_folder_items_with_options(
            dir.path(),
            FolderScanOptions {
                cancel_token: None,
                known_signature: Some(signature.clone()),
                known_items: None,
                allow_signature_only_unchanged: true,
            },
        )?;

        assert!(result.unchanged);
        assert_eq!(result.signature, Some(signature));
        assert!(result.items.is_empty());
        Ok(())
    }

    fn test_error(message: &str) -> Box<dyn Error> {
        Box::new(io::Error::other(message.to_owned()))
    }
}
