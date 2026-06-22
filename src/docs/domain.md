# j3Launcher 도메인 기록

## 목적

j3Launcher는 기존 Python/Tkinter 런처를 Rust 기반 Windows 네이티브 앱으로 포팅하고, 같은 코드베이스에서 Linux GTK4 UI도 제공하는 프로젝트다. 원본 프로젝트는 `C:\Users\dolco\Desktop\src\j3Launcher`에 있으며, 포팅 중 참조만 하고 수정하지 않는다.

## 현재 기준 정보

- 앱 이름: `io.github.edgarp9.j3Launcher`
- 표시 이름: `j3Launcher`
- Linux application id: `io.github.edgarp9.j3Launcher`
- 프로젝트 라이선스: `GPL-3.0-or-later`
- 프로젝트 저작권 표시: `Copyright (C) 2026 j3Launcher contributors`
- 프로젝트 라이선스 전문: `LICENSE`
- 소스코드 제공 안내: About 화면의 `about.txt` 본문과 배포 채널 고지로 제공한다.
- About 링크: `https://github.com/edgarp9`
- About 라이선스 고지: 상단에는 앱 버전과 `https://github.com/edgarp9` 링크만 표시하고, 창은 450x350 크기로 유지하며, 하단 스크롤 본문은 `about.txt` 전체 내용을 내장 표시한다.
- 기본 설정 파일 이름: `j3Launcher.json`
- 창 아이콘 후보 파일: Windows는 `icon.ico`, Linux GTK는 `icon.svg` 우선, `icon.png` fallback
- Rust 크레이트 이름: `j3launcher`

## 초기 용어

- 런처 앱: Windows 네이티브 창과 실행 흐름을 소유하는 애플리케이션.
- 설정 문서: 기본 `j3Launcher.json` 또는 실행 인자로 지정한 JSON 문서. Rust 도메인에서는 `LauncherConfig`로 정규화하며, 원본 JSON 키 호환성을 유지한다.
- Win32 UI: Tkinter UI를 대체할 Windows 네이티브 창 계층.
- GTK4 UI: Linux 전용 GTK4 창 계층. Win32 UI와 같은 앱 유스케이스와 설정 모델을 호출한다.
- 플랫폼 경계: Win32 API 호출을 안전한 Rust API로 감싸는 `platform/windows` 모듈, Linux GTK/GIO 연동을 소유하는 `ui/gtk4` 모듈, 각 UI 메시지 glue를 소유하는 플랫폼별 `ui` 모듈.

## 책임 경계

- `main`: 프로세스 진입점과 최종 사용자 메시지 출력.
- `app`: 애플리케이션 실행 흐름과 작업 경계 조합.
- `domain`: 앱 메타데이터, 설정 문서 타입, 순수 규칙.
- `infra`: 파일 시스템과 JSON 직렬화 같은 외부 I/O.
- `platform/windows`: Windows API 연동과 Win32 리소스 소유권 래퍼.
- `ui/win32`: Win32 창 모델, 메시지 처리, UI 실행 경계.
- `ui/gtk4`: Linux GTK4 창 모델, 메뉴/다이얼로그/이벤트 처리, Linux 실행/클립보드 경계, GTK 창 아이콘 search path 설치.
- `ui/common`: Windows와 Linux가 공유하는 File/About 메뉴 정의/활성 조건/dispatch target, 버튼 context menu 항목/활성 조건, 버튼 라벨, `Open in Explorer` 대상 선택 우선순위, hidden item 목록/선택 변환, visible slot 계산 규칙.

## Windows 플랫폼 래퍼

`src/platform/windows`는 `windows-sys` 기반 Win32 호출을 안전한 Rust API로 감싼다.

- `shell`: `ShellExecuteW` 기반 실행, 관리자 실행, 파일/폴더 열기, explorer 선택 열기를 담당한다.
- `dialogs`: Windows 폴더 선택 대화상자처럼 HWND 소유자가 필요한 OS 대화상자를 담당한다.
- `input`: 0이 아닌 HWND만 감싸는 `WindowHandle` 경계 타입을 담당한다.
- `clipboard`: `Path + Params` 텍스트를 UTF-16 clipboard payload로 복사하는 OS 연동을 담당한다.
- `dpi`: 프로세스 DPI awareness 설정과 창/시스템 DPI 조회를 담당한다. Rust Win32 UI는 per-monitor DPI aware 정책을 유지하되, native size/move 중 non-client 자동 재계산 흔들림을 줄이기 위해 `PER_MONITOR_AWARE` v1을 v2보다 먼저 시도하고 system-aware는 fallback으로만 사용한다.
- `dwm`: DWM dark titlebar 속성 적용을 담당한다.
- `icon`: `SHGetFileInfoW` 기반 아이콘 추출과 `DestroyIcon`/GDI 리소스 정리를 담당한다.

이 모듈의 공개 API는 `Result`로 실패를 반환하고, 사용자 입력 문자열과 경로의 UTF-16 변환에서는 내부 NUL을 오류로 처리한다. Win32 직접 호출이 필요한 로직은 trait 경계와 순수 helper 테스트로 검증 가능하게 유지한다. `unsafe`는 `platform/windows` 래퍼와 `ui/win32/native.rs`의 Win32 메시지 glue 및 HWND/HMENU/HICON 수명 관리 경계에 둔다.
버튼 아이콘 표시는 `BCM_SETIMAGELIST`를 사용하므로 Windows 빌드는 `app.manifest`를 실행 파일에 임베드해 comctl32 v6 common controls를 활성화한다.
Linux GTK 창 아이콘은 GTK `icon-name`을 Linux application id인 `io.github.edgarp9.j3Launcher`로 설정한다. GTK4 기본 CSD titlebar는 이 window icon metadata를 제목줄의 시각 요소로 직접 그려주지 않으므로, 앱은 `gtk::WindowHandle` 기반 compact titlebar를 소유하고 그 안에 앱 아이콘, 중앙 제목, `gtk::WindowControls`를 명시적으로 배치한다. 직접 실행 또는 개발 실행에서도 타이틀바 아이콘을 보강하기 위해 실행 파일 폴더, 현재 작업 폴더 순서로 `icon.svg`를 먼저 찾고, 없으면 `icon.png`를 찾아 titlebar image와 GTK4 `GdkToplevel` texture icon list에 전달한다. 저장소의 `icon.svg`는 Linux desktop entry/icon theme 설치의 우선 자산이고, `icon.png`는 SVG가 없을 때의 fallback 자산이다. Wayland/KDE taskbar 아이콘은 application id와 `.desktop` 파일 매핑 및 icon theme 조회에 의존하므로, 배포/사용 환경에서는 `io.github.edgarp9.j3Launcher.desktop` 파일과 hicolor icon theme의 `io.github.edgarp9.j3Launcher.svg`를 함께 맞춘다. Plasma task manager는 일부 fallback 경로에서 desktop id를 lowercase로 매칭하므로, `--install`은 앱 메뉴 중복을 피하기 위해 `NoDisplay=true`가 들어간 `io.github.edgarp9.j3launcher.desktop` alias와 같은 이름의 SVG alias도 설치한다. 오래된 `j3Launcher.desktop`처럼 깨진 절대 `Icon=` 경로를 가진 legacy entry가 남아 있으면 KDE의 fallback 매칭이 그 파일을 집어 taskbar 아이콘이 비어 보일 수 있으므로, `--install`/`--uninstall`은 `dev.j3launcher.J3Launcher`, `j3Launcher`, `j3launcher` legacy id도 함께 제거한다.

