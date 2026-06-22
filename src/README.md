# j3Launcher Rust Port

j3Launcher의 Rust 네이티브 포팅 프로젝트입니다. Windows에서는 기존 Win32 UI를 사용하고, Linux에서는 GTK4 UI를 사용합니다. 원본 Python/Tkinter 프로젝트는 참조용으로만 둡니다.

## 요구 사항

- Windows 10/11 또는 Linux 데스크톱
- Rust 1.88 이상
- Windows: MSVC 기반 Rust toolchain
- Linux: GTK4 개발 패키지(`gtk4`, `pkg-config`)

## 빌드

```powershell
cargo fmt --check
cargo test
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

릴리스 실행 파일은 `target\release\j3launcher.exe`에 생성됩니다.
릴리스 전 검증 기준은 위 명령 전체를 순서대로 통과하는 것입니다.
릴리스 배포물에는 [LICENSE](LICENSE), [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt), [about.txt](about.txt)를 함께 포함해야 합니다.
바이너리를 배포할 때는 GPL-3.0-or-later의 Corresponding Source 제공 경로도 같은 배포 채널 또는 별도 안내로 제공해야 합니다.
`build_release.py`는 release 폴더에 이 고지 파일들을 복사하고, 같은 버전의 소스 ZIP과 현재 플랫폼 바이너리 ZIP을 생성합니다.
포함 아이콘(`icon.svg`, 파생 `icon.png`/`icon.ico`)은 Google Material Symbols `apps` 아이콘 기반 Apache-2.0 리소스로 감사되었으며, 정확한 프로젝트 반입 이력은 `THIRD_PARTY_NOTICES.txt`의 확인 필요 항목으로 남겨져 있습니다.

Linux에서 같은 검증 명령을 사용할 수 있습니다. Windows cross-check 환경에서 리소스 컴파일러가 없고 Rust 코드 컴파일만 확인하려면 다음처럼 리소스 임베드를 명시적으로 건너뛸 수 있습니다.

```bash
J3LAUNCHER_SKIP_WINDOWS_RESOURCES=1 cargo check --target x86_64-pc-windows-msvc
```

## 실행

```powershell
cargo run
```

또는 빌드 후:

```powershell
.\target\release\j3launcher.exe
.\target\release\j3launcher.exe 111.json
.\target\release\j3launcher.exe 222.json
```

Linux/GTK4에서 Wayland 기본 `W` 아이콘이 보이면 GTK 코드의 직접 아이콘 설정만으로는 데스크톱 환경이 앱 아이콘을 매핑하지 못한 상태일 수 있습니다. 이때는 application id와 같은 이름의 desktop entry를 만들고, 같은 이름의 SVG 아이콘을 hicolor icon theme에 설치합니다.

릴리스 바이너리에서 사용자 영역에 등록하려면:

```bash
./target/release/j3launcher --install
```

개발 빌드를 등록하려면:

```bash
./target/debug/j3launcher --install
```

등록 제거는 같은 바이너리에서 실행합니다.

```bash
./target/debug/j3launcher --uninstall
```

`--install`은 `icon.svg`가 있으면 SVG를 우선 설치하고, 없으면 `icon.png`로 fallback합니다. 중복 실행해도 같은 desktop entry와 아이콘이 이미 설치되어 있으면 다시 쓰지 않고 성공합니다. KDE taskbar의 lowercase fallback 매칭을 위해 `io.github.edgarp9.j3launcher.desktop` alias도 `NoDisplay=true`로 함께 설치합니다. 수동으로 확인하거나 복구해야 한다면 아래와 같은 파일이 만들어져야 합니다.

Linux 배포 파일은 실행 파일, `icon.svg`, 그리고 fallback용 `icon.png`를 함께 두는 것을 권장합니다.

```bash
app_dir="$(pwd)"
mkdir -p ~/.local/share/applications
mkdir -p ~/.local/share/icons/hicolor/scalable/apps

install -m 0644 "$app_dir/icon.svg" \
  ~/.local/share/icons/hicolor/scalable/apps/io.github.edgarp9.j3Launcher.svg

cat > ~/.local/share/applications/io.github.edgarp9.j3Launcher.desktop <<EOF
# Managed by io.github.edgarp9.j3Launcher --install
[Desktop Entry]
Type=Application
Name=io.github.edgarp9.j3Launcher
Comment=io.github.edgarp9.j3Launcher
Exec=$app_dir/target/release/j3launcher
Icon=io.github.edgarp9.j3Launcher
Terminal=false
Categories=Utility;
StartupNotify=true
StartupWMClass=io.github.edgarp9.j3Launcher
EOF

install -m 0644 "$app_dir/icon.svg" \
  ~/.local/share/icons/hicolor/scalable/apps/io.github.edgarp9.j3launcher.svg

cat > ~/.local/share/applications/io.github.edgarp9.j3launcher.desktop <<EOF
# Managed by io.github.edgarp9.j3Launcher --install
[Desktop Entry]
Type=Application
Name=io.github.edgarp9.j3Launcher
Comment=io.github.edgarp9.j3Launcher
Exec=$app_dir/target/release/j3launcher
Icon=io.github.edgarp9.j3launcher
Terminal=false
Categories=Utility;
StartupNotify=true
StartupWMClass=io.github.edgarp9.j3launcher
NoDisplay=true
EOF

