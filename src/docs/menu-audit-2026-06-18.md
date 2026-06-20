# Menu audit - 2026-06-18

## Scope

- 환경: Windows, `c:\_my\src\j3Launcher`
- 기준 앱: `target\debug\j3launcher.exe`
- 방식: 서브에이전트 없이 메뉴 영역을 순차 점검했다.
- Linux 재실행: 이 Windows 환경에는 WSL이 설치되어 있지 않아 `tests/linux_ui_smoke.sh`는 재실행하지 못했다. Linux 항목은 `docs/linux-parity-report.md`의 2026-06-17 X11 smoke 결과를 기존 동작으로 기록한다.
- 추가 설치 도구: 없음.

## Findings

| 메뉴 | 기능 | Windows 동작 | Linux 기존 동작 | 문제 여부 | 원인 | 수정 내용 | 재검증 결과 |
| -- | -- | ---------- | ----------- | ----- | -- | ----- | ------ |
| UI | Main layout/visible controls | 초기, resize 후, Dark Theme 후 main window/tab/button rect가 visible이고 parent 안에 있음 | X11 smoke는 resize/focus 입력과 UI flow 통과 | 없음 | 해당 없음 | Windows smoke에 HWND rect 기반 main layout case 추가 | Windows smoke pass |
| File | Menu structure/visible state | 실제 Win32 `HMENU`에서 top-level `File`, 21개 항목/구분선, Win32 accelerator label, 수동 탭/폴더 탭/Dark Theme enable-check 상태를 확인 | GTK menu model은 같은 공유 메뉴 spec/section/action 순서 테스트와 X11 메뉴 flow가 기존 통과 | 없음 | 해당 없음 | Windows smoke에 `GetMenu`/`GetMenuState` 기반 `menu-structure-and-state`, `menu-folder-command-state` case 추가 | debug/release-compatible smoke pass |
| Build/Run | Windows release-compatible smoke | debug-only picker/context override를 제외한 실제 native picker/menu/button/layout/log flow가 `target\release` 바이너리에서 통과 | Linux는 release-compatible X11 subset 기존 통과 | 없음 | 해당 없음 | Windows smoke에 `-SkipDebugOnly`를 추가해 release-compatible subset 분리 | release smoke pass |
| State | Save/restore after restart | resize, Dark Theme, tab move/add 저장 후 앱 종료/재시작 시 geometry/theme/tab order가 복원되고 후속 메뉴 명령 처리됨 | Linux는 geometry/theme/save flow 기존 테스트와 X11 smoke 통과 | 없음 | 해당 없음 | Windows smoke에 실제 종료 후 같은 config로 재시작하는 state restore case 추가 | Windows smoke pass |
| File | Add Folder Tab... | 실제 native picker selected-folder scan/save와 cancel/no-mutation/owner 복귀 통과. error/duplicate-focus 앱 flow는 debug picker override로 통과 | debug picker override로 selected/cancel/error/duplicate-focus flow 통과 | 일부 남음 | Linux native chooser 선택 UI는 자동화 안정 경로 없음 | Windows native selected/cancel smoke와 debug-only picker override smoke case 추가 | Windows smoke pass; Linux native chooser UI는 수동 확인 필요 |
| File | Add Tab | 클릭 명령으로 새 수동 탭 저장 | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | `windows_ui_smoke.ps1` pass |
| File | Set Current Tab Folder... | 실제 native picker selected-folder scan/save와 cancel/no-mutation/owner 복귀 통과. error/duplicate-focus 앱 flow는 debug picker override로 통과. 수동 탭 disabled no-op 확인 | debug picker override로 selected/cancel/error/duplicate-focus flow 통과 | 일부 남음 | Linux native chooser 선택 UI는 자동화 안정 경로 없음 | Windows native selected/cancel smoke와 debug-only picker override smoke case 추가 | Windows smoke pass; Linux native chooser UI는 수동 확인 필요 |
| File | Current Tab Layout... | dialog title 표시, Rows/Cols 변경 저장, dialog controls rect visible/inside 확인 | X11 smoke 통과 | 수정됨 | Win32 custom dialog가 `WM_NCCREATE`에서 `DefWindowProcW`를 건너뛰어 title text가 비었고, 첫 edit focus가 명시되지 않음 | `WM_NCCREATE` 기본 처리 보존, Rows edit focus 지정, Windows smoke에 dialog layout rect case 추가 | Windows smoke pass |
| File | Rename Current Tab... | dialog title 표시, 이름 변경 저장, dialog controls rect visible/inside 확인 | X11 smoke 통과 | 수정됨 | 위와 동일하게 title/focus 경로가 약했음 | `WM_NCCREATE` 기본 처리 보존, text input focus 지정, Windows smoke에 dialog layout rect case 추가 | Windows smoke pass |
| File | Delete Current Tab... | Yes 삭제 저장, No 기본 버튼 no-op 확인 | X11 smoke 통과 | 없음 | 해당 없음 | Windows smoke가 No 기본 버튼 style을 검사하도록 보강 | Windows smoke pass |
| File | Move Tab Left | 현재 탭 왼쪽 이동 저장, 단일 탭 disabled no-op | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Move Tab Right | 현재 탭 오른쪽 이동 저장, 단일 탭 disabled no-op | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Select Previous Tab | 선택 이동 후 후속 Move가 선택 탭에 적용됨, 단일 탭 no-op | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Select Next Tab | 선택 이동 후 후속 Move가 선택 탭에 적용됨, 단일 탭 no-op | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Sorting Current Tab | 폴더 탭 정렬 저장, 수동 탭 disabled no-op, resize/focus 후 `F5` accelerator 동작 확인 | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Refresh Current Tab | 폴더 재스캔 후 새 파일 반영, 수동 탭 disabled no-op | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Reset Current Tab | No 기본 버튼 no-op, Yes 재스캔/숨김 초기화 저장 | X11 smoke 통과 | 없음 | 해당 없음 | Windows smoke가 No 기본 버튼 style을 검사하도록 보강 | Windows smoke pass |
| File | Manage Hidden Items... | dialog title 표시, 열기/닫기 확인, 수동 탭 disabled no-op, dialog close control rect visible/inside 확인 | X11 smoke 통과 | 수정됨 | Win32 custom dialog title이 비는 공통 원인 | `WM_NCCREATE` 기본 처리 보존, Windows smoke에 dialog layout rect case 추가 | Windows smoke pass |
| File | Dark Theme | 체크 토글 저장 및 즉시 반영 | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| File | Exit | 메뉴 명령으로 앱 종료 | X11 smoke 통과 | 없음 | 해당 없음 | 없음 | Windows smoke pass |
| Button context | Edit | debug context command override로 `WM_CONTEXTMENU` 이후 Edit dialog 저장 flow 통과. Edit dialog controls rect visible/inside 확인. optional `SendInput` pointer probe는 현재 세션에서 popup을 열지 못해 기본 smoke에서 제외 | X11 smoke 통과 | 수정됨, 일부 남음 | Edit dialog도 같은 `WM_NCCREATE` title 보존 문제와 first edit focus 명시 누락, 기존 Windows smoke의 context flow 공백. 실제 pointer input은 현재 자동화 세션에서 앱에 전달되지 않음 | `WM_NCCREATE` 기본 처리 보존, Name edit focus 지정, Windows smoke에 context Edit 저장과 dialog layout rect case 추가. optional physical probe는 `-IncludePhysicalInput` 뒤로 분리 | Windows smoke pass; 실제 pointer popup 선택은 수동 확인 필요 |
| Button context | Open in Explorer | debug context command override로 missing target 오류 메시지와 owner 복귀 통과. 실제 Explorer 폴더 열기 성공 경로도 temp folder와 Shell COM window 확인/닫기로 통과 | X11 smoke는 Windows-path guard와 fake folder handler 통과 | 없음 | 해당 없음 | Windows smoke에 missing target 오류/복귀와 Explorer folder success/owner 복귀 case 추가 | Windows smoke pass |
| Button context | Hide | debug context command override로 `WM_CONTEXTMENU` 이후 folder item hide/save flow 통과. 실제 pointer popup 선택은 수동 범위 | X11 smoke 통과 | 일부 남음 | 실제 pointer context 자동화는 현재 세션에서 안정 경로 없음 | Windows smoke에 context Hide 저장 case 추가 | Windows smoke pass; 실제 pointer popup 선택은 수동 확인 필요 |
| Button | Drag/drop | Win32 drag event/cursor 경계로 두 버튼 swap 저장 확인. optional `SendInput` physical drag probe는 현재 세션에서 저장 상태를 바꾸지 못해 기본 smoke에서 제외 | X11 smoke는 실제 pointer drag swap 통과 | 일부 남음 | 물리 pointer drag 자동화는 Windows 세션 입력 전달에 의존하며 현재 세션에서는 마우스 메시지가 앱 drag 경로까지 도달하지 않음 | Windows smoke에 button drag swap 저장 case 추가. optional physical drag probe는 `-IncludePhysicalInput` 뒤로 분리 | Windows smoke pass; 실제 pointer drag는 수동 확인 권장 |
| Button | Copy Path + Params | 버튼 컨트롤 `BM_CLICK`으로 실제 clipboard가 `Path + Params` 텍스트로 변경됨 | X11 smoke 통과 | 없음 | 해당 없음 | Windows smoke에 버튼 클릭/clipboard 확인 추가 | Windows smoke pass |
| Button | Launch | 버튼 컨트롤 `BM_CLICK`으로 격리된 임시 `.cmd` 실행, marker 파일 생성 확인 | X11 smoke 통과 | 없음 | 해당 없음 | Windows smoke에 버튼 클릭/ShellExecute launch smoke 추가 | Windows smoke pass |
| Button | Admin | 관리자 실행 directory target 오류 메시지와 owner 복귀 통과. 실제 UAC/runas prompt/승인 경로는 수동 범위 | X11 smoke는 fake pkexec 통과 | 일부 남음 | 실제 UAC/runas UX는 OS 정책과 사용자 승인에 의존 | Windows smoke에 admin directory target 오류/복귀 case 추가 | Windows smoke pass; 실제 UAC prompt/승인은 수동 확인 필요 |