## 규칙

- 복구 가능한 오류는 `Result`로 전달한다.
- 내부 오류 원인과 사용자에게 보여줄 메시지는 분리한다.
- 런타임 경로에서 `unwrap()`과 `expect()`를 사용하지 않는다.
- `unsafe`가 필요하면 `platform/windows` 또는 `ui/win32/native.rs`의 Win32 FFI 경계에만 둔다.
- 기존 Python/Tkinter 원본은 참조만 하며 수정하지 않는다.
- Win32 button edit dialog는 컨트롤 좌표 기준인 최소 클라이언트 영역을 보장해야 하며, 창 외곽 크기와 클라이언트 크기를 혼동해 하단 버튼이 잘리지 않게 유지한다.

## 설정 스키마

Rust 설정 도메인 모델은 `src/domain/config.rs`, `tab.rs`, `button.rs`에 둔다. 직렬화 키는 기존 JSON과 동일하게 유지한다.

최상위 설정:

- `Window`: 창 설정 객체. 없거나 객체가 아니면 기본값으로 복구한다.
- `Window.Geometry`: 창 크기/위치 문자열. 기본값은 `800x600`이다.
- `Window.DpiScale`: 선택적 DPI 배율 숫자. 양수만 유지한다.
- `Window.DarkTheme`: 선택적 다크테마 여부. `true`, `1`, `yes`, `on` 계열 값만 활성화하고 기본값은 `false`다.
- `FolderTabs`: 탭 배열. 없거나 배열이 아니면 빈 배열로 복구한다.
- legacy `Tabs` 설정이 있고 `FolderTabs`가 배열이 아니면 기존 버튼 섹션(`Tab0`, `Tab1`...)을 `folder` 탭으로 마이그레이션한다.

탭 설정:

- 공통 필드: `id`, `tab_type`, `title`, `folder_path`, `rows`, `cols`, `hidden_item_ids`, `slot_positions`, `buttons`.
- `tab_type`은 `folder`와 `manual`을 지원한다. 알 수 없는 값은 `folder`로 복구한다.
- `folder` 탭은 선택적으로 `scan_signature`, `scan_item_order`를 유지한다.
- `manual` 탭은 고정 슬롯 기반 탭이므로 `folder_path`, `hidden_item_ids`, `slot_positions`, `scan_signature`, `scan_item_order`를 비운다.
- 폴더 탭 변경 유스케이스는 `app::folder_tabs`에서 `LauncherTab` 목록을 순수하게 변경한다. 버튼 슬롯 이동 유스케이스는 `app::button_layout`에 둔다. 설정 파일 저장과 UI 갱신은 이 유스케이스 밖의 경계에서 수행한다.

## 앱 서비스 경계

`app::actions`는 버튼 실행 유스케이스를 소유한다.

- `ButtonActionRequest`는 UI 버튼 설정을 실행 전 요청으로 정규화한다.
- 버튼 이름이 비어 있으면 항상 `noop`이다.
- `action=0`은 실행 요청, `action=1`은 `path`와 `params`를 공백으로 이어 클립보드에 복사하는 요청, 그 외 값은 `noop`이다.
- 관리자 실행이 요청되었지만 현재 플랫폼에 관리자 실행 경계가 없으면 일반 실행으로 전환하고 안내 메시지를 함께 반환한다.
- Linux GTK는 관리자 실행 의도를 일반 실행으로 자동 낮추지 않는다. `pkexec`가 없거나 직접 지정한 대상 파일이 없거나 실행 권한이 없거나 bare command 대상이 `PATH`에서 발견되지 않으면 관리자 실행 실패로 분류해 사용자에게 알린다. `pkexec`로 시작한 권한 상승 요청은 GTK main loop에서 추적하며, 인증 dialog 취소(`126`)와 인증 실패/오류(`127`)는 사용자 메시지로 다시 보고한다.
- 실행, 관리자 실행, 탐색기 열기, 클립보드 복사는 `LauncherPlatform` trait 뒤에 둔다. UI 계층은 Win32 API나 `platform/windows`를 직접 호출하지 않고 앱 서비스를 통해서만 요청한다.
- 플랫폼 실패의 내부 상세와 사용자에게 보여줄 `UserMessage`는 분리한다.
- Win32 `ShellExecuteW` 실패 detail의 표준 `SE_ERR_*`/`ERROR_*` 코드는 앱 경계에서 공통 실패 범주로 정규화해 Linux `Command`/GIO 실패와 같은 사용자 메시지 분류를 사용한다.

`app::config_service`는 설정 파일 I/O를 담당하는 `infra::config_store::ConfigStore`를 앱 경계에서 감싼다. UI는 설정 읽기/저장 요청을 `ConfigService`로 보낸다. 폴더 탭 변경처럼 반복될 수 있는 저장은 deferred save worker로 큐잉하고, 결과 반영과 실패 롤백은 UI thread가 완료 메시지를 받은 뒤 처리한다.

CLI 인자는 `app` 경계에서 해석한다. 인자가 없으면 기본 설정 파일로 창을 실행하고, 설정 파일 인자 하나는 기존 호환 동작으로 유지한다. `--install`은 Linux 사용자 영역에 desktop entry와 hicolor 아이콘을 설치하는 명시 명령이고, `--uninstall`은 같은 파일들을 제거하는 명시 명령이다. 설치/제거의 파일 I/O는 `infra::desktop_entry`가 담당하며, `--install`은 중복 실행해도 같은 파일 내용이면 다시 쓰지 않고 성공해야 한다.

버튼 설정:

- 버튼 필드는 `item_id`, `source_name`, `source_path`, `is_dir`, `name`, `path`, `params`, `admin`, `action`, `auto_enter`를 사용한다.
- `auto_enter`는 legacy 설정 호환용 필드이며 현재 복사 동작에는 사용하지 않는다.
- `manual` 탭 버튼은 `item_id`를 비워 둘 수 있다.
- `folder` 탭 버튼은 `item_id`가 필요하며, 비어 있으면 경로 기반으로 보정한다.
- 폴더 스캔에서 만든 기본 버튼은 스캔 항목의 `item_id`, `name`, `path`, `is_dir`를 `source_*`와 표시/실행 기본값으로 복사하고, `params`는 빈 문자열, `admin=false`, `action=0`, `auto_enter=false`로 둔다.
- 폴더 탭 refresh는 기존 버튼의 사용자 편집값을 가능한 한 보존한다. 스캔 항목이 같은 `item_id`로 다시 발견되면 `source_name`, `source_path`, `is_dir`는 새 스캔 값으로 갱신하고, 표시 이름이 비어 있을 때만 스캔 이름으로 채운다.

