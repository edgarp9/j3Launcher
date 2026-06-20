use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use crate::LauncherError;
use crate::domain::LauncherButton;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMessage {
    pub level: String,
    pub title: String,
    pub message: String,
}

impl UserMessage {
    pub fn new(
        level: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level: level.into(),
            title: title.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonActionKind {
    Noop,
    Launch,
    CopyToClipboard,
}

impl ButtonActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Noop => "noop",
            Self::Launch => "launch",
            Self::CopyToClipboard => "copy_to_clipboard",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonActionRequest {
    pub kind: ButtonActionKind,
    pub path: String,
    pub params: String,
    pub admin: bool,
    pub command: String,
    pub pre_messages: Vec<UserMessage>,
}

impl ButtonActionRequest {
    pub fn noop() -> Self {
        Self::default()
    }

    pub fn noop_with_messages(pre_messages: Vec<UserMessage>) -> Self {
        Self {
            pre_messages,
            ..Self::default()
        }
    }

    pub fn launch(
        path: impl Into<String>,
        params: impl Into<String>,
        admin: bool,
        pre_messages: Vec<UserMessage>,
    ) -> Self {
        Self {
            kind: ButtonActionKind::Launch,
            path: path.into(),
            params: params.into(),
            admin,
            command: String::new(),
            pre_messages,
        }
    }

    pub fn copy_to_clipboard(command: impl Into<String>) -> Self {
        Self {
            kind: ButtonActionKind::CopyToClipboard,
            path: String::new(),
            params: String::new(),
            admin: false,
            command: command.into(),
            pre_messages: Vec::new(),
        }
    }

    pub fn is_noop(&self) -> bool {
        self.kind == ButtonActionKind::Noop
    }
}

impl Default for ButtonActionRequest {
    fn default() -> Self {
        Self {
            kind: ButtonActionKind::Noop,
            path: String::new(),
            params: String::new(),
            admin: false,
            command: String::new(),
            pre_messages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ButtonActionInput {
    pub name: String,
    pub path: String,
    pub params: String,
    pub admin: bool,
    pub action: u8,
    pub auto_enter: bool,
}

impl From<&LauncherButton> for ButtonActionInput {
    fn from(button: &LauncherButton) -> Self {
        Self {
            name: button.name.clone(),
            path: button.path.clone(),
            params: button.params.clone(),
            admin: button.admin,
            action: button.action,
            auto_enter: button.auto_enter,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminLaunchStatus {
    Success,
    Cancelled,
    Failed,
    Exception,
}

impl AdminLaunchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Exception => "exception",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminLaunchResult {
    pub status: AdminLaunchStatus,
    pub detail: String,
    pub code: isize,
    pub last_error: u32,
    pub executable: String,
}

impl AdminLaunchResult {
    pub fn success(code: isize, executable: impl Into<String>) -> Self {
        Self {
            status: AdminLaunchStatus::Success,
            detail: String::new(),
            code,
            last_error: 0,
            executable: executable.into(),
        }
    }

    pub fn cancelled(
        detail: impl Into<String>,
        code: isize,
        last_error: u32,
        executable: impl Into<String>,
    ) -> Self {
        Self {
            status: AdminLaunchStatus::Cancelled,
            detail: detail.into(),
            code,
            last_error,
            executable: executable.into(),
        }
    }

    pub fn failed(
        detail: impl Into<String>,
        code: isize,
        last_error: u32,
        executable: impl Into<String>,
    ) -> Self {
        Self {
            status: AdminLaunchStatus::Failed,
            detail: detail.into(),
            code,
            last_error,
            executable: executable.into(),
        }
    }

    pub fn exception(detail: impl Into<String>, executable: impl Into<String>) -> Self {
        Self {
            status: AdminLaunchStatus::Exception,
            detail: detail.into(),
            code: 0,
            last_error: 0,
            executable: executable.into(),
        }
    }

    pub fn ok(&self) -> bool {
        self.status == AdminLaunchStatus::Success
    }

    pub fn cancelled_status(&self) -> bool {
        self.status == AdminLaunchStatus::Cancelled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerOpenFeedback {
    pub level: String,
    pub message: String,
}

impl ExplorerOpenFeedback {
    pub fn new(level: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: level.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionFailureKind {
    NotFound,
    PermissionDenied,
    InvalidInput,
    IsDirectory,
    RuntimeUnavailable,
    Platform,
    Unexpected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionFailure {
    pub kind: ActionFailureKind,
    pub detail: String,
}

impl ActionFailure {
    pub fn new(kind: ActionFailureKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(ActionFailureKind::NotFound, detail)
    }

    pub fn permission_denied(detail: impl Into<String>) -> Self {
        Self::new(ActionFailureKind::PermissionDenied, detail)
    }

    pub fn invalid_input(detail: impl Into<String>) -> Self {
        Self::new(ActionFailureKind::InvalidInput, detail)
    }

    pub fn is_directory(detail: impl Into<String>) -> Self {
        Self::new(ActionFailureKind::IsDirectory, detail)
    }

    pub fn runtime_unavailable(detail: impl Into<String>) -> Self {
        Self::new(ActionFailureKind::RuntimeUnavailable, detail)
    }

    pub fn platform(detail: impl Into<String>) -> Self {
        Self::new(ActionFailureKind::Platform, detail)
    }
}

impl Display for ActionFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for ActionFailure {}

impl From<LauncherError> for ActionFailure {
    fn from(error: LauncherError) -> Self {
        match error {
            LauncherError::UnsupportedPlatform { platform } => Self::permission_denied(format!(
                "operation is not supported on platform: {platform}"
            )),
            LauncherError::Platform { message } => action_failure_from_platform_message(message),
            other => Self::new(ActionFailureKind::Unexpected, other.to_string()),
        }
    }
}

fn action_failure_from_platform_message(message: String) -> ActionFailure {
    let lower = message.to_ascii_lowercase();
    if lower.contains("path is empty") || lower.contains("empty") {
        return ActionFailure::invalid_input(message);
    }
    if lower.contains("directories cannot") {
        return ActionFailure::is_directory(message);
    }
    if platform_message_contains_any(
        &message,
        &[
            "SE_ERR_FNF",
            "SE_ERR_PNF",
            "SE_ERR_DLLNOTFOUND",
            "ERROR_FILE_NOT_FOUND",
            "ERROR_PATH_NOT_FOUND",
        ],
    ) {
        return ActionFailure::not_found(message);
    }
    if platform_message_contains_any(&message, &["SE_ERR_ACCESSDENIED", "ERROR_ACCESS_DENIED"]) {
        return ActionFailure::permission_denied(message);
    }
    if platform_message_contains_any(&message, &["ERROR_INVALID_PARAMETER"]) {
        return ActionFailure::invalid_input(message);
    }
    if platform_message_contains_any(
        &message,
        &[
            "SE_ERR_NOASSOC",
            "SE_ERR_ASSOCINCOMPLETE",
            "ERROR_MOD_NOT_FOUND",
            "ERROR_PROC_NOT_FOUND",
        ],
    ) {
        return ActionFailure::runtime_unavailable(message);
    }
    ActionFailure::platform(message)
}

fn platform_message_contains_any(message: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| message.contains(needle))
}

pub type ActionResult<T> = std::result::Result<T, ActionFailure>;

pub trait LauncherPlatform {
    fn supports_native_admin(&self) -> bool;

    fn expand_path(&self, value: &str) -> String {
        value.to_owned()
    }

    fn normalize_path(&self, value: &str) -> String {
        value.to_owned()
    }

    fn is_linux(&self) -> bool {
        false
    }

    fn has_windows_path_syntax(&self, _value: &str) -> bool {
        false
    }

    fn launch(&self, path: &str, params: &str) -> ActionResult<()>;
    fn run_as_admin(&self, path: &str, params: &str) -> ActionResult<AdminLaunchResult>;
    fn open_in_explorer(&self, raw_path: &str) -> ActionResult<Option<ExplorerOpenFeedback>>;
    fn copy_to_clipboard(&self, text: &str) -> ActionResult<()>;
}

#[derive(Debug, Clone, Default)]
pub struct SystemLauncherPlatform {
    base_dir: Option<PathBuf>,
}

impl SystemLauncherPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: Some(base_dir.into()),
        }
    }

    fn runtime_path(&self, value: &str) -> String {
        resolve_runtime_path(value, self.base_dir.as_deref())
    }
}

impl LauncherPlatform for SystemLauncherPlatform {
    fn supports_native_admin(&self) -> bool {
        cfg!(windows)
    }

    fn expand_path(&self, value: &str) -> String {
        expand_environment_variables(value)
    }

    fn normalize_path(&self, value: &str) -> String {
        PathBuf::from(value).to_string_lossy().into_owned()
    }

    fn is_linux(&self) -> bool {
        cfg!(target_os = "linux")
    }

    fn has_windows_path_syntax(&self, value: &str) -> bool {
        has_windows_path_syntax(value)
    }

    fn launch(&self, path: &str, params: &str) -> ActionResult<()> {
        let path = self.runtime_path(path);
        if path.is_empty() {
            return Err(ActionFailure::invalid_input("program path is empty"));
        }

        let target = Path::new(&path);
        let params = params.trim();
        if target.is_dir() && !params.is_empty() {
            return Err(ActionFailure::invalid_input(
                "directory launch does not support additional parameters",
            ));
        }

        #[cfg(windows)]
        {
            crate::platform::windows::shell::launch_program(
                target,
                non_empty_param(params),
                target.parent(),
            )
            .map(|_| ())
            .map_err(ActionFailure::from)
        }

        #[cfg(not(windows))]
        {
            let _ = target;
            let _ = params;
            Err(ActionFailure::from(LauncherError::UnsupportedPlatform {
                platform: std::env::consts::OS,
            }))
        }
    }

    fn run_as_admin(&self, path: &str, params: &str) -> ActionResult<AdminLaunchResult> {
        let path = self.runtime_path(path);
        if path.is_empty() {
            return Err(ActionFailure::invalid_input("program path is empty"));
        }

        let target = Path::new(&path);
        if target.is_dir() {
            return Err(ActionFailure::is_directory(
                "directories cannot be run as administrator",
            ));
        }

        #[cfg(windows)]
        {
            let result =
                crate::platform::windows::shell::run_as_admin(target, non_empty_param(params))
                    .map_err(ActionFailure::from)?;
            let executable = path;
            match result.status {
                crate::platform::windows::shell::AdminLaunchStatus::Started => {
                    Ok(AdminLaunchResult::success(result.outcome.code, executable))
                }
                crate::platform::windows::shell::AdminLaunchStatus::Cancelled => {
                    Ok(AdminLaunchResult::cancelled(
                        "ShellExecuteW administrator launch was cancelled or denied.",
                        result.outcome.code,
                        result.outcome.last_error,
                        executable,
                    ))
                }
            }
        }

        #[cfg(not(windows))]
        {
            let _ = target;
            let _ = params;
            Err(ActionFailure::from(LauncherError::UnsupportedPlatform {
                platform: std::env::consts::OS,
            }))
        }
    }

    fn open_in_explorer(&self, raw_path: &str) -> ActionResult<Option<ExplorerOpenFeedback>> {
        let target_path = self.runtime_path(raw_path);
        if target_path.is_empty() {
            return Err(ActionFailure::invalid_input("path is empty"));
        }

        let target = PathBuf::from(&target_path);
        #[cfg(windows)]
        {
            if target.is_dir() {
                crate::platform::windows::shell::open_folder(&target)
                    .map(|_| None)
                    .map_err(ActionFailure::from)
            } else if target.is_file() {
                crate::platform::windows::shell::select_in_explorer(&target)
                    .map(|_| None)
                    .map_err(ActionFailure::from)
            } else if let Some(parent_dir) = target.parent().filter(|parent| parent.is_dir()) {
                crate::platform::windows::shell::open_folder(parent_dir)
                    .map(|_| {
                        Some(ExplorerOpenFeedback::new(
                            "warning",
                            format!("대상을 찾을 수 없어 상위 폴더를 엽니다:\n{target_path}"),
                        ))
                    })
                    .map_err(ActionFailure::from)
            } else {
                Ok(Some(ExplorerOpenFeedback::new(
                    "warning",
                    format!("경로를 찾을 수 없습니다:\n{target_path}"),
                )))
            }
        }

        #[cfg(not(windows))]
        {
            let _ = target;
            Err(ActionFailure::from(LauncherError::UnsupportedPlatform {
                platform: std::env::consts::OS,
            }))
        }
    }

    fn copy_to_clipboard(&self, text: &str) -> ActionResult<()> {
        if text.is_empty() {
            return Err(ActionFailure::invalid_input("clipboard text is empty"));
        }
        #[cfg(windows)]
        {
            crate::platform::windows::clipboard::set_clipboard_text(text)
                .map_err(ActionFailure::from)
        }

        #[cfg(not(windows))]
        {
            let _ = text;
            Err(ActionFailure::from(LauncherError::UnsupportedPlatform {
                platform: std::env::consts::OS,
            }))
        }
    }
}

#[derive(Debug, Clone)]
pub struct LauncherActionService<P> {
    platform: P,
}

impl<P> LauncherActionService<P>
where
    P: LauncherPlatform,
{
    pub fn new(platform: P) -> Self {
        Self { platform }
    }

    pub fn platform(&self) -> &P {
        &self.platform
    }

    pub fn supports_native_admin(&self) -> bool {
        self.platform.supports_native_admin()
    }

    pub fn prepare_button_action(&self, button: &LauncherButton) -> ButtonActionRequest {
        self.prepare_button_action_input(Some(ButtonActionInput::from(button)))
    }

    pub fn prepare_button_action_input(
        &self,
        info: Option<ButtonActionInput>,
    ) -> ButtonActionRequest {
        let Some(info) = info else {
            return ButtonActionRequest::noop();
        };
        if info.name.trim().is_empty() {
            return ButtonActionRequest::noop();
        }

        if info.action == 1 {
            let command = copy_command_text(&info.path, &info.params);
            if command.is_empty() {
                return ButtonActionRequest::noop();
            }
            return ButtonActionRequest::copy_to_clipboard(command);
        }

        if info.action != 0 {
            return ButtonActionRequest::noop();
        }

        let path = self.platform.expand_path(&info.path);
        let params = self.platform.expand_path(&info.params);
        if path.trim().is_empty() {
            return ButtonActionRequest::noop();
        }
        if self.platform.is_linux()
            && (self.platform.has_windows_path_syntax(&path)
                || has_unresolved_windows_env_reference(&path))
        {
            return ButtonActionRequest::noop_with_messages(vec![UserMessage::new(
                "info",
                "Launch",
                format!(
                    "Windows 전용 경로는 Linux에서 직접 실행할 수 없습니다.\n{}",
                    self.platform.normalize_path(&path)
                ),
            )]);
        }

        let mut admin = info.admin;
        let mut pre_messages = Vec::new();
        if admin && !self.supports_native_admin() {
            admin = false;
            pre_messages.push(UserMessage::new(
                "info",
                "Run as administrator",
                "현재 플랫폼에서는 관리자 권한 실행을 지원하지 않아 일반 실행으로 전환합니다.",
            ));
        }

        ButtonActionRequest::launch(path, params, admin, pre_messages)
    }

    pub fn execute_button_action(&self, request: &ButtonActionRequest) -> Vec<UserMessage> {
        match request.kind {
            ButtonActionKind::Noop => Vec::new(),
            ButtonActionKind::Launch => self.execute_launch_action(request),
            ButtonActionKind::CopyToClipboard => self.execute_copy_action(request),
        }
    }

    pub fn open_in_explorer(&self, raw_path: &str, _is_manual_tab: bool) -> Vec<UserMessage> {
        let raw_path = raw_path.trim();
        if raw_path.is_empty() {
            return vec![UserMessage::new(
                "info",
                "탐색기에서 열기",
                "열 수 있는 경로가 없습니다.",
            )];
        }

        let expanded_raw_path = self.platform.expand_path(raw_path);
        if self.platform.is_linux()
            && (self.platform.has_windows_path_syntax(&expanded_raw_path)
                || has_unresolved_windows_env_reference(&expanded_raw_path))
        {
            return vec![UserMessage::new(
                "info",
                "탐색기에서 열기",
                format!(
                    "Windows 전용 경로는 Linux에서 직접 열 수 없습니다.\n{}",
                    self.platform.normalize_path(&expanded_raw_path)
                ),
            )];
        }

        match self.platform.open_in_explorer(&expanded_raw_path) {
            Ok(None) => Vec::new(),
            Ok(Some(feedback)) => vec![UserMessage::new(
                feedback.level,
                "탐색기에서 열기",
                feedback.message,
            )],
            Err(error) => vec![build_open_in_explorer_failure_message(&error)],
        }
    }

    fn execute_launch_action(&self, request: &ButtonActionRequest) -> Vec<UserMessage> {
        if request.admin {
            match self.platform.run_as_admin(&request.path, &request.params) {
                Ok(result) if result.ok() => Vec::new(),
                Ok(result) => vec![admin_result_user_message(&result)],
                Err(error) => vec![build_admin_failure_message(&error)],
            }
        } else {
            match self.platform.launch(&request.path, &request.params) {
                Ok(()) => Vec::new(),
                Err(error) => vec![build_launch_failure_message(&error)],
            }
        }
    }

    fn execute_copy_action(&self, request: &ButtonActionRequest) -> Vec<UserMessage> {
        match self.platform.copy_to_clipboard(&request.command) {
            Ok(()) => Vec::new(),
            Err(error) => vec![build_copy_failure_message(&error)],
        }
    }
}

impl Default for LauncherActionService<SystemLauncherPlatform> {
    fn default() -> Self {
        Self::new(SystemLauncherPlatform::default())
    }
}

fn build_launch_failure_message(error: &ActionFailure) -> UserMessage {
    match error.kind {
        ActionFailureKind::NotFound => UserMessage::new(
            "error",
            "Launch failed",
            "실행 대상을 찾을 수 없습니다.\n경로를 확인해 주세요.",
        ),
        ActionFailureKind::PermissionDenied => UserMessage::new(
            "error",
            "Launch failed",
            "대상을 실행할 권한이 없거나 현재 플랫폼에서 지원되지 않는 방식입니다.",
        ),
        ActionFailureKind::InvalidInput => UserMessage::new(
            "error",
            "Launch failed",
            "실행 설정이 올바르지 않습니다.\n경로와 인수를 확인해 주세요.",
        ),
        ActionFailureKind::Platform => UserMessage::new(
            "error",
            "Launch failed",
            "대상을 실행하지 못했습니다.\n경로와 플랫폼 호환 여부를 확인해 주세요.",
        ),
        ActionFailureKind::IsDirectory
        | ActionFailureKind::RuntimeUnavailable
        | ActionFailureKind::Unexpected => {
            UserMessage::new("error", "Launch failed", "대상을 실행하지 못했습니다.")
        }
    }
}

fn build_admin_failure_message(error: &ActionFailure) -> UserMessage {
    match error.kind {
        ActionFailureKind::IsDirectory => UserMessage::new(
            "error",
            "Run as administrator",
            "폴더는 관리자 권한으로 실행할 수 없습니다.\n일반 실행으로 열어 주세요.",
        ),
        ActionFailureKind::NotFound => UserMessage::new(
            "error",
            "Run as administrator",
            "관리자 권한으로 실행할 대상을 찾을 수 없습니다.\n경로를 확인해 주세요.",
        ),
        ActionFailureKind::InvalidInput => UserMessage::new(
            "error",
            "Run as administrator",
            "관리자 권한 실행 설정이 올바르지 않습니다.\n경로와 인수를 확인해 주세요.",
        ),
        ActionFailureKind::PermissionDenied
        | ActionFailureKind::RuntimeUnavailable
        | ActionFailureKind::Platform
        | ActionFailureKind::Unexpected => UserMessage::new(
            "error",
            "Run as administrator",
            "관리자 권한으로 실행하지 못했습니다.",
        ),
    }
}

pub(crate) fn admin_result_user_message(result: &AdminLaunchResult) -> UserMessage {
    match result.status {
        AdminLaunchStatus::Cancelled => UserMessage::new(
            "info",
            "Run as administrator",
            "관리자 권한 실행이 취소되었거나 권한 요청이 거부되었습니다.",
        ),
        AdminLaunchStatus::Exception => UserMessage::new(
            "error",
            "Run as administrator",
            "관리자 권한 실행 중 내부 오류가 발생했습니다.\n잠시 후 다시 시도해 주세요.",
        ),
        AdminLaunchStatus::Failed | AdminLaunchStatus::Success => UserMessage::new(
            "error",
            "Run as administrator",
            "관리자 권한 실행 요청을 완료하지 못했습니다.",
        ),
    }
}

fn build_open_in_explorer_failure_message(error: &ActionFailure) -> UserMessage {
    match error.kind {
        ActionFailureKind::RuntimeUnavailable => UserMessage::new(
            "error",
            "탐색기에서 열기",
            "이 시스템에서 경로를 열 수 있는 파일 관리자를 찾지 못했습니다.",
        ),
        ActionFailureKind::InvalidInput => {
            UserMessage::new("info", "탐색기에서 열기", "열 수 있는 경로가 없습니다.")
        }
        ActionFailureKind::NotFound
        | ActionFailureKind::PermissionDenied
        | ActionFailureKind::IsDirectory
        | ActionFailureKind::Platform
        | ActionFailureKind::Unexpected => UserMessage::new(
            "error",
            "탐색기에서 열기",
            "경로를 열 수 없습니다.\n설정된 경로를 확인해 주세요.",
        ),
    }
}

fn build_copy_failure_message(error: &ActionFailure) -> UserMessage {
    match error.kind {
        ActionFailureKind::InvalidInput => {
            UserMessage::new("info", "Copy", "복사할 명령이 없습니다.")
        }
        ActionFailureKind::PermissionDenied | ActionFailureKind::RuntimeUnavailable => {
            UserMessage::new(
                "warning",
                "Copy",
                "현재 플랫폼에서 클립보드를 사용할 수 없습니다.",
            )
        }
        ActionFailureKind::NotFound
        | ActionFailureKind::IsDirectory
        | ActionFailureKind::Platform
        | ActionFailureKind::Unexpected => {
            UserMessage::new("warning", "Copy", "클립보드에 복사하지 못했습니다.")
        }
    }
}

fn copy_command_text(path: &str, params: &str) -> String {
    let path = path.trim();
    let params = params.trim();
    match (path.is_empty(), params.is_empty()) {
        (true, true) => String::new(),
        (false, true) => path.to_owned(),
        (true, false) => params.to_owned(),
        (false, false) => format!("{path} {params}"),
    }
}

#[cfg(windows)]
fn non_empty_param(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalized_runtime_path(value: &str) -> String {
    expand_environment_variables(value.trim().trim_matches('"'))
}

pub(crate) fn resolve_runtime_path(value: &str, base_dir: Option<&Path>) -> String {
    let path = normalized_runtime_path(value);
    let Some(base_dir) = base_dir else {
        return path;
    };
    if path.is_empty() {
        return path;
    }
    if runtime_uri_scheme(&path).is_some() {
        return path;
    }

    let target = PathBuf::from(&path);
    if is_rooted_or_absolute_runtime_path(&target, &path) {
        return path;
    }

    if has_runtime_path_separator(&path) {
        return base_dir.join(target).to_string_lossy().into_owned();
    }

    let local_target = base_dir.join(&target);
    if local_target.exists() {
        local_target.to_string_lossy().into_owned()
    } else {
        path
    }
}

fn is_rooted_or_absolute_runtime_path(path: &Path, raw_path: &str) -> bool {
    path.is_absolute()
        || path.has_root()
        || has_windows_path_syntax(raw_path)
        || raw_path.starts_with("\\\\")
}

fn has_runtime_path_separator(value: &str) -> bool {
    value.contains('\\') || value.contains('/')
}

pub(crate) fn runtime_uri_scheme(value: &str) -> Option<&str> {
    let value = value.trim();
    let (scheme, _) = value.split_once(':')?;
    if scheme.len() <= 1 {
        return None;
    }
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')) {
        return None;
    }
    Some(scheme)
}

pub(crate) fn is_windows_absolute_path(value: &str) -> bool {
    if value.starts_with("\\\\") {
        return true;
    }
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(drive), Some(':'), Some('\\' | '/')) if drive.is_ascii_alphabetic()
    )
}

pub(crate) fn has_windows_path_syntax(value: &str) -> bool {
    is_windows_absolute_path(value) || is_windows_drive_qualified_path(value)
}

fn is_windows_drive_qualified_path(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic()
    )
}

pub(crate) fn has_unresolved_windows_env_reference(value: &str) -> bool {
    let value = value.trim();
    if value.starts_with('/') && !value.contains('\\') && !has_windows_path_syntax(value) {
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

pub(crate) fn expand_environment_variables(value: &str) -> String {
    let with_windows_vars = expand_percent_variables(value);
    expand_dollar_variables(&with_windows_vars)
}

fn expand_percent_variables(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('%') else {
            output.push('%');
            output.push_str(after_start);
            return output;
        };
        let name = &after_start[..end];
        if name.is_empty() {
            output.push_str("%%");
        } else if let Some(value) = std::env::var_os(name) {
            output.push_str(&value.to_string_lossy());
        } else {
            output.push('%');
            output.push_str(name);
            output.push('%');
        }
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output
}

fn expand_dollar_variables(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        if character != '$' {
            output.push(character);
            continue;
        }

        let Some((next_index, next_character)) = chars.peek().copied() else {
            output.push('$');
            continue;
        };

        if next_character == '{' {
            let mut end_index = None;
            for (candidate_index, candidate) in chars.clone() {
                if candidate == '}' {
                    end_index = Some(candidate_index);
                    break;
                }
            }
            let Some(end_index) = end_index else {
                output.push_str(&value[index..]);
                return output;
            };

            chars.next();
            while chars
                .peek()
                .is_some_and(|(candidate_index, _)| *candidate_index <= end_index)
            {
                chars.next();
            }

            let name_start = next_index + next_character.len_utf8();
            let name = &value[name_start..end_index];
            if name.is_empty() {
                output.push_str("${}");
            } else if let Some(env_value) = std::env::var_os(name) {
                output.push_str(&env_value.to_string_lossy());
            } else {
                output.push_str(&value[index..end_index + 1]);
            }
            continue;
        }

        if !is_env_name_start(next_character) {
            output.push('$');
            continue;
        }

        let mut end_index = next_index + next_character.len_utf8();
        chars.next();
        while let Some((candidate_index, candidate)) = chars.peek().copied() {
            if !is_env_name_continue(candidate) {
                break;
            }
            end_index = candidate_index + candidate.len_utf8();
            chars.next();
        }

        let name = &value[next_index..end_index];
        if let Some(env_value) = std::env::var_os(name) {
            output.push_str(&env_value.to_string_lossy());
        } else {
            output.push_str(&value[index..end_index]);
        }
    }
    output
}

fn is_env_name_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_env_name_continue(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum PlatformCall {
        Launch { path: String, params: String },
        RunAsAdmin { path: String, params: String },
        OpenInExplorer { raw_path: String },
        CopyToClipboard { text: String },
    }

    #[derive(Debug)]
    struct MockPlatform {
        supports_native_admin: bool,
        is_linux: bool,
        calls: RefCell<Vec<PlatformCall>>,
        launch_result: ActionResult<()>,
        admin_result: ActionResult<AdminLaunchResult>,
        explorer_result: ActionResult<Option<ExplorerOpenFeedback>>,
        copy_result: ActionResult<()>,
    }

    impl Default for MockPlatform {
        fn default() -> Self {
            Self {
                supports_native_admin: true,
                is_linux: false,
                calls: RefCell::new(Vec::new()),
                launch_result: Ok(()),
                admin_result: Ok(AdminLaunchResult::success(33, "tool.exe")),
                explorer_result: Ok(None),
                copy_result: Ok(()),
            }
        }
    }

    impl LauncherPlatform for MockPlatform {
        fn supports_native_admin(&self) -> bool {
            self.supports_native_admin
        }

        fn expand_path(&self, value: &str) -> String {
            value.replace("%TOOLS%", "C:\\Tools")
        }

        fn is_linux(&self) -> bool {
            self.is_linux
        }

        fn has_windows_path_syntax(&self, value: &str) -> bool {
            has_windows_path_syntax(value)
        }

        fn launch(&self, path: &str, params: &str) -> ActionResult<()> {
            self.calls.borrow_mut().push(PlatformCall::Launch {
                path: path.to_owned(),
                params: params.to_owned(),
            });
            self.launch_result.clone()
        }

        fn run_as_admin(&self, path: &str, params: &str) -> ActionResult<AdminLaunchResult> {
            self.calls.borrow_mut().push(PlatformCall::RunAsAdmin {
                path: path.to_owned(),
                params: params.to_owned(),
            });
            self.admin_result.clone()
        }

        fn open_in_explorer(&self, raw_path: &str) -> ActionResult<Option<ExplorerOpenFeedback>> {
            self.calls.borrow_mut().push(PlatformCall::OpenInExplorer {
                raw_path: raw_path.to_owned(),
            });
            self.explorer_result.clone()
        }

        fn copy_to_clipboard(&self, text: &str) -> ActionResult<()> {
            self.calls.borrow_mut().push(PlatformCall::CopyToClipboard {
                text: text.to_owned(),
            });
            self.copy_result.clone()
        }
    }

    fn button(name: &str, action: u8) -> LauncherButton {
        LauncherButton {
            name: name.to_owned(),
            action,
            path: String::from("%TOOLS%\\tool.exe"),
            params: String::from("--fast"),
            admin: false,
            auto_enter: false,
            ..LauncherButton::manual_default()
        }
    }

    #[test]
    fn empty_button_prepares_noop() {
        let service = LauncherActionService::new(MockPlatform::default());

        let request = service.prepare_button_action(&LauncherButton::manual_default());

        assert!(request.is_noop());
        assert_eq!(service.platform.calls.borrow().len(), 0);
    }

    #[test]
    fn action_zero_prepares_launch_request() {
        let service = LauncherActionService::new(MockPlatform::default());

        let request = service.prepare_button_action(&button("Tool", 0));

        assert_eq!(request.kind, ButtonActionKind::Launch);
        assert_eq!(request.path, "C:\\Tools\\tool.exe");
        assert_eq!(request.params, "--fast");
        assert!(!request.admin);
    }

    #[test]
    fn linux_launch_request_reports_windows_absolute_path_without_launching() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let request = service.prepare_button_action(&button("Tool", 0));

        assert_eq!(request.kind, ButtonActionKind::Noop);
        assert_eq!(request.pre_messages.len(), 1);
        assert_eq!(request.pre_messages[0].level, "info");
        assert!(
            request.pre_messages[0]
                .message
                .contains("Windows 전용 경로")
        );
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_launch_request_reports_windows_unc_path_without_launching() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);
        let mut source = button("Tool", 0);
        source.path = String::from(r"\\server\share\tool.exe");

        let request = service.prepare_button_action(&source);

        assert_eq!(request.kind, ButtonActionKind::Noop);
        assert_eq!(request.pre_messages.len(), 1);
        assert_eq!(request.pre_messages[0].level, "info");
        assert!(
            request.pre_messages[0]
                .message
                .contains("Windows 전용 경로")
        );
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_launch_request_reports_windows_drive_relative_path_without_launching() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);
        let mut source = button("Tool", 0);
        source.path = String::from("C:Tools\\tool.exe");

        let request = service.prepare_button_action(&source);

        assert_eq!(request.kind, ButtonActionKind::Noop);
        assert_eq!(request.pre_messages.len(), 1);
        assert_eq!(request.pre_messages[0].level, "info");
        assert!(
            request.pre_messages[0]
                .message
                .contains("Windows 전용 경로")
        );
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_launch_request_reports_unresolved_windows_env_path_without_launching() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);
        let mut source = button("Tool", 0);
        source.path = String::from("%USERPROFILE%\\tool.exe");

        let request = service.prepare_button_action(&source);

        assert_eq!(request.kind, ButtonActionKind::Noop);
        assert_eq!(request.pre_messages.len(), 1);
        assert_eq!(request.pre_messages[0].level, "info");
        assert!(
            request.pre_messages[0]
                .message
                .contains("Windows 전용 경로")
        );
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_launch_request_allows_posix_absolute_literal_percent_path() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);
        let mut source = button("Tool", 0);
        source.path = String::from("/tmp/%stage%/tool");

        let request = service.prepare_button_action(&source);

        assert_eq!(request.kind, ButtonActionKind::Launch);
        assert_eq!(request.path, "/tmp/%stage%/tool");
        assert!(request.pre_messages.is_empty());
    }

    #[test]
    fn action_one_prepares_copy_request() {
        let mut source = button("Tool", 1);
        source.path = String::from("dir.exe");
        source.params = String::from("/a");
        let service = LauncherActionService::new(MockPlatform::default());

        let request = service.prepare_button_action(&source);

        assert_eq!(request.kind, ButtonActionKind::CopyToClipboard);
        assert_eq!(request.command, "dir.exe /a");
    }

    #[test]
    fn copy_request_accepts_path_or_params_only() {
        assert_eq!(copy_command_text(" tool.exe ", ""), "tool.exe");
        assert_eq!(copy_command_text("", " --help "), "--help");
        assert_eq!(copy_command_text("", ""), "");
    }

    #[test]
    fn runtime_path_resolves_separator_relative_path_against_base_dir() {
        let base_dir = Path::new("C:\\Launcher");

        let resolved = resolve_runtime_path("my\\tool.exe", Some(base_dir));

        assert_eq!(PathBuf::from(resolved), base_dir.join("my\\tool.exe"));
    }

    #[test]
    fn runtime_path_preserves_protocol_uri_targets() {
        let base_dir = Path::new("/launcher");

        assert_eq!(
            resolve_runtime_path("https://example.test/tools", Some(base_dir)),
            "https://example.test/tools"
        );
        assert_eq!(
            resolve_runtime_path("mailto:user@example.test", Some(base_dir)),
            "mailto:user@example.test"
        );
        assert_eq!(runtime_uri_scheme("C:Tools\\tool.exe"), None);
    }

    #[test]
    fn runtime_path_keeps_absolute_windows_path() {
        let resolved = resolve_runtime_path("C:\\Tools\\tool.exe", Some(Path::new("C:\\Launcher")));

        assert_eq!(resolved, "C:\\Tools\\tool.exe");
    }

    #[test]
    fn runtime_path_keeps_windows_drive_qualified_path() {
        let resolved = resolve_runtime_path("C:Tools\\tool.exe", Some(Path::new("/launcher")));

        assert_eq!(resolved, "C:Tools\\tool.exe");
    }

    #[test]
    fn windows_path_syntax_detection_includes_unc_and_drive_relative_paths() {
        assert!(is_windows_absolute_path("C:\\Tools\\tool.exe"));
        assert!(is_windows_absolute_path("\\\\server\\share\\tool.exe"));
        assert!(has_windows_path_syntax("C:Tools\\tool.exe"));
        assert!(!is_windows_absolute_path("tools\\tool.exe"));
        assert!(!has_windows_path_syntax("tools\\tool.exe"));
    }

    #[test]
    fn runtime_path_uses_base_dir_for_existing_bare_command()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("runtime-local-command")?;
        let local_tool = temp.path().join("CommandTimer.exe");
        fs::write(&local_tool, b"tool")?;

        let resolved = resolve_runtime_path("CommandTimer.exe", Some(temp.path()));

        assert_eq!(PathBuf::from(resolved), local_tool);
        Ok(())
    }

    #[test]
    fn runtime_path_keeps_missing_bare_command_for_shell_lookup()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = TempTestDir::new("runtime-shell-command")?;

        let resolved = resolve_runtime_path("notepad.exe", Some(temp.path()));

        assert_eq!(resolved, "notepad.exe");
        Ok(())
    }

