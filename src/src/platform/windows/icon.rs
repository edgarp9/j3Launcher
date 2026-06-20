use std::collections::HashMap;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::Result;

pub const DEFAULT_ICON_SIZE: u32 = 32;
pub const DEFAULT_BUTTON_ICON_RENDER_SIZE: u32 = 20;
pub const DEFAULT_BUTTON_ICON_CACHE_MAX_ITEMS: usize = 256;
pub const DEFAULT_BUTTON_ICON_QUEUE_MAX_ITEMS: usize = 64;
pub const DEFAULT_RENDERED_ICON_CACHE_MAX_ITEMS: usize = 256;
pub const MAX_ICON_SIZE: u32 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconBitmap {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl IconBitmap {
    pub fn new(width: u32, height: u32, bgra: Vec<u8>) -> Result<Self> {
        let expected_len = bitmap_byte_len(width, height)?;
        if bgra.len() != expected_len {
            return Err(super::platform_error(format!(
                "icon bitmap buffer length mismatch: expected {expected_len}, got {}",
                bgra.len()
            )));
        }
        Ok(Self {
            width,
            height,
            bgra,
        })
    }

    pub fn transparent(width: u32, height: u32) -> Result<Self> {
        let len = bitmap_byte_len(width, height)?;
        Self::new(width, height, vec![0; len])
    }
}

#[derive(Debug)]
pub struct ExtractedIcon {
    icon: OwnedIcon,
}

impl ExtractedIcon {
    pub fn raw_handle(&self) -> usize {
        self.icon.raw_handle()
    }

    pub fn to_bgra_bitmap(&self, size: u32) -> Result<IconBitmap> {
        icon_to_bgra_bitmap_impl(&self.icon, size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IconCacheKey {
    path: String,
    size: u32,
}

impl IconCacheKey {
    pub fn new(path: impl AsRef<Path>, size: u32) -> Self {
        Self {
            path: normalize_icon_cache_path(path.as_ref()),
            size,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn size(&self) -> u32 {
        self.size
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ButtonIconKey {
    tab_id: String,
    button_id: String,
    path: String,
    target_size: u32,
}

impl ButtonIconKey {
    pub fn new(
        tab_id: impl Into<String>,
        button_id: impl Into<String>,
        path: impl Into<String>,
        target_size: u32,
    ) -> Self {
        Self {
            tab_id: tab_id.into(),
            button_id: button_id.into(),
            path: path.into(),
            target_size,
        }
    }

    pub fn target_size(&self) -> u32 {
        self.target_size
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderedIconKey {
    source: IconCacheKey,
    target_size: u32,
}

impl RenderedIconKey {
    pub fn new(source: IconCacheKey, target_size: u32) -> Self {
        Self {
            source,
            target_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LruCache<K, V> {
    max_len: usize,
    tick: u64,
    entries: HashMap<K, LruEntry<V>>,
}

#[derive(Debug, Clone)]
struct LruEntry<V> {
    value: V,
    last_used: u64,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len,
            tick: 0,
            entries: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn max_len(&self) -> usize {
        self.max_len
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let tick = self.next_tick();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = tick;
        Some(&entry.value)
    }

    pub fn get_cloned(&mut self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.get(key).cloned()
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.max_len == 0 {
            return;
        }

        let tick = self.next_tick();
        self.entries.insert(
            key,
            LruEntry {
                value,
                last_used: tick,
            },
        );

        while self.entries.len() > self.max_len {
            self.evict_lru();
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        if self.tick == 0 {
            self.renumber_ticks();
            self.tick = self.entries.len() as u64 + 1;
        }
        self.tick
    }

    fn renumber_ticks(&mut self) {
        let mut keys = self
            .entries
            .iter()
            .map(|(key, entry)| (key.clone(), entry.last_used))
            .collect::<Vec<_>>();
        keys.sort_by_key(|(_, last_used)| *last_used);
        for (index, (key, _)) in keys.into_iter().enumerate() {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.last_used = index as u64 + 1;
            }
        }
    }

    fn evict_lru(&mut self) {
        let oldest_key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest_key {
            self.entries.remove(&key);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ButtonIconRequest {
    pub generation: u64,
    pub button_key: ButtonIconKey,
    pub path: PathBuf,
    pub source_size: u32,
    pub path_exists_verified: bool,
}

impl ButtonIconRequest {
    pub fn new(
        generation: u64,
        button_key: ButtonIconKey,
        path: PathBuf,
        source_size: u32,
    ) -> Self {
        Self {
            generation,
            button_key,
            path,
            source_size,
            path_exists_verified: false,
        }
    }

    pub fn with_verified_existing_path(mut self) -> Self {
        self.path_exists_verified = true;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ButtonIconResult {
    pub generation: u64,
    pub button_key: ButtonIconKey,
    pub source_key: IconCacheKey,
    pub bitmap: Option<Arc<IconBitmap>>,
    pub error: Option<String>,
    pub cache_hit: bool,
}

pub trait IconExtractor: Send + 'static {
    fn extract_icon_bitmap(&mut self, path: &Path, size: u32) -> Result<Option<IconBitmap>>;
}

#[derive(Debug, Default)]
pub struct WindowsIconExtractor;

impl IconExtractor for WindowsIconExtractor {
    fn extract_icon_bitmap(&mut self, path: &Path, size: u32) -> Result<Option<IconBitmap>> {
        extract_file_icon_bitmap(path, size)
    }
}

enum IconWorkerCommand {
    Request(ButtonIconRequest),
    Shutdown,
}

pub struct ButtonIconWorker {
    command_tx: Option<mpsc::SyncSender<IconWorkerCommand>>,
    result_rx: mpsc::Receiver<ButtonIconResult>,
    join: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl ButtonIconWorker {
    pub fn request(&self, request: ButtonIconRequest) -> bool {
        if self.stop.load(Ordering::SeqCst) {
            return false;
        }
        let Some(sender) = self.command_tx.as_ref() else {
            return false;
        };

        match sender.try_send(IconWorkerCommand::Request(request)) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) | Err(mpsc::TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn try_recv(&self) -> Option<ButtonIconResult> {
        self.result_rx.try_recv().ok()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<ButtonIconResult> {
        self.result_rx.recv_timeout(timeout).ok()
    }

    pub fn shutdown(&mut self) -> bool {
        self.request_shutdown();
        if let Some(join) = self.join.take() {
            join.join().is_ok()
        } else {
            true
        }
    }

    pub fn shutdown_without_join(mut self) {
        self.request_shutdown();
        drop(self.join.take());
    }

    fn request_shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(sender) = self.command_tx.take() {
            let _ = sender.try_send(IconWorkerCommand::Shutdown);
        }
    }
}

impl Drop for ButtonIconWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub fn spawn_button_icon_worker<N>(cache_max_items: usize, notifier: N) -> Result<ButtonIconWorker>
where
    N: Fn() + Send + 'static,
{
    spawn_button_icon_worker_with_extractor(WindowsIconExtractor, cache_max_items, notifier)
}

pub fn spawn_button_icon_worker_with_extractor<E, N>(
    mut extractor: E,
    cache_max_items: usize,
    notifier: N,
) -> Result<ButtonIconWorker>
where
    E: IconExtractor,
    N: Fn() + Send + 'static,
{
    let queue_max_items = cache_max_items.clamp(1, DEFAULT_BUTTON_ICON_QUEUE_MAX_ITEMS);
    let (command_tx, command_rx) = mpsc::sync_channel(queue_max_items);
    let (result_tx, result_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let join = thread::Builder::new()
        .name(String::from("button-icon-worker"))
        .spawn(move || {
            let mut cache = LruCache::new(cache_max_items);
            while let Ok(command) = command_rx.recv() {
                if worker_stop.load(Ordering::SeqCst) {
                    break;
                }
                match command {
                    IconWorkerCommand::Request(request) => {
                        let result =
                            process_icon_worker_request(&mut extractor, &mut cache, request);
                        if !worker_stop.load(Ordering::SeqCst) && result_tx.send(result).is_ok() {
                            notifier();
                        }
                    }
                    IconWorkerCommand::Shutdown => break,
                }
            }
        })
        .map_err(|source| {
            super::platform_error(format!("button icon worker를 시작할 수 없습니다: {source}"))
        })?;

    Ok(ButtonIconWorker {
        command_tx: Some(command_tx),
        result_rx,
        join: Some(join),
        stop,
    })
}

fn process_icon_worker_request<E>(
    extractor: &mut E,
    cache: &mut LruCache<IconCacheKey, Option<Arc<IconBitmap>>>,
    request: ButtonIconRequest,
) -> ButtonIconResult
where
    E: IconExtractor,
{
    let source_size = match validate_icon_size(request.source_size) {
        Ok(size) => size,
        Err(error) => {
            let source_key = IconCacheKey::new(&request.path, request.source_size);
            return ButtonIconResult {
                generation: request.generation,
                button_key: request.button_key,
                source_key,
                bitmap: None,
                error: Some(error.to_string()),
                cache_hit: false,
            };
        }
    };
    let source_key = IconCacheKey::new(&request.path, source_size);
    if let Some(cached) = cache.get_cloned(&source_key) {
        return ButtonIconResult {
            generation: request.generation,
            button_key: request.button_key,
            source_key,
            bitmap: cached,
            error: None,
            cache_hit: true,
        };
    }

    let path_exists = request.path_exists_verified
        || !request.path.as_os_str().is_empty() && request.path.exists();
    let (bitmap, error) = if request.path.as_os_str().is_empty() || !path_exists {
        (None, None)
    } else {
        match extractor.extract_icon_bitmap(&request.path, source_size) {
            Ok(bitmap) => (bitmap.map(Arc::new), None),
            Err(error) => (None, Some(error.to_string())),
        }
    };
    cache.insert(source_key.clone(), bitmap.clone());

    ButtonIconResult {
        generation: request.generation,
        button_key: request.button_key,
        source_key,
        bitmap,
        error,
        cache_hit: false,
    }
}

#[derive(Debug)]
struct OwnedIcon {
    raw: NonZeroUsize,
}

impl OwnedIcon {
    #[cfg(windows)]
    fn from_hicon(hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON) -> Option<Self> {
        NonZeroUsize::new(hicon as usize).map(|raw| Self { raw })
    }

    fn raw_handle(&self) -> usize {
        self.raw.get()
    }

    #[cfg(windows)]
    fn as_hicon(&self) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
        self.raw_handle() as windows_sys::Win32::UI::WindowsAndMessaging::HICON
    }
}

#[cfg(windows)]
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

        // Safety: this object owns an HICON returned by a Win32 icon creation
        // or extraction API, and calls DestroyIcon exactly once when dropped.
        let _ = unsafe { DestroyIcon(self.as_hicon()) };
    }
}

pub fn extract_file_hicon(path: impl AsRef<Path>) -> Result<Option<ExtractedIcon>> {
    extract_file_hicon_impl(path.as_ref())
}

pub fn extract_file_icon_bitmap(path: impl AsRef<Path>, size: u32) -> Result<Option<IconBitmap>> {
    extract_file_icon_bitmap_impl(path.as_ref(), size)
}

pub fn create_icon_from_bitmap(bitmap: &IconBitmap) -> Result<ExtractedIcon> {
    create_icon_from_bitmap_impl(bitmap)
}

pub fn render_icon_bitmap(source: &IconBitmap, target_size: u32) -> Result<IconBitmap> {
    let target_size = validate_icon_size(target_size)?;
    if source.width == 0 || source.height == 0 {
        return Err(super::platform_error(
            "source icon bitmap dimensions are empty",
        ));
    }
    let expected_source_len = bitmap_byte_len(source.width, source.height)?;
    if source.bgra.len() != expected_source_len {
        return Err(super::platform_error(format!(
            "source icon bitmap buffer length mismatch: expected {expected_source_len}, got {}",
            source.bgra.len()
        )));
    }
    if source.width == target_size && source.height == target_size {
        return IconBitmap::new(target_size, target_size, source.bgra.clone());
    }

    let scale_x = f64::from(target_size) / f64::from(source.width);
    let scale_y = f64::from(target_size) / f64::from(source.height);
    let scale = scale_x.min(scale_y);
    let rendered_width = ((f64::from(source.width) * scale).round() as u32).clamp(1, target_size);
    let rendered_height = ((f64::from(source.height) * scale).round() as u32).clamp(1, target_size);
    let offset_x = (target_size - rendered_width) / 2;
    let offset_y = (target_size - rendered_height) / 2;
    let mut rendered = IconBitmap::transparent(target_size, target_size)?;

    for target_y in 0..rendered_height {
        let source_y = ((f64::from(target_y) / scale).floor() as u32).min(source.height - 1);
        for target_x in 0..rendered_width {
            let source_x = ((f64::from(target_x) / scale).floor() as u32).min(source.width - 1);
            let source_offset = pixel_offset(source.width, source_x, source_y)?;
            let target_offset =
                pixel_offset(target_size, offset_x + target_x, offset_y + target_y)?;
            rendered.bgra[target_offset..target_offset + 4]
                .copy_from_slice(&source.bgra[source_offset..source_offset + 4]);
        }
    }

    Ok(rendered)
}

pub fn validate_icon_size(size: u32) -> Result<u32> {
    if (1..=MAX_ICON_SIZE).contains(&size) {
        Ok(size)
    } else {
        Err(super::platform_error(format!(
            "icon size must be between 1 and {MAX_ICON_SIZE}: {size}"
        )))
    }
}

pub fn normalize_icon_cache_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_lowercase()
}

#[cfg(windows)]
fn extract_file_icon_bitmap_impl(path: &Path, size: u32) -> Result<Option<IconBitmap>> {
    let platform = WindowsShellIconPlatform;
    extract_file_icon_bitmap_with_platform(&platform, path, size)
}

#[cfg(not(windows))]
fn extract_file_icon_bitmap_impl(_path: &Path, _size: u32) -> Result<Option<IconBitmap>> {
    Err(super::unsupported_platform())
}

trait ShellIconPlatform {
    type RawIcon: Copy + Eq;

    fn path_exists(&self, path: &Path) -> bool;
    fn sh_get_file_info_icon(&self, path: &Path) -> Result<Option<Self::RawIcon>>;
    fn icon_to_bitmap(&self, icon: Self::RawIcon, size: u32) -> Result<IconBitmap>;
    fn destroy_icon(&self, icon: Self::RawIcon);
    fn default_icon_bitmap(&self, size: u32) -> Result<Option<IconBitmap>>;
}

fn extract_file_icon_bitmap_with_platform<P>(
    platform: &P,
    path: &Path,
    size: u32,
) -> Result<Option<IconBitmap>>
where
    P: ShellIconPlatform,
{
    let size = validate_icon_size(size)?;
    if path.as_os_str().is_empty() {
        return Err(super::platform_error("path is empty"));
    }
    if !platform.path_exists(path) {
        return Ok(None);
    }

    let Some(raw_icon) = platform.sh_get_file_info_icon(path)? else {
        return Ok(None);
    };
    let _guard = IconDestroyGuard::new(platform, raw_icon);
    let bitmap = platform.icon_to_bitmap(raw_icon, size)?;
    if let Some(default) = platform.default_icon_bitmap(size)?
        && default == bitmap
    {
        return Ok(None);
    }

    Ok(Some(bitmap))
}

struct IconDestroyGuard<'a, P>
where
    P: ShellIconPlatform,
{
    platform: &'a P,
    icon: Option<P::RawIcon>,
}

impl<'a, P> IconDestroyGuard<'a, P>
where
    P: ShellIconPlatform,
{
    fn new(platform: &'a P, icon: P::RawIcon) -> Self {
        Self {
            platform,
            icon: Some(icon),
        }
    }
}

impl<P> Drop for IconDestroyGuard<'_, P>
where
    P: ShellIconPlatform,
{
    fn drop(&mut self) {
        if let Some(icon) = self.icon.take() {
            self.platform.destroy_icon(icon);
        }
    }
}

#[cfg(windows)]
struct WindowsShellIconPlatform;

#[cfg(windows)]
impl ShellIconPlatform for WindowsShellIconPlatform {
    type RawIcon = windows_sys::Win32::UI::WindowsAndMessaging::HICON;

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn sh_get_file_info_icon(&self, path: &Path) -> Result<Option<Self::RawIcon>> {
        sh_get_file_info_hicon(path)
    }

    fn icon_to_bitmap(&self, icon: Self::RawIcon, size: u32) -> Result<IconBitmap> {
        hicon_to_bgra_bitmap(icon, size)
    }

    fn destroy_icon(&self, icon: Self::RawIcon) {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

        // Safety: SHGetFileInfoW returned this owned icon handle. This guard is
        // the single owner on every conversion path.
        let _ = unsafe { DestroyIcon(icon) };
    }

    fn default_icon_bitmap(&self, size: u32) -> Result<Option<IconBitmap>> {
        use std::ptr::null_mut;

        use windows_sys::Win32::UI::WindowsAndMessaging::{IDI_APPLICATION, LoadIconW};

        // Safety: IDI_APPLICATION is a system resource identifier. LoadIconW
        // returns a shared handle here, so it must not be destroyed by us.
        let icon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
        if icon.is_null() {
            Ok(None)
        } else {
            hicon_to_bgra_bitmap(icon, size).map(Some)
        }
    }
}

#[cfg(windows)]
fn extract_file_hicon_impl(path: &Path) -> Result<Option<ExtractedIcon>> {
    if path.as_os_str().is_empty() {
        return Err(super::platform_error("path is empty"));
    }
    if !path.exists() {
        return Ok(None);
    }

    Ok(sh_get_file_info_hicon(path)?
        .and_then(|hicon| OwnedIcon::from_hicon(hicon).map(|icon| ExtractedIcon { icon })))
}

#[cfg(windows)]
fn sh_get_file_info_hicon(
    path: &Path,
) -> Result<Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON>> {
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};

    let _com = ComGuard::initialize()?;
    let path_wide = super::wide::path_to_wide_z(path)?;
    let mut info = SHFILEINFOW::default();

    // Safety: SetLastError only mutates this thread's last-error slot.
    unsafe { SetLastError(0) };
    // Safety: path_wide is a validated NUL-terminated UTF-16 path, info points
    // to a valid SHFILEINFOW buffer, and cbFileInfo matches the buffer size.
    let result = unsafe {
        SHGetFileInfoW(
            path_wide.as_ptr(),
            0,
            &mut info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    // Safety: GetLastError reads this thread's last-error slot.
    let last_error = unsafe { GetLastError() };
    if result == 0 {
        return Err(super::platform_error(format!(
            "SHGetFileInfoW failed (last_error={last_error})"
        )));
    }

    if info.hIcon.is_null() {
        Ok(None)
    } else {
        Ok(Some(info.hIcon))
    }
}

#[cfg(not(windows))]
fn extract_file_hicon_impl(_path: &Path) -> Result<Option<ExtractedIcon>> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
fn icon_to_bgra_bitmap_impl(icon: &OwnedIcon, size: u32) -> Result<IconBitmap> {
    hicon_to_bgra_bitmap(icon.as_hicon(), size)
}

#[cfg(windows)]
fn hicon_to_bgra_bitmap(
    hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    size: u32,
) -> Result<IconBitmap> {
    use std::ffi::c_void;
    use std::ptr::{null_mut, write_bytes};

    use windows_sys::Win32::Graphics::Gdi::HGDIOBJ;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx};

    let size = validate_icon_size(size)?;
    let size_i32 = i32::try_from(size)
        .map_err(|_| super::platform_error(format!("icon size is too large: {size}")))?;
    let byte_len = bitmap_byte_len(size, size)?;
    let screen_dc = ScreenDc::acquire()?;
    let memory_dc = MemoryDc::create(screen_dc.hdc)?;
    let mut bits: *mut c_void = null_mut();
    let bitmap = DibSection::create(screen_dc.hdc, size_i32, size_i32, &mut bits)?;
    let _selection = SelectedObject::select(memory_dc.hdc, bitmap.handle as HGDIOBJ)?;

    // Safety: bits points to the DIB section memory for byte_len bytes while
    // bitmap is alive. The memory is owned by GDI and writable through this pointer.
    unsafe { write_bytes(bits.cast::<u8>(), 0, byte_len) };
    // Safety: memory_dc and icon are valid handles, dimensions are positive, and
    // the selected DIB section remains alive for the draw call.
    let drawn = unsafe {
        DrawIconEx(
            memory_dc.hdc,
            0,
            0,
            hicon,
            size_i32,
            size_i32,
            0,
            null_mut(),
            DI_NORMAL,
        )
    };
    if drawn == 0 {
        return Err(super::platform_error("DrawIconEx failed"));
    }

    // Safety: bits still points to byte_len initialized bytes in the selected
    // DIB section. The slice is copied immediately before GDI resources drop.
    let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), byte_len).to_vec() };
    IconBitmap::new(size, size, bgra)
}

#[cfg(not(windows))]
fn icon_to_bgra_bitmap_impl(_icon: &OwnedIcon, _size: u32) -> Result<IconBitmap> {
    Err(super::unsupported_platform())
}

#[cfg(windows)]
fn create_icon_from_bitmap_impl(bitmap: &IconBitmap) -> Result<ExtractedIcon> {
    use std::ffi::c_void;
    use std::ptr::{copy_nonoverlapping, null_mut};

    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

    validate_icon_size(bitmap.width)?;
    validate_icon_size(bitmap.height)?;
    let expected_len = bitmap_byte_len(bitmap.width, bitmap.height)?;
    if bitmap.bgra.len() != expected_len {
        return Err(super::platform_error(format!(
            "icon bitmap buffer length mismatch: expected {expected_len}, got {}",
            bitmap.bgra.len()
        )));
    }

    let width = i32::try_from(bitmap.width)
        .map_err(|_| super::platform_error(format!("icon width is too large: {}", bitmap.width)))?;
    let height = i32::try_from(bitmap.height).map_err(|_| {
        super::platform_error(format!("icon height is too large: {}", bitmap.height))
    })?;
    let screen_dc = ScreenDc::acquire()?;
    let mut bits: *mut c_void = null_mut();
    let color_bitmap = DibSection::create(screen_dc.hdc, width, height, &mut bits)?;
    let mask_bitmap = OwnedBitmap::create_mask(bitmap.width, bitmap.height)?;

    // Safety: bits points to the DIB section memory for expected_len bytes, and
    // bitmap.bgra has already been validated to exactly that length.
    unsafe { copy_nonoverlapping(bitmap.bgra.as_ptr(), bits.cast::<u8>(), expected_len) };

    let icon_info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bitmap.handle,
        hbmColor: color_bitmap.handle,
    };
    // Safety: icon_info references valid bitmap handles for the duration of the
    // call. CreateIconIndirect copies the bitmap data into the returned icon.
    let hicon = unsafe { CreateIconIndirect(&icon_info) };
    if hicon.is_null() {
        return Err(super::platform_error("CreateIconIndirect failed"));
    }

    OwnedIcon::from_hicon(hicon)
        .map(|icon| ExtractedIcon { icon })
        .ok_or_else(|| super::platform_error("CreateIconIndirect returned a null icon"))
}

#[cfg(not(windows))]
fn create_icon_from_bitmap_impl(_bitmap: &IconBitmap) -> Result<ExtractedIcon> {
    Err(super::unsupported_platform())
}

fn bitmap_byte_len(width: u32, height: u32) -> Result<usize> {
    let bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| super::platform_error("icon bitmap dimensions overflow"))?;
    usize::try_from(bytes).map_err(|_| super::platform_error("icon bitmap is too large"))
}

fn pixel_offset(width: u32, x: u32, y: u32) -> Result<usize> {
    if x >= width {
        return Err(super::platform_error(
            "icon bitmap x coordinate is out of range",
        ));
    }
    let pixel_index = y
        .checked_mul(width)
        .and_then(|row| row.checked_add(x))
        .ok_or_else(|| super::platform_error("icon bitmap pixel offset overflow"))?;
    usize::try_from(
        pixel_index
            .checked_mul(4)
            .ok_or_else(|| super::platform_error("icon bitmap byte offset overflow"))?,
    )
    .map_err(|_| super::platform_error("icon bitmap byte offset is too large"))
}

#[cfg(windows)]
struct ComGuard {
    initialized: bool,
}

#[cfg(windows)]
impl ComGuard {
    fn initialize() -> Result<Self> {
        use std::ptr::null;

        use windows_sys::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
        use windows_sys::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
        };

        let flags = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32;
        // Safety: CoInitializeEx receives a null reserved pointer as required by
        // Win32 and initializes COM for the current thread only.
        let hr = unsafe { CoInitializeEx(null(), flags) };
        if hr == S_OK || hr == S_FALSE {
            Ok(Self { initialized: true })
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(Self { initialized: false })
        } else {
            Err(super::platform_error(format!(
                "CoInitializeEx failed (hr=0x{:08X})",
                hr as u32
            )))
        }
    }
}

#[cfg(windows)]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            use windows_sys::Win32::System::Com::CoUninitialize;

            // Safety: this guard calls CoUninitialize only for a successful
            // CoInitializeEx on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
struct ScreenDc {
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
}

#[cfg(windows)]
impl ScreenDc {
    fn acquire() -> Result<Self> {
        use std::ptr::null_mut;

        use windows_sys::Win32::Graphics::Gdi::GetDC;

        // Safety: GetDC with a null HWND requests the screen DC and returns a
        // handle that must be released with ReleaseDC.
        let hdc = unsafe { GetDC(null_mut()) };
        if hdc.is_null() {
            Err(super::platform_error("GetDC failed"))
        } else {
            Ok(Self { hdc })
        }
    }
}

#[cfg(windows)]
impl Drop for ScreenDc {
    fn drop(&mut self) {
        use std::ptr::null_mut;

        use windows_sys::Win32::Graphics::Gdi::ReleaseDC;

        // Safety: hdc was acquired by GetDC(NULL) in ScreenDc::acquire and is
        // released once here.
        let _ = unsafe { ReleaseDC(null_mut(), self.hdc) };
    }
}

#[cfg(windows)]
struct MemoryDc {
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
}

#[cfg(windows)]
impl MemoryDc {
    fn create(reference: windows_sys::Win32::Graphics::Gdi::HDC) -> Result<Self> {
        use windows_sys::Win32::Graphics::Gdi::CreateCompatibleDC;

        // Safety: reference is a valid screen DC. The returned memory DC is
        // owned by this wrapper and released by DeleteDC.
        let hdc = unsafe { CreateCompatibleDC(reference) };
        if hdc.is_null() {
            Err(super::platform_error("CreateCompatibleDC failed"))
        } else {
            Ok(Self { hdc })
        }
    }
}

#[cfg(windows)]
impl Drop for MemoryDc {
    fn drop(&mut self) {
        use windows_sys::Win32::Graphics::Gdi::DeleteDC;

        // Safety: hdc was created by CreateCompatibleDC and is deleted once here.
        let _ = unsafe { DeleteDC(self.hdc) };
    }
}

#[cfg(windows)]
struct DibSection {
    handle: windows_sys::Win32::Graphics::Gdi::HBITMAP,
}

#[cfg(windows)]
impl DibSection {
    fn create(
        reference: windows_sys::Win32::Graphics::Gdi::HDC,
        width: i32,
        height: i32,
        bits: &mut *mut std::ffi::c_void,
    ) -> Result<Self> {
        use std::ptr::null_mut;

        use windows_sys::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, CreateDIBSection, DIB_RGB_COLORS,
        };

        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize =
            std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;

        // Safety: info is a valid BITMAPINFO, bits is an out-pointer to receive
        // DIB memory, and the returned bitmap is owned by this wrapper.
        let handle =
            unsafe { CreateDIBSection(reference, &info, DIB_RGB_COLORS, bits, null_mut(), 0) };
        if handle.is_null() || (*bits).is_null() {
            Err(super::platform_error("CreateDIBSection failed"))
        } else {
            Ok(Self { handle })
        }
    }
}

#[cfg(windows)]
impl Drop for DibSection {
    fn drop(&mut self) {
        use windows_sys::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};

        // Safety: handle was created by CreateDIBSection and is deleted once
        // after it has been restored out of any DC selection.
        let _ = unsafe { DeleteObject(self.handle as HGDIOBJ) };
    }
}

#[cfg(windows)]
struct OwnedBitmap {
    handle: windows_sys::Win32::Graphics::Gdi::HBITMAP,
}

#[cfg(windows)]
impl OwnedBitmap {
    fn create_mask(width: u32, height: u32) -> Result<Self> {
        use windows_sys::Win32::Graphics::Gdi::CreateBitmap;

        let width_i32 = i32::try_from(width)
            .map_err(|_| super::platform_error(format!("mask width is too large: {width}")))?;
        let height_i32 = i32::try_from(height)
            .map_err(|_| super::platform_error(format!("mask height is too large: {height}")))?;
        let stride_bytes = width
            .div_ceil(16)
            .checked_mul(2)
            .and_then(|stride| stride.checked_mul(height))
            .ok_or_else(|| super::platform_error("icon mask bitmap dimensions overflow"))?;
        let mask = vec![
            0u8;
            usize::try_from(stride_bytes).map_err(|_| {
                super::platform_error("icon mask bitmap is too large")
            })?
        ];

        // Safety: mask points to a zeroed 1bpp bitmap buffer sized for the
        // requested dimensions. The returned HBITMAP is owned by this wrapper.
        let handle = unsafe { CreateBitmap(width_i32, height_i32, 1, 1, mask.as_ptr().cast()) };
        if handle.is_null() {
            Err(super::platform_error("CreateBitmap failed for icon mask"))
        } else {
            Ok(Self { handle })
        }
    }
}

#[cfg(windows)]
impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        use windows_sys::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};

        // Safety: handle was created by CreateBitmap and is deleted once here.
        let _ = unsafe { DeleteObject(self.handle as HGDIOBJ) };
    }
}

#[cfg(windows)]
struct SelectedObject {
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    previous: windows_sys::Win32::Graphics::Gdi::HGDIOBJ,
}

#[cfg(windows)]
impl SelectedObject {
    fn select(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        object: windows_sys::Win32::Graphics::Gdi::HGDIOBJ,
    ) -> Result<Self> {
        use windows_sys::Win32::Graphics::Gdi::SelectObject;

        // Safety: hdc is a valid memory DC and object is a valid bitmap handle.
        // The previous object is stored and restored in Drop.
        let previous = unsafe { SelectObject(hdc, object) };
        if previous.is_null() {
            Err(super::platform_error("SelectObject failed"))
        } else {
            Ok(Self { hdc, previous })
        }
    }
}

#[cfg(windows)]
impl Drop for SelectedObject {
    fn drop(&mut self) {
        use windows_sys::Win32::Graphics::Gdi::SelectObject;

        // Safety: previous was returned by SelectObject for this hdc and is
        // restored before the selected bitmap is deleted.
        let _ = unsafe { SelectObject(self.hdc, self.previous) };
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::io;
    use std::process;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug)]
    struct TestFile {
        path: PathBuf,
    }

    impl TestFile {
        fn new(label: &str) -> io::Result<Self> {
            let sequence = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "j3launcher-icon-{label}-{}-{sequence}.tmp",
                process::id()
            ));
            fs::write(&path, b"icon-test")?;
            Ok(Self { path })
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let temp_dir = std::env::temp_dir();
            if self.path.starts_with(&temp_dir) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    #[derive(Debug)]
    struct MockExtractor {
        calls: Arc<AtomicUsize>,
        bitmap: IconBitmap,
    }

    impl MockExtractor {
        fn new(calls: Arc<AtomicUsize>) -> Self {
            Self {
                calls,
                bitmap: one_pixel_bitmap([1, 2, 3, 4]),
            }
        }
    }

    struct BlockingExtractor {
        calls: Arc<AtomicUsize>,
        started_tx: mpsc::Sender<()>,
        release_rx: mpsc::Receiver<()>,
        bitmap: IconBitmap,
    }

    impl IconExtractor for BlockingExtractor {
        fn extract_icon_bitmap(&mut self, _path: &Path, _size: u32) -> Result<Option<IconBitmap>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = self.started_tx.send(());
            let _ = self.release_rx.recv_timeout(Duration::from_secs(2));
            Ok(Some(self.bitmap.clone()))
        }
    }

    impl IconExtractor for MockExtractor {
        fn extract_icon_bitmap(&mut self, _path: &Path, _size: u32) -> Result<Option<IconBitmap>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(self.bitmap.clone()))
        }
    }

    struct MockShellIconPlatform {
        exists: bool,
        icon: Option<usize>,
        convert_error: bool,
        default_bitmap: Option<IconBitmap>,
        default_error: bool,
        destroy_calls: RefCell<Vec<usize>>,
        sh_calls: Cell<usize>,
    }

    impl MockShellIconPlatform {
        fn success(icon: usize) -> Self {
            Self {
                exists: true,
                icon: Some(icon),
                convert_error: false,
                default_bitmap: None,
                default_error: false,
                destroy_calls: RefCell::new(Vec::new()),
                sh_calls: Cell::new(0),
            }
        }
    }

    impl ShellIconPlatform for MockShellIconPlatform {
        type RawIcon = usize;

        fn path_exists(&self, _path: &Path) -> bool {
            self.exists
        }

        fn sh_get_file_info_icon(&self, _path: &Path) -> Result<Option<Self::RawIcon>> {
            self.sh_calls.set(self.sh_calls.get() + 1);
            Ok(self.icon)
        }

        fn icon_to_bitmap(&self, _icon: Self::RawIcon, _size: u32) -> Result<IconBitmap> {
            if self.convert_error {
                Err(super::super::platform_error("mock conversion failed"))
            } else {
                Ok(one_pixel_bitmap([9, 8, 7, 6]))
            }
        }

        fn destroy_icon(&self, icon: Self::RawIcon) {
            self.destroy_calls.borrow_mut().push(icon);
        }

        fn default_icon_bitmap(&self, _size: u32) -> Result<Option<IconBitmap>> {
            if self.default_error {
                Err(super::super::platform_error("mock default icon failed"))
            } else {
                Ok(self.default_bitmap.clone())
            }
        }
    }

    fn one_pixel_bitmap(pixel: [u8; 4]) -> IconBitmap {
        IconBitmap::new(1, 1, pixel.to_vec()).expect("valid test bitmap")
    }

    fn request_for(path: PathBuf, ordinal: u64) -> ButtonIconRequest {
        let key = ButtonIconKey::new("tab", format!("button-{ordinal}"), "path", 16);
        ButtonIconRequest::new(ordinal, key, path, 16)
    }

    #[test]
    fn validate_icon_size_accepts_supported_range() {
        assert_eq!(validate_icon_size(1).ok(), Some(1));
        assert_eq!(
            validate_icon_size(DEFAULT_ICON_SIZE).ok(),
            Some(DEFAULT_ICON_SIZE)
        );
        assert_eq!(validate_icon_size(MAX_ICON_SIZE).ok(), Some(MAX_ICON_SIZE));
    }

    #[test]
    fn validate_icon_size_rejects_invalid_values() {
        assert!(validate_icon_size(0).is_err());
        assert!(validate_icon_size(MAX_ICON_SIZE + 1).is_err());
    }

    #[test]
    fn icon_bitmap_validates_buffer_length() {
        let bitmap = IconBitmap::new(2, 2, vec![0; 16]).expect("valid buffer");
        assert_eq!(bitmap.bgra.len(), 16);

        let error = IconBitmap::new(2, 2, vec![0; 15]).expect_err("invalid buffer");
        assert!(error.to_string().contains("buffer length mismatch"));
    }

    #[test]
    fn lru_cache_reports_hits_and_misses() {
        let mut cache = LruCache::new(2);
        let key = String::from("a");

        assert!(cache.get(&key).is_none());
        cache.insert(key.clone(), 10);

        assert_eq!(cache.get(&key), Some(&10));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn lru_cache_evicts_least_recently_used_entry() {
        let mut cache = LruCache::new(2);
        let a = String::from("a");
        let b = String::from("b");
        let c = String::from("c");
        cache.insert(a.clone(), 1);
        cache.insert(b.clone(), 2);

        assert_eq!(cache.get(&a), Some(&1));
        cache.insert(c.clone(), 3);

        assert!(cache.contains_key(&a));
        assert!(!cache.contains_key(&b));
        assert!(cache.contains_key(&c));
    }

    #[test]
    fn worker_uses_cache_for_repeated_path() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let file = TestFile::new("cache")?;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut worker = spawn_button_icon_worker_with_extractor(
            MockExtractor::new(Arc::clone(&calls)),
            8,
            || {},
        )?;

        assert!(worker.request(request_for(file.path.clone(), 1)));
        let first = worker
            .recv_timeout(Duration::from_secs(2))
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "first icon result"))?;
        assert!(!first.cache_hit);
        assert!(first.bitmap.is_some());

        assert!(worker.request(request_for(file.path.clone(), 2)));
        let second = worker
            .recv_timeout(Duration::from_secs(2))
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "second icon result"))?;
        assert!(second.cache_hit);
        assert!(second.bitmap.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(worker.shutdown());
        Ok(())
    }

    #[test]
    fn worker_shutdown_stops_accepting_requests()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut worker = spawn_button_icon_worker_with_extractor(
            MockExtractor::new(Arc::clone(&calls)),
            8,
            || {},
        )?;

        assert!(worker.shutdown());
        assert!(!worker.request(request_for(PathBuf::from("missing.exe"), 1)));
        Ok(())
    }

    #[test]
    fn worker_shutdown_without_join_returns_while_request_is_processing()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let file = TestFile::new("shutdown-detach")?;
        let calls = Arc::new(AtomicUsize::new(0));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = spawn_button_icon_worker_with_extractor(
            BlockingExtractor {
                calls: Arc::clone(&calls),
                started_tx,
                release_rx,
                bitmap: one_pixel_bitmap([1, 2, 3, 4]),
            },
            8,
            || {},
        )?;

        assert!(worker.request(request_for(file.path.clone(), 1)));
        started_rx.recv_timeout(Duration::from_secs(2))?;
        let (returned_tx, returned_rx) = mpsc::channel();
        let shutdown_thread = thread::spawn(move || {
            worker.shutdown_without_join();
            let _ = returned_tx.send(());
        });

        let returned_before_release = returned_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        let _ = release_tx.send(());
        shutdown_thread
            .join()
            .map_err(|_| io::Error::other("shutdown thread panicked"))?;

        assert!(returned_before_release);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn worker_rejects_requests_when_bounded_queue_is_full()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let file = TestFile::new("bounded-queue")?;
        let calls = Arc::new(AtomicUsize::new(0));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = spawn_button_icon_worker_with_extractor(
            BlockingExtractor {
                calls: Arc::clone(&calls),
                started_tx,
                release_rx,
                bitmap: one_pixel_bitmap([1, 2, 3, 4]),
            },
            1,
            || {},
        )?;

        assert!(worker.request(request_for(file.path.clone(), 1)));
        started_rx.recv_timeout(Duration::from_secs(2))?;
        assert!(worker.request(request_for(file.path.clone(), 2)));
        assert!(!worker.request(request_for(file.path.clone(), 3)));

        let shutdown_thread = thread::spawn(move || {
            let mut worker = worker;
            worker.shutdown()
        });
        release_tx.send(())?;

        assert!(
            shutdown_thread
                .join()
                .map_err(|_| io::Error::other("shutdown thread panicked"))?
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn worker_shutdown_does_not_drain_queued_backlog()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let file = TestFile::new("shutdown-backlog")?;
        let calls = Arc::new(AtomicUsize::new(0));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = spawn_button_icon_worker_with_extractor(
            BlockingExtractor {
                calls: Arc::clone(&calls),
                started_tx,
                release_rx,
                bitmap: one_pixel_bitmap([1, 2, 3, 4]),
            },
            8,
            || {},
        )?;

        assert!(worker.request(request_for(file.path.clone(), 1)));
        started_rx.recv_timeout(Duration::from_secs(2))?;
        assert!(worker.request(request_for(file.path.clone(), 2)));

        let shutdown_thread = thread::spawn(move || {
            let mut worker = worker;
            worker.shutdown()
        });
        release_tx.send(())?;

        assert!(
            shutdown_thread
                .join()
                .map_err(|_| io::Error::other("shutdown thread panicked"))?
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn worker_handles_invalid_path_without_extracting()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker = spawn_button_icon_worker_with_extractor(
            MockExtractor::new(Arc::clone(&calls)),
            8,
            || {},
        )?;
        let missing =
            std::env::temp_dir().join(format!("j3launcher-missing-icon-{}", process::id()));

        assert!(worker.request(request_for(missing, 1)));
        let result = worker
            .recv_timeout(Duration::from_secs(2))
            .expect("invalid path result");

        assert!(result.bitmap.is_none());
        assert!(result.error.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn worker_trusts_verified_existing_path_before_extracting() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut extractor = MockExtractor::new(Arc::clone(&calls));
        let mut cache = LruCache::new(8);
        let missing = std::env::temp_dir().join(format!(
            "j3launcher-verified-icon-{}-{}",
            process::id(),
            TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let request = request_for(missing, 1).with_verified_existing_path();

        let result = process_icon_worker_request(&mut extractor, &mut cache, request);

        assert!(result.bitmap.is_some());
        assert!(result.error.is_none());
        assert!(!result.cache_hit);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn platform_destroy_icon_runs_after_successful_conversion() {
        let platform = MockShellIconPlatform::success(77);

        let result =
            extract_file_icon_bitmap_with_platform(&platform, Path::new("C:\\tool.exe"), 1)
                .expect("mock extraction");

        assert!(result.is_some());
        assert_eq!(platform.destroy_calls.borrow().as_slice(), &[77]);
        assert_eq!(platform.sh_calls.get(), 1);
    }

    #[test]
    fn platform_destroy_icon_runs_after_failed_conversion() {
        let mut platform = MockShellIconPlatform::success(88);
        platform.convert_error = true;

        let result =
            extract_file_icon_bitmap_with_platform(&platform, Path::new("C:\\tool.exe"), 1);

        assert!(result.is_err());
        assert_eq!(platform.destroy_calls.borrow().as_slice(), &[88]);
    }

    #[test]
    fn platform_destroy_icon_runs_when_default_icon_is_suppressed() {
        let mut platform = MockShellIconPlatform::success(99);
        platform.default_bitmap = Some(one_pixel_bitmap([9, 8, 7, 6]));

        let result =
            extract_file_icon_bitmap_with_platform(&platform, Path::new("C:\\tool.exe"), 1)
                .expect("mock extraction");

        assert!(result.is_none());
        assert_eq!(platform.destroy_calls.borrow().as_slice(), &[99]);
    }

    #[test]
    fn platform_destroy_icon_runs_after_default_icon_comparison_error() {
        let mut platform = MockShellIconPlatform::success(100);
        platform.default_error = true;

        let result =
            extract_file_icon_bitmap_with_platform(&platform, Path::new("C:\\tool.exe"), 1);

        assert!(result.is_err());
        assert_eq!(platform.destroy_calls.borrow().as_slice(), &[100]);
    }
}