## 폴더 스캔 규칙

폴더 스캔 도메인 값은 `src/domain/scan.rs`에 둔다.

- `ScanItem`: 스캔된 폴더 항목. `item_id`, `name`, `path`, `is_dir`를 가진다.
- `ScanFailure`: 개별 항목을 읽지 못했을 때의 항목 이름과 내부 오류 상세.
- `FolderScanResult`: 스캔 항목, 실패 목록, 취소 여부, `scan_signature`, unchanged 여부를 가진다.
- `ScanSignature`: `version=1`, `path`, `mtime_ns`, `ctime_ns`, `size`로 구성한다.

실제 파일 시스템 접근은 `src/infra/folder_scan.rs`에서 수행한다.

- 스캔 전 취소 토큰이 설정되어 있으면 파일 시스템 I/O 없이 취소 결과를 반환한다.
- 폴더 경로가 비어 있거나 폴더가 아니면 복구 가능한 오류로 반환한다.
- 항목 정렬은 폴더 먼저, 파일 나중이며 각 그룹 안에서는 이름의 case-insensitive 순서를 사용한다.
- 개별 항목의 metadata 또는 item id 생성이 실패하면 전체 스캔을 중단하지 않고 `ScanFailure`에 기록한다.
- 스캔 시작과 종료 시점의 signature가 같을 때만 결과의 `scan_signature`를 유지한다. 스캔 중 폴더 metadata가 바뀌면 signature를 비워 다음 refresh가 캐시 재사용을 하지 않게 한다.
- 전달된 known signature가 현재 폴더 signature와 같으면 `read_dir` 가능 여부만 확인한 뒤 known items를 재사용하고 `unchanged=true`를 반환한다.
- 취소 토큰은 `Arc<AtomicBool>` 기반으로 공유하며, 스캔 루프와 정렬 경계에서 반복 확인한다.

## 탭 변경 규칙

`app::folder_tabs`는 다음 유스케이스를 제공한다.

- add folder tab: 중복 폴더와 최대 탭 수를 확인한 뒤, 스캔 항목으로 폴더 탭을 만들고 `scan_signature`, `scan_item_order`를 함께 저장한다.
- add manual tab: 기본 3행 8열 슬롯 수만큼 빈 수동 버튼을 만든다. 스캔 관련 필드는 비운다.
- set tab folder: 대상 탭의 폴더 경로를 바꾸고 스캔 항목으로 버튼을 재생성한다. 기존 `hidden_item_ids`와 `slot_positions`는 새 스캔 항목 기준으로 정리한다.
- refresh tab: 기존 버튼과 새 스캔 항목을 `item_id` 기준으로 병합한다. 완전한 스캔이면 사라진 항목과 그 항목의 숨김/슬롯 정보를 제거하고, 불완전한 스캔이면 누락 가능성이 있는 기존 버튼과 숨김 정보를 보존하되 scan metadata는 비운다.
- cancelled scan result는 탭을 변경하지 않는다. stale tab id로 도착한 refresh 결과는 `TabNotFound`로 거부하고 기존 탭 상태를 유지한다.
- reset tab: 스캔 항목만으로 버튼 목록을 다시 만들고 `slot_positions`를 비운다.
- sort tab: 폴더 먼저, 파일 나중, 이름/source path/item id의 case-insensitive 순서로 버튼을 정렬하고 `slot_positions`를 비운다. scan metadata는 유지한다.
- hide item: 폴더 탭에서만 유효하며, 빈 item id와 중복 숨김 항목은 추가하지 않는다.
- unhide items: 폴더 탭에서만 유효하며, 지정한 item id를 `hidden_item_ids`에서 제거한다.
- move button between slots: 수동 탭은 두 슬롯의 버튼 설정 전체를 교환한다. 대상 슬롯이 비어 있으면 빈 설정과 교환되어 이동처럼 동작한다. 폴더 탭은 스캔 항목과 사용자 설정은 각 버튼에 그대로 둔 채 `slot_positions`만 교환해 화면상 위치를 맞바꾼다.
- move/select tab: `app::tab_actions`에서 현재 인덱스와 좌우 이동 가능 여부를 계산한다. 범위를 벗어난 선택이나 첫/마지막 탭 바깥으로의 이동은 차단된 결과로 반환한다.

`scan_item_order`는 스캔 항목의 `item_id` 전체 순서를 중복 없이 저장할 수 있을 때만 유지한다. unchanged signature로 known items를 만들 때는 현재 버튼 순서가 아니라 `scan_item_order`를 기준으로 원래 스캔 순서를 복원한다.

## Win32 UI 1차 범위

`ui/win32`는 Rust 포팅의 첫 Windows 네이티브 UI다.

