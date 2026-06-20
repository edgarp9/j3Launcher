# 릴리스 준비 점검

기준일: 2026-05-02

## Dependency

- `serde`: 설정 도메인 모델 직렬화/역직렬화에 필요하다.
- `serde_json`: 기존 `j3Launcher.json` 호환 파싱, 정규화, 저장에 필요하다.
- `windows-sys`: Win32 UI, Shell, DPI, DWM, GDI, COM, 클립보드, 파일 교체에 필요하다. `default-features = false`로 두고 필요한 Win32 feature만 선언한다.

## windows-sys feature 근거

- `Win32_Foundation`: HWND/HRESULT/RECT, last-error, 공통 Win32 타입.
- `Win32_Storage_FileSystem`: `MoveFileExW` 기반 설정 파일 교체.
- `Win32_System_DataExchange`: `OpenClipboard`, `EmptyClipboard`, `SetClipboardData` 기반 텍스트 복사.
- `Win32_System_Memory`: `CF_UNICODETEXT` clipboard payload용 movable global memory.
- `Win32_UI_WindowsAndMessaging`: 창 생성, 메시지 루프, 메뉴, 버튼, 아이콘, 키 입력 보조.
- `Win32_UI_Shell`, `Win32_UI_Shell_Common`: `ShellExecuteW`, `SHGetFileInfoW`, `BROWSEINFOW`, `SHBrowseForFolderW`.
- `Win32_UI_Input_KeyboardAndMouse`: Win32 UI focus helper와 keyboard accelerator virtual-key constants.
- `Win32_Graphics_Dwm`: dark titlebar 속성.
- `Win32_Graphics_Gdi`: HDC/HBITMAP/DIB/ImageList용 비트맵 처리.
- `Win32_System_Com`: shell dialog PIDL 해제와 icon 추출 worker의 COM 초기화.
- `Win32_System_LibraryLoader`: 현재 module handle 조회.
- `Win32_UI_Controls`: tab control, common controls, image list.
- `Win32_UI_HiDpi`: process/window DPI API.

## unsafe 근거

- `src/platform/windows/mod.rs`: `MoveFileExW` 호출. 입력 path는 내부 NUL을 검사한 NUL-terminated UTF-16 buffer이며 호출 중 수명이 유지된다.
- `src/platform/windows/dialogs.rs`: shell folder dialog와 PIDL path 변환. `BROWSEINFOW`/buffer는 호출 중 유효하고, 반환 PIDL은 `CoTaskMemFree`로 한 번 해제한다.
- `src/platform/windows/dpi.rs`: DPI awareness와 DPI query. 포인터를 넘기지 않거나 Win32 sentinel handle만 사용한다.
- `src/platform/windows/dwm.rs`: `DwmSetWindowAttribute`에 stack `i32` pointer와 정확한 크기를 넘기며, 호출 후 pointer를 보관하지 않는다.
- `src/platform/windows/clipboard.rs`: 클립보드 문자열은 NUL-terminated UTF-16으로 만들고, `GlobalAlloc(GMEM_MOVEABLE)` handle은 `SetClipboardData` 성공 전까지 guard가 소유해 실패 경로에서 해제한다. 클립보드 open 상태도 guard로 닫는다.
- `src/platform/windows/icon.rs`: HICON/HDC/HBITMAP/GDI object를 RAII guard로 소유하고, bitmap buffer 길이와 좌표를 검증한 뒤 raw pointer/slice를 만든다.
- `src/platform/windows/input.rs`: `WindowHandle`이 0이 아닌 HWND만 감싸고, Win32 API 호출 경계에는 이 래퍼를 통해 검증된 handle만 넘긴다.
- `src/platform/windows/shell.rs`: `ShellExecuteW` 문자열은 내부 NUL을 거부한 NUL-terminated UTF-16 buffer이고, last-error 접근은 thread-local Win32 상태만 다룬다.
- `src/ui/win32/native.rs`: HWND/HMENU/control/message glue 경계다. `Win32App`과 dialog state는 `Box::into_raw`로 창 생성 전에 소유권을 분리하고, `WM_NCCREATE`에서 pointer를 저장한 뒤 `WM_NCDESTROY`에서 정확히 한 번 회수한다. 생성 중 실패해 `WM_NCDESTROY`가 먼저 실행된 경우 호출자 실패 경로가 같은 pointer를 다시 drop하지 않도록 creation state가 파괴 여부를 기록한다. modal 완료 플래그와 결과는 단일 UI thread용 `ModalDialogState<T>`가 `Rc<Cell<_>>`/`RefCell<Option<_>>`로 공유해 stack raw pointer aliasing을 피한다. child HWND, menu, icon, image list는 생성 주체가 소유권을 갖고 teardown에서 해제한다.

## unwrap/expect

런타임 경로의 패닉성 `unwrap()`/`expect()`는 검색되지 않았다. 현재 `expect()`는 단위 테스트의 fixture/Mock setup에만 있다. `unwrap_or`/`unwrap_or_else`는 복구 기본값 선택 용도로 남긴다.

## 이번 하드닝에서 수정한 리스크

- Win32 창 생성 중 `WM_CREATE` 실패가 발생하면 `WM_NCDESTROY`에서 app state를 회수한 뒤 호출자 stack의 `Box`가 다시 drop될 수 있었다. 창 생성 전에 raw pointer로 소유권을 분리하고 creation state로 파괴 여부를 기록하도록 수정했다.
- edit dialog도 같은 생성 메시지 소유권 패턴으로 맞췄고, modal loop가 `WM_QUIT` 또는 `GetMessageW` 오류로 중단될 때 dialog HWND를 명시적으로 닫아 dialog state가 남지 않게 했다.
- release 빌드에서 edit dialog wndproc이 stack `done` 플래그를 raw pointer로 쓰고 modal loop가 같은 값을 `&mut bool`로 읽는 aliasing UB 때문에 owner 재활성화까지 진행되지 않는 경로가 있었다. 모달 완료/결과 전달을 `ModalDialogState<T>` 공유 상태로 바꿔 닫기 후 owner가 disabled 상태로 남지 않게 했다.
- 음수 모니터 좌표 저장이 `800x600+-10+20`처럼 다시 파싱할 수 없는 문자열을 만들 수 있었다. 좌표 부호를 직접 포맷하고 극단 rect 차이는 saturating 계산으로 처리한다.
- context menu 항목 추가 실패를 무시하던 경로를 `Result`로 바꾸고, 실패 시 생성된 HMENU를 즉시 해제한다.

## 자동 검증 기준

- `cargo fmt --check`
- `cargo test`
- `cargo check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo build --release`

## 남은 수동 확인

- 릴리스 빌드 실행 파일을 더블클릭 또는 바로가기 시작 위치 지정 상태에서 실행한다.
- `j3Launcher.json` 생성/저장과 창 위치 저장을 실제 Windows UI에서 확인한다.
- 버튼 실행, 관리자 실행 취소, 탐색기 열기, `Copy Path + Params` 클립보드 복사를 실제 Windows UI에서 확인한다.