update-desktop-database ~/.local/share/applications
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
kbuildsycoca6 --noincremental 2>/dev/null || kbuildsycoca5 --noincremental 2>/dev/null || true
```

## 설정 파일

현재 Win32 UI는 실행 파일이 있는 디렉터리의 설정 파일을 읽고 저장합니다. 실행 인자가 없으면 `j3Launcher.json`을 사용하고, 첫 번째 실행 인자가 있으면 해당 파일을 설정 파일로 사용합니다. 상대 경로 인자는 실행 파일이 있는 디렉터리를 기준으로 해석합니다.

- 선택된 설정 파일이 없으면 같은 디렉터리의 `j3Launcher_win.json`을 초기 seed로 사용합니다.
- seed도 없으면 기본 설정(`800x600`, 빈 탭 목록)을 생성합니다.
- 저장 중에는 선택된 설정 파일 이름에 `.lock`을 붙인 잠금 파일을 만들고, 임시 파일 기록 후 교체합니다.
- 창 위치는 `WIDTHxHEIGHT+X+Y` 형식으로 저장하며, 왼쪽/위쪽 모니터처럼 좌표가 음수인 경우 `WIDTHxHEIGHT-X-Y`처럼 부호를 유지합니다.
- `my\app\app.exe`처럼 경로 구분자를 포함한 상대 실행 경로는 실행 파일이 있는 디렉터리를 기준으로 해석합니다. Linux에서도 Windows식 상대 구분자(`\`)는 설정 호환을 위해 `/`로 보정합니다.
- 파일명만 있는 실행 경로는 같은 디렉터리에 파일이 있으면 그 파일을 우선 사용하고, 없으면 Windows shell/PATH 조회에 맡깁니다.
- `C:\...`, UNC, 미해결 `%VAR%`처럼 Windows 전용으로 남은 경로는 Linux에서 직접 실행하지 않고 안내 메시지를 표시합니다.

테스트 fixture는 `tests/fixtures/j3Launcher_win.json`에 있으며, 원본 Windows 설정의 구조만 남기고 개인 경로는 제거했습니다.

Windows/Linux 동작 점검 기록은 [docs/linux-parity-report.md](docs/linux-parity-report.md)에 정리합니다. 현재 기록에는 자동 검증과 정적 대조 결과가 포함되며, 전체 데스크톱 수동 병행 검증은 별도 남은 항목으로 추적합니다.

## 플랫폼 UI 범위

Windows는 기존 Win32 네이티브 UI를 유지합니다.

- Win32 top-level window, menu, tab control, button grid
- `ShellExecuteW` 기반 실행 및 관리자 실행
- Windows 폴더 선택 대화상자
- DPI awareness, DWM dark titlebar
- `SHGetFileInfoW` 기반 버튼 아이콘 추출
- 버튼별 `Path + Params` 클립보드 복사

Linux는 GTK4 네이티브 UI를 제공합니다.

- GTK4 top-level window, File 메뉴, notebook tab, button grid
- GTK dialog 기반 폴더 선택/입력/편집/확인/숨김 항목 관리
- GIO 기반 폴더/파일 열기와 GTK clipboard 복사
- 같은 `app`/`domain` 유스케이스를 통한 탭 추가, 수동 탭, 폴더 변경, 레이아웃 변경, 이름 변경, 삭제, 이동, 정렬, refresh, reset, hide/unhide, 버튼 드래그 이동

## 알려진 제한

- 설정 파일 위치는 표준 OS 설정 디렉터리가 아니라 실행 파일 디렉터리 또는 실행 인자로 지정한 파일입니다.
- 손상된 설정 파일 백업/복구 UI는 원본보다 단순합니다.
- 실행 중 외부에서 설정 파일이 바뀌면 저장 충돌을 감지하고 현재 저장을 거부하지만, 자동 병합은 하지 않습니다.
- Linux 파일 관리자는 Windows Explorer의 파일 선택 명령과 같은 표준 API가 없어 파일의 상위 폴더를 엽니다.
- Linux에서 연결 파일은 Windows처럼 파라미터가 있어도 기본 앱 열기를 시도하지만, GIO 기본 앱 열기에는 해당 파라미터를 전달하지 않습니다.
- GTK4/Wayland에서는 창 위치 조회가 제한되어 Linux 종료 시 창 크기만 저장합니다. Windows에서 저장한 DPI 배율이 있으면 Linux 시작 시 1.0 기준 크기로 보정하고, Linux 종료 시 `DpiScale=1.0`으로 갱신합니다.
- GTK4/Wayland의 앱 전환기/작업 관리자 아이콘은 창 titlebar 아이콘과 별개이며, 컴포지터 정책에 따라 application id와 `.desktop` 파일 매핑이 필요할 수 있습니다.
- Linux 버튼 아이콘은 GTK symbolic icon을 사용하며 Windows의 `SHGetFileInfoW` 기반 파일별 시스템 아이콘 캐시와 완전히 같지는 않습니다.
- Win32/GTK4 UI 동작은 실제 데스크톱에서 수동 스모크 검증이 필요합니다.