- 프로세스 시작 시 DPI awareness를 초기화하고, 현재 DPI 기준으로 저장된 창 geometry를 적용한다. 저장된 `Window.DpiScale`은 Win32 pixel geometry를 시작/복원 시점에만 논리 크기 기준으로 보정하기 위한 값이며, 모니터 이동 중에는 geometry 되먹임에 사용하지 않는다.
- top-level 창의 native size/move 루프는 `WM_ENTERSIZEMOVE`/`WM_EXITSIZEMOVE`로 추적한다. 루프 중 `WM_DPICHANGED`가 오면 suggested rectangle 적용, DPI metric 갱신, 레이아웃 재계산을 즉시 수행하지 않고 pending DPI로 표시한 뒤, 루프 종료 시 실제 현재 HWND DPI와 client rect 기준으로 한 번만 갱신한다.
- 창은 `windows-sys` 기반 top-level window, native menu, tab control, 현재 탭 버튼 grid로 구성한다.
- 메뉴는 File 아래에 원본 Tkinter 메뉴와 맞춘 add folder tab, add tab, set current tab folder, current tab layout, rename/delete current tab, move/select tab, sorting/refresh/reset current tab, manage hidden items, exit 명령을 두고, 별도 About 메뉴에 앱 버전, `https://github.com/edgarp9` 링크, 하단 스크롤의 `about.txt` 본문을 보여주는 about 명령을 둔다. About 링크는 Windows 기본 브라우저로 연다. Move/select와 sorting 명령은 원본과 같은 accelerator 표시와 Win32 accelerator 처리를 제공한다.
- File 메뉴의 Dark Theme 옵션은 `Window.DarkTheme`에 저장되며, 토글 시 메인 창 타이틀바와 탭/버튼 영역을 즉시 다시 그린다.
- 다크 테마의 탭 페이지 콘텐츠 배경은 버튼이 없는 빈 슬롯과 남는 영역까지 동일한 어두운 배경으로 표시한다.
- folder tab 추가와 refresh는 `infra::folder_scan` 작업을 별도 worker에서 수행하고, 닫기 시 취소 토큰을 설정한 뒤 worker를 join한다.
- tab 생성/폴더 연결/레이아웃 변경/이름 변경/삭제/정렬/이동/숨김/숨김 해제는 `app::folder_tabs`와 `app::tab_actions` 유스케이스를 통해 수행하고, `ConfigService` deferred save worker에 저장을 요청한다. UI는 요청이 큐에 들어간 뒤 낙관적으로 갱신하고, 최신 저장 실패가 완료 메시지로 돌아오면 마지막 committed 설정으로 롤백한다.
- 버튼 클릭은 `app::actions::LauncherActionService`에 `LauncherButton`을 전달해 실행 요청과 사용자 메시지 생성을 처리한다.
- 버튼 드래그 앤 드롭은 같은 탭 안의 다른 버튼 위에 놓을 때만 적용한다. 일반 클릭을 방해하지 않도록 Win32 버튼의 마우스 이동이 임계값을 넘은 경우에만 드래그로 보고, 저장은 다른 탭 변경과 동일하게 `ConfigService` deferred save 경로를 사용한다.
- 버튼 아이콘은 Windows에서만 `SHGetFileInfoW`로 실행 파일, 폴더, 일반 파일의 시스템 아이콘을 추출한다. 추출은 UI thread를 막지 않도록 background worker에서 수행하고, 결과 적용은 `WM_ICON_COMPLETE`를 받은 UI thread에서만 수행한다.
- 런처 버튼은 라이트/다크 테마 모두 owner-draw로 그리며, 아이콘과 텍스트를 하나의 내용 묶음으로 계산해 버튼 셀 중앙에 배치한다. 긴 라벨은 기존처럼 줄바꿈과 말줄임으로 제한한다.
- 버튼 아이콘 경로는 실행용 `path`를 먼저 현재 설정 기준 폴더와 직접 경로, `PATH/PATHEXT`에서 찾고, 실패하면 원본 스캔 경로인 `source_path`를 같은 방식으로 찾는다. 복사 버튼(`action=1`)은 실행 파일 아이콘 대상이 아니므로 아이콘 요청을 만들지 않는다.
- `SHGetFileInfoW`가 반환한 `HICON`은 변환 성공, 변환 실패, 기본 아이콘 판정 등 모든 경로에서 소유 guard가 `DestroyIcon`을 호출한다. `LoadIconW(IDI_APPLICATION)`처럼 공유 시스템 리소스로 받은 비교용 핸들은 파괴하지 않는다.
- 버튼 아이콘 캐시는 버튼 식별자, resolved path, 목표 크기를 키로 하며 아이콘이 없는 결과도 저장해 같은 잘못된 경로를 반복 추출하지 않는다. rendered icon cache는 source icon key와 목표 크기를 키로 하며, Win32 버튼 이미지 리스트를 저장한다.
- 아이콘 캐시는 최대 항목 수를 둔 LRU 유사 정책으로 갱신한다. cache hit는 최근 사용으로 갱신하고, 초과 시 가장 오래 사용하지 않은 항목부터 제거한다. rendered cache에서 제거된 이미지 리스트도 현재 버튼이 참조 중이면 `Arc`로 수명을 유지한다.
- Windows 기본 애플리케이션 아이콘과 동일한 비트맵만 얻은 경우에는 버튼 아이콘을 생략할 수 있다. 아이콘 추출 실패나 잘못된 경로는 버튼 표시 실패로 전파하지 않고 텍스트 버튼으로 남긴다.
- 종료 시 버튼 아이콘 worker에 shutdown을 요청하고 join한 뒤 버튼 HWND와 이미지 리스트 참조를 정리한다. shutdown 이후에는 현재 처리 중인 요청 1건만 마칠 수 있고, 큐에 남은 아이콘 요청 backlog는 처리하지 않는다.
- 우클릭 메뉴는 Edit, Open in Explorer, Hide를 제공한다. Edit는 버튼 이름/경로/인수와 실행 옵션을 저장하고, Hide는 folder tab 항목에만 적용한다.
- Win32 모달 팝업은 owner/main window를 비활성화한 상태로 표시하며, 표시 직전 owner 창의 현재 window rect 기준 중앙에 배치한다. 모달 루프는 `IsDialogMessageW`와 Escape 취소 처리를 적용해 표준 dialog 키 입력이 닫기 경로로 들어오게 한다. 닫기 요청 시 dialog를 먼저 숨기고, owner를 다시 활성화/전면 복구한 뒤 dialog HWND를 파괴해 owner가 disabled 상태나 뒤쪽 z-order에 남지 않게 한다. 모달 완료 플래그와 결과는 stack raw pointer가 아니라 단일 UI thread용 `ModalDialogState<T>`의 `Rc<Cell<_>>`/`RefCell<Option<_>>` 공유 상태로 유지해 release 최적화에서도 메시지 callback의 상태 변경이 모달 루프에 명확히 관측되게 한다.
- Win32 custom modal dialog는 `WM_NCCREATE`에서 dialog 상태 포인터를 `GWLP_USERDATA`에 저장한 뒤 `DefWindowProcW` 기본 처리를 통과시켜 title bar text와 non-client 초기화를 보존한다. 텍스트 입력, 탭 레이아웃, 버튼 편집 dialog는 표시 직후 키보드 입력이 첫 edit control로 들어가도록 첫 입력 칸에 명시적으로 focus를 둔다.
- 종료 시 현재 Win32 창 rect를 `WIDTHxHEIGHT+X+Y` 형식으로 저장하고 현재 DPI scale을 함께 기록한다. 저장 후 main window와 child controls의 redraw를 끄고 파괴해 child control teardown과 배경 erase frame이 사용자에게 보이지 않게 한다.
- 창 위치 좌표가 음수인 모니터 배치에서는 Tk geometry 규칙에 맞춰 `WIDTHxHEIGHT-X-Y`, `WIDTHxHEIGHT-X+Y`처럼 좌표 자체가 부호를 가진 문자열로 저장한다.
- HWND, HMENU, HICON, Win32 메시지 처리와 관련된 `unsafe`는 `ui/win32`의 message glue 또는 `platform/windows` 내부로 한정한다.

## Linux GTK4 UI 범위

Linux에서는 `ui/gtk4`가 GTK4 `ApplicationWindow`, `PopoverMenuBar`, `Notebook`, `Grid` 기반 버튼 화면을 구성한다. Windows와 동일한 `ConfigService`, `app::folder_tabs`, `app::tab_actions`, `app::button_layout`, `LauncherActionService` 흐름을 사용하고, GTK/GIO/Command가 필요한 창/메뉴/다이얼로그/실행/클립보드/파일 열기만 Linux UI 경계에 둔다.