    #[test]
    fn unknown_action_prepares_noop() {
        let service = LauncherActionService::new(MockPlatform::default());

        let request = service.prepare_button_action(&button("Tool", 9));

        assert!(request.is_noop());
    }

    #[test]
    fn admin_request_falls_back_when_native_admin_is_unavailable() {
        let platform = MockPlatform {
            supports_native_admin: false,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);
        let mut source = button("Tool", 0);
        source.admin = true;

        let request = service.prepare_button_action(&source);

        assert_eq!(request.kind, ButtonActionKind::Launch);
        assert!(!request.admin);
        assert_eq!(request.pre_messages.len(), 1);
        assert_eq!(request.pre_messages[0].level, "info");
        assert_eq!(request.pre_messages[0].title, "Run as administrator");
        assert_eq!(
            request.pre_messages[0].message,
            "현재 플랫폼에서는 관리자 권한 실행을 지원하지 않아 일반 실행으로 전환합니다."
        );
    }

    #[test]
    fn launch_failure_messages_hide_internal_detail() {
        let platform = MockPlatform {
            launch_result: Err(ActionFailure::not_found("missing.exe")),
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let messages = service.execute_button_action(&ButtonActionRequest::launch(
            "missing.exe",
            "",
            false,
            Vec::new(),
        ));

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, "error");
        assert_eq!(messages[0].title, "Launch failed");
        assert!(messages[0].message.contains("실행 대상을 찾을 수 없습니다"));
        assert!(!messages[0].message.contains("missing.exe"));
    }