## Problems Reproduced And Fixed

| 문제 | 재현 방법 | 근본 원인 | 수정 |
| -- | -- | -- | -- |
| Windows smoke가 실행 전 파싱 오류로 중단 | `tests/windows_ui_smoke.ps1` 실행 시 `"$Name: ok"` 파싱 실패 | PowerShell이 `$Name:`을 drive-qualified variable처럼 해석 | `"${Name}: ok"`로 명확화 |
| Windows smoke가 메인 창을 찾지 못함 | 앱은 뜨지만 `window did not open ... j3Launcher` | 실제 창 title은 `j3Launcher v0.1.0`, 하네스는 exact `j3Launcher`를 요구 | PID + class 기준으로 메인 창 식별 |
| Windows smoke가 `WM_COMMAND`를 보내지 못함 | `[UIntPtr]$CommandId` 변환 오류 | Windows PowerShell의 `UIntPtr` 직접 캐스팅 불가 | `[UIntPtr]::new([uint32]...)` 사용 |
| Win32 입력 dialog title이 비어 자동화와 UI 표시가 깨짐 | Rename 명령 후 `TextInputDialog` title이 빈 문자열 | custom window proc가 `WM_NCCREATE`에서 `DefWindowProcW`를 호출하지 않아 `lpWindowName` 기본 처리가 생략됨 | text/layout/hidden/edit dialog의 `WM_NCCREATE`에서 상태 포인터 저장 후 `DefWindowProcW` 호출 |
| Rename/Layout/Edit keyboard focus가 명시적이지 않음 | dialog 표시 후 입력 자동화가 edit control을 안정적으로 대상으로 삼지 못함 | 첫 edit control focus를 toolkit 기본값에 의존 | text/layout/edit dialog 생성 후 첫 edit control에 `SetFocus` |
| Windows smoke dialog 입력이 앱 내부 값에 반영되지 않음 | 외부 `SetWindowText` 후 OK를 눌러도 앱은 기존 값 읽음 | cross-process `SetWindowText`/`GetWindowText`가 edit control 내부 버퍼 검증에 부적합 | smoke 입력을 `EM_SETSEL` + `EM_REPLACESEL`로 변경 |
| Windows button/accelerator coverage가 약함 | 기존 smoke는 메뉴 `WM_COMMAND` 위주라 버튼 클릭, drag swap, clipboard, ShellExecute launch, resize/focus 후 accelerator를 직접 확인하지 않음 | 하네스 범위 부족 | child button lookup + `BM_CLICK`, Win32 drag event/cursor 기반 swap, clipboard 확인, 임시 `.cmd` marker launch, resize/focus 후 `F5` accelerator case 추가 |
| Windows UI 표시/레이아웃 검증이 상태 저장 위주였음 | 메뉴/버튼 동작은 통과하지만 child control rect가 보이는 위치/크기를 갖는지 자동 확인이 부족 | 기존 smoke가 설정 파일 결과만 주로 검사 | main/tab/button rect와 주요 custom dialog control rect가 visible/inside/min-size인지 확인하는 layout smoke 추가 |
| Windows 실제 메뉴 구조/표시 상태 검증이 약함 | `WM_COMMAND`를 직접 보내면 메뉴 항목 순서, 구분선, accelerator label, disabled/checked visual state가 깨져도 통과 가능 | 기존 smoke가 명령 dispatch 결과 위주였고 실제 `HMENU` 상태를 읽지 않음 | `GetMenu`/`GetSubMenu`/`GetMenuStringW`/`GetMenuState`로 File 메뉴 구조와 수동/폴더 탭별 enable state, Dark Theme check state를 검증하는 case 추가 |
| Windows smoke 성공 경로에서 앱 로그 진단을 보지 않음 | 메뉴/버튼 flow가 모두 통과해도 stdout/stderr에 panic/ERROR/CRITICAL 같은 진단이 남는지 확인되지 않음 | 기존 harness가 GUI 프로세스 출력을 캡처하지 않음 | 앱별 stdout/stderr를 case별 log file로 redirect하고 성공 case 종료 후 unexpected diagnostic pattern을 검사 |
| Windows release 바이너리 smoke가 debug-only override에 묶여 있음 | release 바이너리는 `J3LAUNCHER_TEST_PICK_FOLDER`/`J3LAUNCHER_TEST_CONTEXT_MENU_COMMAND`를 사용하지 않으므로 기존 전체 smoke를 그대로 실행할 수 없음 | debug-only app flow와 release-compatible real UI flow가 한 harness에 섞여 있었음 | `-SkipDebugOnly`/`J3LAUNCHER_UI_SMOKE_SKIP_DEBUG_ONLY`와 `Run-DebugOnlyCase`를 추가해 release-compatible subset을 분리 |
| Windows 상태 복원 검증이 설정 파일 단편 확인에 가까웠음 | 저장된 config는 확인하지만 동일 config로 재시작했을 때 geometry/theme/tab order가 실제 UI 시작 경로에서 복원되는지 별도 검증이 약함 | 기존 smoke가 단일 프로세스 실행 중 변경 결과를 주로 확인 | resize/theme/tab 변경 후 Exit, 같은 config로 재시작, window size/theme/tab order/후속 메뉴 처리 확인 case 추가 |
| Windows Add/Set folder picker 앱 flow가 자동화되지 않음 | Add Folder Tab/Set Current Tab Folder는 native picker 때문에 기존 Windows smoke에서 selected/cancel/error/duplicate-focus 결과를 재현하지 못함 | OS picker UI 자동화에 의존하면 smoke가 불안정하고 앱 flow 검증이 비어 있었음 | `src/platform/windows/dialogs.rs`에 debug-only picker override를 추가하고 Windows smoke에 selected/cancel/error/duplicate-focus case 추가 |
| Windows native picker cancel/owner 복귀가 기록되지 않음 | Add Folder Tab/Set Current Tab Folder 실제 picker를 열고 취소한 뒤 상태 변경 여부와 후속 메뉴 처리 확인 필요 | 기존 smoke가 native shell dialog 자체를 열지 않았음 | Windows smoke에 실제 native picker `IDCANCEL` case를 추가하고 cancel 후 후속 메뉴 명령 처리까지 확인 |
| Windows native picker selected-folder 자동화가 기본 사용자 폴더를 선택함 | 실제 picker edit control에 경로를 넣고 OK만 클릭하면 `C:\Users\dolco`가 저장됨 | `SHBrowseForFolderW` dialog는 edit text 변경만으로 내부 tree selection을 확정하지 않으며 Enter/navigation 이벤트가 필요함 | native picker 선택 helper가 edit에 focus를 주고 `EM_REPLACESEL` 후 `VK_RETURN`으로 선택을 확정하며, dialog가 이미 닫혔으면 추가 OK 클릭을 생략 |
| Windows context menu Edit/Hide 앱 flow가 자동화되지 않음 | 버튼 우클릭 메뉴 선택은 OS popup 입력에 의존해 기존 Windows smoke에서 Edit 저장과 Hide 저장을 재현하지 못함 | `TrackPopupMenu` 선택을 외부 입력으로 안정적으로 재현하기 어려움 | debug-only context command override를 추가하고 `WM_CONTEXTMENU` 이후 Edit/Hide 저장 case 추가 |
| Windows Open in Explorer/Admin 경계가 UI smoke에 없음 | explorer folder success, missing explorer target, admin directory target에서 사용자/OS 경계와 owner 복귀 확인 필요 | 실제 Explorer/UAC 성공 경로만 생각하면 자동화 부작용이 커져 안전한 오류 경계 검증도 빠짐 | Open in Explorer는 temp folder를 실제 Explorer로 열고 Shell COM으로 확인 후 닫는 success smoke와 missing target 오류/복귀 smoke 추가. Admin은 directory target 오류/복귀 smoke 추가 |
| Windows physical pointer input probe가 앱 경로를 재현하지 못함 | `-IncludePhysicalInput`으로 context Edit와 physical drag case 실행 | 현재 자동화 세션의 `SendInput` 마우스 입력이 Win32 popup/drag 경로에 안정적으로 전달되지 않음 | optional physical-input case로 분리해 기본 회귀 smoke에서는 제외하고, 실제 포인터 검증은 수동 항목으로 유지 |