- Linux 빌드는 target-specific dependency로 `gtk4` Rust 바인딩을 사용한다. Windows 빌드는 기존 Win32 구현과 `windows-sys` dependency를 유지한다.
- 메뉴는 File 아래에 Add Folder Tab, Add Tab, Set Current Tab Folder, Current Tab Layout, Rename/Delete Current Tab, Move/Select Tab, Sorting/Refresh/Reset Current Tab, Manage Hidden Items, Dark Theme, Exit를 두고, About 메뉴에 앱 버전, `https://github.com/edgarp9` 링크, 하단 스크롤의 `about.txt` 본문을 보여주는 about dialog를 둔다.
- GTK application 실행 중에는 메뉴/버튼/타이머 콜백이 참조하는 `GtkLauncher` 상태를 강한 참조로 보관한다. 개별 콜백은 재진입 안전성을 위해 `Weak`를 사용하되, top-level 실행 수명 동안 상태가 먼저 drop되어 사용자 이벤트가 무시되지 않게 한다.
- 단축키는 Win32와 같은 의미로 `Ctrl+Shift+Left/Right`, `Ctrl+PageUp/PageDown`, `F5`를 등록하고, GTK 메뉴 모델에도 accelerator hint를 제공한다. GTK accelerator 등록 목록은 메뉴 스펙에서만 생성하며, 실제 창 key event는 Win32 accelerator table과 같은 의미가 되도록 window capture-phase controller에서도 같은 command로 매핑한다.
- 폴더 선택, 텍스트 입력, 탭 레이아웃, 버튼 편집, 숨김 항목 관리, 확인/알림은 GTK dialog로 구현한다. 폴더 선택기는 Win32처럼 owner-modal로 열고, 탭 레이아웃과 숨김 항목 관리의 확인 버튼/닫기 흐름은 Win32 다이얼로그 문구와 동작을 기준으로 맞춘다. 삭제/리셋 같은 파괴적 확인은 Win32 `MB_DEFBUTTON2`와 같이 기본 응답을 취소/No로 둔다. 텍스트 입력/탭 레이아웃/버튼 편집/숨김 항목 관리 다이얼로그의 기본 크기와 GTK entry 입력 길이도 Win32 edit control 기준값에 맞춘다.
- 버튼 클릭은 공통 `LauncherActionService`를 통과한다. Linux 관리자 실행은 `pkexec`를 통해 OS 권한 상승 요청을 시작하고, `pkexec`가 없으면 관리자 실행 실패로 보고한다. Win32 `runas`와 같이 관리자 실행에는 실행 파일 parent working directory를 강제하지 않는다. GTK는 `pkexec` child를 nonblocking poll로 회수하고, 인증 취소/거부 결과만 사용자에게 다시 알린다. 인증 후 대상 프로그램의 일반 종료 코드는 Win32 `runas`처럼 버튼 실행 결과로 표시하지 않는다.
- Linux 실행은 실행 파일이면 `Command`로 시작하고, raw params는 Windows 호환 double-quote argv 규칙으로 인자화한다. 폴더, 연결 파일, URL/protocol 대상은 GIO 기본 앱 열기를 사용한다. 연결 파일과 URL/protocol 대상은 Windows `ShellExecuteW` 기준에 맞춰 raw params가 있어도 열기를 시도하지만, GIO 기본 앱 열기에는 params를 전달하지 않는다. URL/protocol 대상은 설정 기준 폴더와 결합하지 않고 raw URI를 보존한 뒤 기본 handler에 전달한다. Windows식 상대 경로 구분자는 설정 호환을 위해 Linux 실행 경계에서 `/`로 보정한다. Windows 전용 drive/UNC/drive-qualified 경로와 Windows형 미해결 `%VAR%` 경로는 직접 실행하지 않고 안내 메시지를 보여주며, POSIX 절대 경로 안의 literal `%...%` 이름은 Linux 경로로 허용한다. 클립보드 복사는 GTK clipboard를 사용하되, Win32 clipboard 경계처럼 내부 NUL 문자는 U+FFFD로 치환한다.
- Linux `Open in Explorer`는 폴더를 GIO 기본 앱으로 열고, 파일은 freedesktop `org.freedesktop.FileManager1.ShowItems`로 선택을 먼저 요청한다. 파일 관리자가 선택 요청을 지원하지 않거나 실패하면 Win32와의 차이를 안내하고 상위 폴더를 연다.
- 버튼 드래그 앤 드롭은 GTK drag source/drop target으로 같은 탭 내부 버튼 간 이동만 허용하며, 저장은 Win32와 같은 deferred save 경로를 사용한다. Win32와 같이 스캔 중이거나 종료 중이면 drag 시작과 drop을 무시하고, drag가 성립된 release가 버튼 실행 click으로 이어지지 않도록 억제한다. 버튼 context menu의 Edit, Open in Explorer, Hide 항목 순서와 활성 조건은 Win32/GTK가 같은 공통 스펙을 사용한다. 키보드 context menu는 Win32의 `WM_CONTEXTMENU` 키보드 경로처럼 버튼 중앙을 기준으로 열고 첫 활성 항목에 포커스를 둔다. GTK context popover는 `Esc` 닫기, `Up/Down/Home/End` 활성 항목 순환, `Enter/Space` 활성 항목 실행을 명시 처리해 Win32 popup menu keyboard loop에 맞춘다. GTK context popover는 Win32 popup menu의 one-shot 수명에 맞춰 새 popover, 탭 전환, 탭 rebuild, 창 close 전에 닫고 부모에서 분리하며, 사용자가 닫은 경우 활성 참조도 즉시 비운다. Context menu 콜백은 reentrant GTK 이벤트 중에도 패닉하지 않도록 non-panicking borrow 경로를 사용하고, stale tab/button 대상이면 popover 자체를 열지 않는다.
- 버튼 grid는 Win32와 같이 설정된 `rows * cols`와 실제 최대 슬롯을 기준으로 빈 슬롯까지 유지한다. GTK는 빈 슬롯 placeholder를 붙여 버튼 수가 적거나 숨김 항목이 있어도 셀 크기가 Windows처럼 전체 슬롯 기준으로 계산되게 하며, Win32의 48x36 logical px cell floor와 light/dark theme 간격 규칙(light 6px, dark 0px)을 따른다. Grid가 viewport보다 커질 때도 Win32 tab client처럼 스크롤 없이 클리핑한다.
- 버튼 편집 대화상자는 Win32 edit control 기준에 맞춰 `OK`를 기본 응답으로 두고 Enter로 확정되며, 첫 Name entry에 명시적으로 focus를 두고 Name/Path/Params 입력 길이는 32,767자로 제한한다. Path 오른쪽의 파일 선택 버튼은 owner-modal GTK 파일 선택기를 열고, 로컬 파일 선택이 확인되면 Path entry에 선택 경로를 반영한다.
- 버튼 아이콘은 GTK symbolic icon을 사용하되, Win32처럼 Copy Path + Params 버튼은 아이콘을 생략한다. 버튼 이름, 스캔 이름, 실행 경로, 스캔 원본 경로가 모두 비어 있는 정보 없는 버튼도 아이콘을 생략한다. 실행 파일 표시는 설정 파일 base dir 기준으로 해석한 Unix 실행 권한 또는 알려진 실행 파일 확장자(`.exe`, `.cmd`, `.sh`, `.appimage` 등)로만 판단해 일반 문서 파일을 실행 파일처럼 표시하지 않는다.
- 폴더 스캔은 Win32와 같은 `infra::folder_scan` 규칙을 사용하고, GTK UI thread는 polling으로 scan worker 결과를 반영한다. Linux에서 Windows 전용 drive/UNC/drive-qualified 폴더 경로나 Windows형 미해결 `%VAR%` 폴더 경로가 저장된 탭은 refresh/reset worker를 시작하지 않고 안내 메시지를 보여준다. 허용된 폴더 경로는 guard와 worker I/O가 같은 값을 보도록 환경 변수 확장 후 Windows식 상대 구분자를 `/`로 보정한다. Deferred save 결과는 Win32 `WM_CONFIG_SAVE_COMPLETE`와 같은 역할의 GTK 내부 action을 메인 루프에 큐잉해 즉시 반영하고, 주기 polling은 fallback으로 유지한다.
- 종료 중 active scan은 Win32 close 경로처럼 취소만 요청하고 UI thread에서 worker join을 기다리지 않는다. 정상 scan 완료는 poll 경로에서 결과 수신 후 join한다.
- Dark Theme는 `Window.DarkTheme`에 저장하고 GTK application dark theme preference에 반영한다.
- Linux 창 geometry는 GTK4/Wayland의 창 위치 조회 제한 때문에 종료 시 `WIDTHxHEIGHT` 크기만 저장한다. 시작 시 크기 구분자는 Win32 parser와 같이 `x`/`X`를 모두 허용한다. Windows가 저장한 DPI scale이 있으면 Linux 시작 시 1.0 기준 크기로 보정하고, 이 보정도 `x`/`X` 구분자를 모두 처리한다. Linux 종료 시 `Window.DpiScale=1.0`으로 갱신해 반복 스케일링을 막는다. Windows는 기존처럼 위치와 실제 DPI scale까지 저장한다. 종료 중 geometry 저장 실패는 Win32처럼 warning으로 표시한다.