    #[test]
    fn platform_shell_execute_failures_map_to_common_failure_kinds() {
        let cases = [
            (
                "ShellExecuteW failed (operation='open', code=2 SE_ERR_FNF, last_error=0 ERROR_SUCCESS)",
                ActionFailureKind::NotFound,
            ),
            (
                "ShellExecuteW failed (operation='open', code=3 SE_ERR_PNF, last_error=3 ERROR_PATH_NOT_FOUND)",
                ActionFailureKind::NotFound,
            ),
            (
                "ShellExecuteW failed (operation='open', code=5 SE_ERR_ACCESSDENIED, last_error=5 ERROR_ACCESS_DENIED)",
                ActionFailureKind::PermissionDenied,
            ),
            (
                "ShellExecuteW failed (operation='open', code=31 SE_ERR_NOASSOC, last_error=0 ERROR_SUCCESS)",
                ActionFailureKind::RuntimeUnavailable,
            ),
            (
                "ShellExecuteW failed (operation='open', code=32 SE_ERR_DLLNOTFOUND, last_error=126 ERROR_MOD_NOT_FOUND)",
                ActionFailureKind::NotFound,
            ),
            (
                "ShellExecuteW failed (operation='open', code=0 SE_ERR_OOM_OR_ZERO, last_error=87 ERROR_INVALID_PARAMETER)",
                ActionFailureKind::InvalidInput,
            ),
        ];

        for (message, expected_kind) in cases {
            let failure = ActionFailure::from(LauncherError::Platform {
                message: String::from(message),
            });

            assert_eq!(failure.kind, expected_kind);
        }
    }

