use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::button::{make_item_id, normalize_path_text, parse_int, truthy_string};

pub const FOLDER_SCAN_SIGNATURE_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanSignature {
    pub version: u8,
    pub path: String,
    pub mtime_ns: u64,
    pub ctime_ns: u64,
    pub size: u64,
}

impl ScanSignature {
    pub fn new(path: impl Into<String>, mtime_ns: u64, ctime_ns: u64, size: u64) -> Self {
        Self {
            version: FOLDER_SCAN_SIGNATURE_VERSION,
            path: path.into(),
            mtime_ns,
            ctime_ns,
            size,
        }
    }

    pub(crate) fn from_value(raw_signature: Option<&Value>, folder_path: &str) -> Option<Self> {
        let data = raw_signature?.as_object()?;
        if folder_path.is_empty() {
            return None;
        }

        let version = parse_int(data.get("version"))?;
        let mtime_ns = parse_int(data.get("mtime_ns"))?;
        let ctime_ns = parse_int(data.get("ctime_ns"))?;
        let size = parse_int(data.get("size"))?;
        if version != i64::from(FOLDER_SCAN_SIGNATURE_VERSION)
            || mtime_ns < 0
            || ctime_ns < 0
            || size < 0
        {
            return None;
        }

        let signature_path = normalize_path_text(&truthy_string(data.get("path")));
        if signature_path.is_empty() {
            return None;
        }

        if make_item_id(&signature_path) != make_item_id(folder_path) {
            return None;
        }

        Some(Self::new(
            folder_path.to_owned(),
            mtime_ns as u64,
            ctime_ns as u64,
            size as u64,
        ))
    }
}

pub fn scan_signatures_match(left: Option<&ScanSignature>, right: Option<&ScanSignature>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanItem {
    pub item_id: String,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

impl ScanItem {
    pub fn new(
        item_id: impl Into<String>,
        name: impl Into<String>,
        path: impl Into<String>,
        is_dir: bool,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            name: name.into(),
            path: path.into(),
            is_dir,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFailure {
    pub item_name: String,
    pub error_detail: String,
}

impl ScanFailure {
    pub fn new(item_name: impl Into<String>, error_detail: impl Into<String>) -> Self {
        Self {
            item_name: item_name.into(),
            error_detail: error_detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScanResult {
    pub items: Vec<ScanItem>,
    pub failures: Vec<ScanFailure>,
    pub cancelled: bool,
    pub signature: Option<ScanSignature>,
    pub unchanged: bool,
}

impl FolderScanResult {
    pub fn cancelled() -> Self {
        Self {
            items: Vec::new(),
            failures: Vec::new(),
            cancelled: true,
            signature: None,
            unchanged: false,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.failures.is_empty() && !self.cancelled
    }

    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }
}

pub(crate) fn scan_item_order_from_items(scanned_items: &[ScanItem]) -> Option<Vec<String>> {
    let mut seen = HashSet::new();
    let mut order = Vec::with_capacity(scanned_items.len());
    for item in scanned_items {
        let item_id = item.item_id.trim();
        if item_id.is_empty() || !seen.insert(item_id.to_owned()) {
            return None;
        }
        order.push(item_id.to_owned());
    }
    Some(order)
}