## Windows/Linux 기능 비교

| 기능 | Windows Win32 | Linux GTK4 | 차이 |
| --- | --- | --- | --- |
| 설정 파일 열기/seed/백업/원자 저장/deferred save | 지원 | 지원 | Linux 저장 잠금은 `fs2` advisory lock을 사용한다. |
| 창 표시, 탭, 버튼 grid | 지원 | 지원 | visible slot 계산은 `ui/common` 공유 규칙을 사용한다. |
| Add Folder Tab / duplicate folder focus | 지원 | 지원 | 동일 유스케이스 사용. |
| Add Manual Tab | 지원 | 지원 | 동일 유스케이스 사용. |
| Set Current Tab Folder | 지원 | 지원 | 동일 유스케이스 사용. |
| Current Tab Layout | 지원 | 지원 | Win32와 같은 rows/cols 텍스트 입력 검증을 적용한다. |
| Rename/Delete Current Tab | 지원 | 지원 | 동일 유스케이스 사용. |
| Move/Select Tab 및 단축키 | 지원 | 지원 | 동일 유스케이스 사용. |
| Sorting/Refresh/Reset Current Tab | 지원 | 지원 | 동일 스캔/탭 병합 규칙 사용. |
| Manage Hidden Items / Hide | 지원 | 지원 | 동일 유스케이스 사용. |
| 버튼 Edit | 지원 | 지원 | Name/Path/Params/Admin/Copy Path + Params를 저장한다. |
| 버튼 클릭 실행 | `ShellExecuteW` | executable은 `Command`, 폴더/연결 파일/URL protocol은 GIO | Linux raw params는 실행 파일일 때 Windows 호환 double-quote argv parser로 인자화하고 Windows식 경로 백슬래시는 literal로 보존한다. 연결 파일과 URL/protocol 대상의 params는 GIO에 전달하지 않지만 Windows처럼 열기 시도 자체는 막지 않는다. Windows 전용 drive/UNC/drive-qualified 경로와 Windows형 미해결 `%VAR%` 경로는 안내 후 실행하지 않고, POSIX 절대 literal `%...%` 경로는 허용한다. |
| Run as administrator | `ShellExecuteW runas` | `pkexec` 권한 상승 요청, 없으면 관리자 실행 실패 | `pkexec` 실행 시 Win32 `runas`처럼 별도 working directory를 강제하지 않는다. GTK main loop가 `pkexec` child를 nonblocking으로 추적해 인증 취소/거부 결과를 보고하고, 인증 후 대상 프로그램 종료 코드는 버튼 실행 결과로 표시하지 않는다. 실제 prompt 표시 방식은 OS policy agent에 의존한다. |
| Copy Path + Params | Win32 clipboard | GTK clipboard | 동일 사용자 동작. |
| Open in Explorer | 폴더 열기/파일 선택 | 폴더 열기/파일은 FileManager1 선택 요청 후 fallback으로 상위 폴더 열기 | 파일 관리자가 FileManager1 선택 요청을 지원하지 않으면 상위 폴더 열기로 남는다. |
| About | 상단에 앱 버전과 `https://github.com/edgarp9` URL 버튼, 하단 스크롤에 `about.txt` 본문 표시, `ShellExecuteW`로 기본 브라우저 열기 | 상단에 앱 버전과 `https://github.com/edgarp9` `LinkButton`, 하단 스크롤에 `about.txt` 본문 표시, GIO 기본 URI handler로 브라우저 열기 | About 메뉴는 File 메뉴와 분리된 최상위 메뉴로 둔다. |
| 버튼 아이콘 | `SHGetFileInfoW` 시스템 아이콘, Copy 버튼 아이콘 생략 | GTK symbolic icon + 실행 파일 hint, Copy/정보 없는 버튼 아이콘 생략 | Linux는 파일별 시스템 아이콘 추출 캐시와 완전 동일하지 않지만 일반 문서 파일을 실행 파일로 오분류하지 않고 상대 실행 파일은 설정 base dir 기준으로 판단한다. |
| Dark Theme | DWM titlebar 및 owner-draw controls | GTK dark theme preference와 Win32 기준 dark grid gap | 툴킷 렌더링 차이는 남는다. |
| 창 geometry 저장 | 크기와 위치, DPI scale | 크기, DpiScale=1.0 | GTK4/Wayland 위치 조회 제한. Windows 저장 DPI는 Linux 시작 시 1.0 기준으로 보정하고 `x`/`X` 크기 구분자를 모두 읽는다. |

## 설정 정규화 규칙