    #[test]
    fn admin_failure_messages_are_mapped_by_error_kind() {
        let cases = [
            (
                ActionFailure::is_directory("C:\\Tools"),
                "폴더는 관리자 권한으로 실행할 수 없습니다.",
            ),
            (
                ActionFailure::not_found("missing.exe"),
                "관리자 권한으로 실행할 대상을 찾을 수 없습니다.",
            ),
            (
                ActionFailure::invalid_input("bad params"),
                "관리자 권한 실행 설정이 올바르지 않습니다.",
            ),
        ];

        for (error, expected) in cases {
            let platform = MockPlatform {
                admin_result: Err(error),
                ..MockPlatform::default()
            };
            let service = LauncherActionService::new(platform);
            let messages = service.execute_button_action(&ButtonActionRequest::launch(
                "tool.exe",
                "",
                true,
                Vec::new(),
            ));

            assert_eq!(messages.len(), 1);
            assert!(messages[0].message.contains(expected));
        }
    }

    #[test]
    fn admin_result_messages_are_mapped_without_internal_detail() {
        let cases = [
            (
                AdminLaunchResult::cancelled("tool.exe denied", 5, 1223, "tool.exe"),
                "info",
                "취소",
            ),
            (
                AdminLaunchResult::failed("code 31 tool.exe", 31, 0, "tool.exe"),
                "error",
                "완료하지 못했습니다",
            ),
            (
                AdminLaunchResult::exception("RuntimeError: boom", "tool.exe"),
                "error",
                "내부 오류",
            ),
        ];

        for (result, level, expected) in cases {
            let platform = MockPlatform {
                admin_result: Ok(result),
                ..MockPlatform::default()
            };
            let service = LauncherActionService::new(platform);
            let messages = service.execute_button_action(&ButtonActionRequest::launch(
                "tool.exe",
                "",
                true,
                Vec::new(),
            ));

            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].level, level);
            assert!(messages[0].message.contains(expected));
            assert!(!messages[0].message.contains("Windows"));
            assert!(!messages[0].message.contains("tool.exe"));
            assert!(!messages[0].message.contains("boom"));
        }
    }

    #[test]
    fn open_explorer_failure_messages_are_mapped() {
        let cases = [
            (
                ActionFailure::runtime_unavailable("xdg-open not found"),
                "error",
                "파일 관리자를 찾지 못했습니다",
            ),
            (
                ActionFailure::invalid_input("empty path"),
                "info",
                "열 수 있는 경로가 없습니다",
            ),
            (
                ActionFailure::platform("explorer failed for C:\\secret"),
                "error",
                "경로를 열 수 없습니다",
            ),
        ];

        for (error, level, expected) in cases {
            let platform = MockPlatform {
                explorer_result: Err(error),
                ..MockPlatform::default()
            };
            let service = LauncherActionService::new(platform);

            let messages = service.open_in_explorer("C:\\Tools", false);

            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].level, level);
            assert!(messages[0].message.contains(expected));
            assert!(!messages[0].message.contains("C:\\secret"));
        }
    }

    #[test]
    fn linux_open_explorer_reports_unresolved_windows_env_path_without_opening() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let messages = service.open_in_explorer("%USERPROFILE%\\Desktop", false);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, "info");
        assert!(messages[0].message.contains("Windows 전용 경로"));
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_open_explorer_reports_windows_drive_relative_path_without_opening() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let messages = service.open_in_explorer("C:Tools\\tool.exe", false);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, "info");
        assert!(messages[0].message.contains("Windows 전용 경로"));
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_open_explorer_reports_windows_unc_path_without_opening() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let messages = service.open_in_explorer(r"\\server\share\tool.exe", false);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, "info");
        assert!(messages[0].message.contains("Windows 전용 경로"));
        assert!(service.platform.calls.borrow().is_empty());
    }

    #[test]
    fn linux_open_explorer_allows_posix_absolute_literal_percent_path() {
        let platform = MockPlatform {
            is_linux: true,
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let messages = service.open_in_explorer("/tmp/%stage%/tool", false);

        assert!(messages.is_empty());
        assert_eq!(
            service.platform.calls.borrow().as_slice(),
            &[PlatformCall::OpenInExplorer {
                raw_path: String::from("/tmp/%stage%/tool"),
            }]
        );
    }

    #[test]
    fn open_explorer_passes_expanded_path_to_platform() {
        let service = LauncherActionService::new(MockPlatform::default());

        let messages = service.open_in_explorer("%TOOLS%\\tool.exe", false);

        assert!(messages.is_empty());
        assert_eq!(
            service.platform.calls.borrow().as_slice(),
            &[PlatformCall::OpenInExplorer {
                raw_path: String::from("C:\\Tools\\tool.exe"),
            }]
        );
    }

    #[test]
    fn copy_failure_messages_hide_internal_detail() {
        let platform = MockPlatform {
            copy_result: Err(ActionFailure::platform("OpenClipboard failed last_error=5")),
            ..MockPlatform::default()
        };
        let service = LauncherActionService::new(platform);

        let messages =
            service.execute_button_action(&ButtonActionRequest::copy_to_clipboard("tool.exe /a"));

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].level, "warning");
        assert_eq!(messages[0].title, "Copy");
        assert!(
            messages[0]
                .message
                .contains("클립보드에 복사하지 못했습니다")
        );
        assert!(!messages[0].message.contains("OpenClipboard"));
    }

    #[test]
    fn platform_mock_receives_launch_admin_explorer_and_copy_calls() {
        let service = LauncherActionService::new(MockPlatform::default());

        let launch_messages = service.execute_button_action(&ButtonActionRequest::launch(
            "tool.exe",
            "--fast",
            false,
            Vec::new(),
        ));
        let admin_messages = service.execute_button_action(&ButtonActionRequest::launch(
            "admin.exe",
            "--safe",
            true,
            Vec::new(),
        ));
        let explorer_messages = service.open_in_explorer("C:\\Tools", true);
        let copy_messages =
            service.execute_button_action(&ButtonActionRequest::copy_to_clipboard("tool.exe /a"));

        assert!(launch_messages.is_empty());
        assert!(admin_messages.is_empty());
        assert!(explorer_messages.is_empty());
        assert!(copy_messages.is_empty());
        assert_eq!(
            *service.platform.calls.borrow(),
            vec![
                PlatformCall::Launch {
                    path: String::from("tool.exe"),
                    params: String::from("--fast"),
                },
                PlatformCall::RunAsAdmin {
                    path: String::from("admin.exe"),
                    params: String::from("--safe"),
                },
                PlatformCall::OpenInExplorer {
                    raw_path: String::from("C:\\Tools"),
                },
                PlatformCall::CopyToClipboard {
                    text: String::from("tool.exe /a"),
                },
            ]
        );
    }

    #[test]
    fn execute_copy_request_uses_clipboard() {
        let service = LauncherActionService::new(MockPlatform::default());
        let request = ButtonActionRequest::copy_to_clipboard("hello");

        let messages = service.execute_button_action(&request);

        assert!(messages.is_empty());
        assert_eq!(
            service.platform.calls.borrow().as_slice(),
            [PlatformCall::CopyToClipboard {
                text: String::from("hello"),
            }]
        );
    }

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(label: &str) -> std::io::Result<Self> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "j3launcher-actions-test-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let temp_dir = std::env::temp_dir();
            if self.path.starts_with(&temp_dir) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }
}