## Commands Run

| Command | Exit | Result |
| -- | -- | -- |
| `cargo build` | 0 | pass |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe` | 0 | pass; includes Add/Set folder native picker selected/cancel, Add/Set folder error/duplicate app flows through debug override, actual Win32 File `HMENU` structure/enabled/checked state checks, File menu commands, state restore after restart, main/dialog layout rect checks, context Edit/Hide/OpenInExplorer error and Explorer folder success app flows through debug override, disabled no-op cases, button drag swap, admin directory error, resize/focus `F5`, button Copy clipboard, button Launch marker smoke, and success-path stdout/stderr diagnostic scan |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only menu-structure-and-state` | 0 | pass; verifies top-level File menu, item order/separators/accelerator labels, manual-tab enable state, Dark Theme check state, and state changes after Select Next |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only menu-folder-command-state` | 0 | pass; verifies folder-tab File menu enable state including Set Folder, Sort, Refresh, Reset, and Manage Hidden Items |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only state-restore-after-restart` | 0 | pass; resize/theme/tab changes persisted, app exited, restarted with same config, restored geometry/theme/tab order, and accepted a follow-up menu command |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -SkipDebugOnly` | 0 | pass; verifies the release-compatible Windows subset against the debug binary |
| `cargo build --release` | 0 | pass |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\release\j3launcher.exe -SkipDebugOnly` | 0 | pass; release binary validated for actual File `HMENU` structure/enabled/checked state, real native picker selected/cancel, File menu subset, state restore after restart, layout rect checks, button drag event/copy/launch/admin-error, Dark Theme, Exit, and stdout/stderr diagnostic scan |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only main-layout-rects` | 0 | pass; initial/resized/dark-theme main window, tab control, and button rects are visible and inside parent |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only dialog-layout-rects` | 0 | pass; Rename, Tab Layout, Manage Hidden Items, and Edit Button dialog controls are visible and inside their dialog |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only context-open-explorer-folder-success` | 0 | pass; actual Explorer folder window opened for a temp target, was detected through Shell COM, closed, and the app accepted a later menu command |
| `cargo fmt --check` | 0 | pass |
| `cargo test` | 0 | 183 tests pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | pass |
| `cargo check` | 0 | pass |
| `git diff --check` | 0 | pass; Git reported LF-to-CRLF working-copy warnings only |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only context-edit-save-physical-pointer -IncludePhysicalInput` | 1 | probe failed; popup class `#32768` did not appear from current `SendInput` session |
| `powershell -ExecutionPolicy Bypass -File tests\windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe -Only button-physical-drag-swap -IncludePhysicalInput` | 1 | probe failed; physical drag did not persist the swapped button order in this session |
| `wsl -l -q` | 1 | WSL not installed, Linux smoke not runnable here |
| `rustup target list --installed` | 1 | `rustup` not available in PATH |
| `cargo check --target x86_64-unknown-linux-gnu` | 1 | Failed before project code check because GTK/GObject/Cairo/Pango sys crates require a configured Linux `pkg-config` cross sysroot |

## Remaining Checks

- Windows native folder picker selected-folder and cancel flows are now covered. Linux GTK native chooser UI still needs manual selected-folder recording; selected/error/duplicate-focus app flows are covered through the debug picker override.
- Windows native pointer context popup selection, actual UAC/runas prompt/approval, and physical pointer drag/drop should be exercised manually or with a broader UI harness; Edit/Hide/OpenInExplorer error and Explorer folder success app flows, admin error handling, and drag swap persistence are now covered by smoke. Direct popup probes either opened `#32768` without reliable command dispatch or, with `SendInput`, did not open the popup in this session. Physical drag `SendInput` probe also did not reach the app drag path, so both remain outside the default regression gate.
- Linux GTK smoke was not rerun in this Windows environment. Use `tests/linux_ui_smoke.sh` on a Linux desktop with `xdotool`, `jq`, and `update-desktop-database`.