- `Window.Geometry`가 없거나 비문자열이면 `800x600`으로 복구한다.
- `Window.DarkTheme`은 boolean/숫자/문자열 true 값만 `true`로 정규화하고, 값이 없거나 알 수 없으면 `false`로 복구한다.
- `FolderTabs`는 최대 50개까지만 유지한다.
- 탭 `id`가 비어 있거나 중복되면 새 `tab-N` 형식의 id로 보정한다.
- `rows`는 1 이상 500 이하, `cols`는 1 이상 32 이하로 정규화한다. 파싱할 수 없는 값은 탭 타입별 기본값인 3행 8열로 복구한다.
- `manual` 탭의 버튼 수가 `rows * cols`보다 적으면 빈 버튼으로 채운다.
- `folder` 탭에서 중복된 버튼 `item_id`는 첫 항목만 유지하고 뒤 항목은 제거한다.
- `hidden_item_ids`는 빈 값과 중복 값을 제거한다.
- `slot_positions`는 `folder` 탭의 유효한 버튼 `item_id`에 대한 0 이상 슬롯 번호만 유지한다.
- `scan_signature`는 `folder` 탭의 `folder_path`와 같은 경로를 가리키고 `version=1`, `mtime_ns`, `ctime_ns`, `size`가 0 이상일 때만 유지한다.
- `scan_item_order`는 `scan_signature`가 유효하고 모든 버튼 `item_id`를 중복 없이 정확히 포함할 때만 유지한다.

## 버튼 실행 경로 정책

- 버튼 실행 경로와 탐색기 열기 경로는 환경 변수를 확장한 뒤 처리한다.
- `my\tool\tool.exe`처럼 경로 구분자를 포함한 상대 경로는 설정 기준 폴더 아래의 절대 경로로 해석한다.
- Linux에서는 Windows식 상대 구분자(`\`)를 `/`로 바꿔 Windows 설정 파일의 상대 경로를 같은 폴더 구조에서 사용할 수 있게 한다.
- `CommandTimer.exe`처럼 파일명만 있는 경로는 설정 기준 폴더에 같은 파일이 있으면 그 파일을 우선 실행하고, 없으면 기존처럼 Windows shell/PATH 조회에 맡긴다.
- 절대 경로, UNC 경로, Windows drive-qualified 경로는 설정 기준 폴더와 결합하지 않는다. Linux에서 Windows drive/UNC/drive-qualified 경로와 Windows형 미해결 `%VAR%` 경로는 직접 실행/열기하지 않고 사용자 안내로 종료한다. POSIX 절대 경로 안의 literal `%...%` 파일명은 Linux 경로로 허용한다.

## 설정 저장 정책

- Rust `ConfigStore`는 기준 폴더 바로 아래의 기본 `j3Launcher.json` 또는 명시된 설정 파일 경로를 읽고 저장한다.
- 현재 Win32/GTK UI는 실행 인자가 없으면 `ConfigService::open_from_executable_or_current_dir()`로 기본 설정 파일을 사용한다. 첫 번째 실행 인자가 있으면 `ConfigService::open_path_from_executable_or_current_dir()`로 해당 파일을 사용하며, 상대 경로 인자는 실행 파일이 있는 폴더를 우선 기준으로 해석한다. 실행 파일 경로를 확인할 수 없을 때만 프로세스의 현재 작업 디렉터리로 대체한다. 표준 OS 설정 경로는 사용하지 않는다.
- 선택된 설정 파일이 없으면 같은 폴더의 `j3Launcher_win.json`을 초기 seed로 사용하고, seed가 없으면 기본 설정(`Window.Geometry=800x600`, 빈 `FolderTabs`)을 생성한다.
- JSON 읽기는 UTF-8과 UTF-8 BOM을 모두 허용한다. 저장은 UTF-8 JSON으로 수행한다.
- 선택된 설정 파일이 JSON으로 파싱되지 않으면 원본 바이트를 `<설정파일>.bak` 계열 파일로 백업한 뒤 기본 설정 파일로 복구한다.
- `j3Launcher_win.json` seed가 JSON으로 파싱되지 않으면 seed를 무시하고 기본 설정으로 초기화한다.
- 저장은 같은 폴더에 임시 파일을 만들고 전체 JSON을 기록한 뒤 `flush`, `sync_all`, 원자적 교체 순서로 수행한다.
- 저장 중에는 선택된 설정 파일 이름에 `.lock`을 붙인 파일을 통해 프로세스 간 저장 경합을 막고, 저장 완료 후 제거한다.
- Windows에서는 기존 stale lock 파일이 남아 있어도 파일 핸들 기반 단독 잠금을 다시 획득할 수 있으면 저장을 계속하고, 실제로 다른 저장자가 잠금을 잡고 있을 때만 저장을 거부한다.
- Unix/Linux에서는 advisory lock inode가 갈라지는 split-lock race를 피하기 위해 `.lock` 파일을 저장 후 삭제하지 않는다. 기존 unheld lock 파일이 남아 있어도 exclusive lock 획득이 가능하면 저장을 계속한다.
- 저장 직전에는 마지막 로드/저장 시점의 파일 signature와 현재 파일 metadata/hash를 비교한다. 파일이 외부에서 바뀐 경우 저장을 거부한다.
- `save_window_geometry`, `set_folder_tabs`, `set_button_info`는 저장이 성공한 경우에만 메모리 상태를 교체한다. 잠금 실패, 쓰기 실패, stale store 충돌이 발생하면 기존 메모리 상태를 유지한다.
- `ConfigService::set_folder_tabs_deferred`는 저장 snapshot을 worker에 보내고 즉시 in-memory 설정을 갱신한다. worker는 큐에 쌓인 저장 요청을 coalescing하여 최신 snapshot만 파일에 쓴다.
- deferred 저장 결과는 `drain_deferred_save_results`에서만 반영한다. 성공한 저장은 파일 signature와 committed snapshot을 갱신하고, 최신 sequence 저장 실패는 마지막 committed snapshot으로 in-memory 설정을 롤백한다. 최신이 아닌 실패는 새 저장 요청을 보호하기 위해 롤백하지 않는다.
- 종료 또는 동기 저장 전에는 `shutdown_deferred_save_worker`로 pending deferred save를 먼저 처리한다. shutdown sentinel은 이미 큐에 들어간 저장 뒤에서 처리되며, worker는 pending 요청 중 최신 snapshot을 저장한 뒤 join된다.
- GTK 종료 시에는 Linux에서 새 창 위치를 안정적으로 읽을 수 없으므로 현재 창 크기와 `DpiScale=1.0`을 저장하되, 기존 geometry의 유효한 `+x+y` 또는 `-x+y` 위치 suffix는 보존한다.

## 릴리스 회귀 기준

- 원본 `j3Launcher_win.json` 회귀는 `tests/fixtures/j3Launcher_win.json`의 익명화된 fixture로 검증한다.
- 런타임 경로에서 패닉성 `unwrap()`/`expect()`는 사용하지 않는다. 현재 검색 결과의 `expect()`는 테스트 코드에만 남아 있다.
- Win32 창 생성은 `CreateWindowExW`가 보내는 동기 생성 메시지 중 실패해도 `Box` 소유권을 한 번만 회수하도록 생성 상태 포인터를 별도로 전달한다.
- `windows-sys` feature는 현재 Win32 호출에 필요한 범위로 유지한다. `Win32_UI_Shell_Common`은 폴더 선택 대화상자의 `BROWSEINFOW`/`SHBrowseForFolderW`에 필요하다.
- 필수 외부 dependency는 `serde`, `serde_json`, `windows-sys`다.
- 소스 수정 후 최소 검증은 `cargo fmt --check`, `cargo test`, `cargo check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build --release`를 기준으로 한다.
- Linux 데스크톱 입력 회귀는 `tests/linux_ui_smoke.sh`에서 X11 백엔드와 deterministic GTK 입력 모듈(`GTK_IM_MODULE=gtk-im-context-simple`)로 disabled accelerator no-mutation, enabled accelerator, 리사이즈/포커스 복귀 후 반복 accelerator, File 메뉴 Move/Select/Sort 실제 클릭, Select Prev/Next, Sort, Refresh, active scan 중 Exit와 Dark Theme 및 mutating accelerator/drag no-op, Reset default-No와 Yes 흐름, Manage Hidden Items, keyboard context menu activation, repeated context menu dismiss/reopen, pointer context menu activation, button drag/drop, context menu Edit save와 checkbox Enter default-OK, context menu Open in Explorer Windows-path guard, context menu Open in Explorer folder open through an isolated fake directory desktop handler, context menu Hide, button launch with params, button admin launch through fake `pkexec`, fake `pkexec` cancellation dialog, button associated-file open through an isolated fake desktop handler, button URL/protocol open through an isolated fake `x-scheme-handler` desktop handler, button Copy Path + Params followed by GTK paste, File 메뉴 Add Folder Tab selected-path/cancel/error/duplicate-focus flow through the debug picker override, Add Tab, Set Current Tab Folder selected-path/cancel/error/duplicate-focus flow through the debug picker override, Tab Layout edit/save, Rename edit/save, modal dialog main-accelerator isolation, Delete default-No와 Yes 삭제 흐름, Dark Theme, Exit를 확인한다. `APP=target/release/j3launcher J3LAUNCHER_UI_SMOKE_SKIP_DEBUG_ONLY=1 tests/linux_ui_smoke.sh`는 debug-only picker override와 delayed active-scan marker 흐름을 제외한 release-compatible subset을 실제 release 바이너리로 검증한다. 테스트는 기존 `xdotool`, `jq`, `update-desktop-database`를 사용하며, 새로 띄운 앱 PID와 X11 window PID를 맞춰 외부/잔여 `j3Launcher` 창에 입력을 보내지 않는다. File 메뉴 자동화는 GTK popover 포커스 race를 줄이기 위해 메뉴 open 후 분리된 `Home` 입력과 짧은 key delay를 사용한다. fake desktop handler 출력은 임시 파일 후 rename으로 기록해 비동기 open 타이밍에서 부분 파일을 읽지 않는다. 성공 경로 끝에서 예상 밖의 `CRITICAL`/`ERROR`/panic 앱 로그를 검사한다. 실패 분석이 필요할 때 `J3LAUNCHER_UI_SMOKE_DUMP_LOGS=1`로 전체 앱 로그를 출력한다.
- Windows 데스크톱 명령 회귀는 Windows 환경에서 `powershell -ExecutionPolicy Bypass -File tests/windows_ui_smoke.ps1 -AppPath target\debug\j3launcher.exe`로 실행한다. 이 harness는 Win32 메뉴 클릭이 보내는 `WM_COMMAND` 경로로 Add Folder Tab/Set Current Tab Folder의 실제 native picker selected-folder scan/save와 cancel/no-mutation/owner 복귀 흐름을 확인하고, error/duplicate-focus flow는 debug-only picker override(`J3LAUNCHER_TEST_PICK_FOLDER`, `J3LAUNCHER_TEST_PICK_FOLDER_ERROR`)로 검증한다. 실제 Win32 `HMENU`도 `GetMenu`/`GetMenuState`로 읽어 File 메뉴 label/order/separator/accelerator text, 수동 탭과 폴더 탭의 enabled state, Dark Theme checked state를 확인한다. Add Tab, disabled single-tab move/select no-op, disabled manual-tab folder-command no-dialog/no-mutation, Move/Select, Rename, Tab Layout, Delete default-No/Yes, Sort/Refresh/Reset default-No/Yes, Manage Hidden Items dialog open, Dark Theme, Exit도 상태 파일과 dialog 결과로 검증한다. State restore smoke는 resize/Dark Theme/tab move/add 후 Exit하고 같은 config로 재시작해 saved geometry/theme/tab order가 시작 경로에서 복원되고 후속 메뉴 명령을 받는지 확인한다. 메인 창 layout smoke는 초기/resize/dark-theme 후 main window, tab control, button rect가 visible이고 parent 안에 있는지 확인하며, dialog layout smoke는 Rename, Tab Layout, Manage Hidden Items, Edit Button controls가 dialog 안에 표시되는지 확인한다. 또한 debug-only context command override(`J3LAUNCHER_TEST_CONTEXT_MENU_COMMAND`)로 `WM_CONTEXTMENU` 이후 Edit 저장, Hide 저장, missing target Open in Explorer 오류/복귀 flow와 실제 Explorer folder success/owner 복귀 flow를 검증하고, Win32 drag event/cursor 경계로 button drag swap 저장을 확인하며, admin directory target 오류/복귀, resize/focus 후 `F5` accelerator, 버튼 컨트롤 `BM_CLICK` 기반 Copy clipboard, 격리된 임시 `.cmd` launch marker를 확인한다. 각 case는 앱 stdout/stderr를 별도 log file로 redirect하고 성공 종료 후 panic/ERROR/CRITICAL 같은 unexpected diagnostic pattern이 없는지 검사한다. release-compatible subset은 `cargo build --release` 후 `powershell -ExecutionPolicy Bypass -File tests/windows_ui_smoke.ps1 -AppPath target\release\j3launcher.exe -SkipDebugOnly`로 실행하며 debug-only picker/context override case만 제외한다. 새로 띄운 앱 PID와 Win32 window PID를 맞춰 기존 `j3Launcher` 창이나 다른 프로세스의 같은 제목 dialog에 입력을 보내지 않는다. 실제 Explorer folder success smoke는 temp 폴더를 열고 Shell COM으로 해당 Explorer window를 확인한 뒤 닫는다. `-IncludePhysicalInput`은 실제 포인터 context/drag probe를 추가로 실행하는 실험 옵션이며, 현재 자동화 세션에서는 `SendInput`이 popup/drag 경로에 안정적으로 도달하지 않아 기본 회귀 게이트에서 제외한다. 실제 UAC runas prompt/승인 경로와 실제 물리 포인터 기반 context menu/drag/drop은 별도 수동 검증 대상으로 남긴다.
