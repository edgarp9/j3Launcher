param(
    [string]$AppPath = "target\debug\j3launcher.exe",
    [int]$TimeoutSeconds = 8,
    [string]$WorkDir = "",
    [string[]]$Only = @(),
    [switch]$IncludePhysicalInput,
    [switch]$SkipDebugOnly
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
if (-not [System.IO.Path]::IsPathRooted($AppPath)) {
    $AppPath = Join-Path $RepoRoot $AppPath
}
if (-not (Test-Path -LiteralPath $AppPath)) {
    throw "app binary does not exist: $AppPath"
}

$SkipDebugOnlyCases = [bool]$SkipDebugOnly
if (-not $SkipDebugOnlyCases -and ($env:J3LAUNCHER_UI_SMOKE_SKIP_DEBUG_ONLY -match '^(1|true|yes|on)$')) {
    $SkipDebugOnlyCases = $true
}

if ($WorkDir -eq "") {
    $WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ("j3launcher-win-smoke-" + [System.Guid]::NewGuid().ToString("N"))
}
New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class WinSmokeUser32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool EnumChildWindows(IntPtr hWnd, EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SetFocus(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool IsWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool GetWindowRect(IntPtr hWnd, out WinSmokeRect lpRect);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode, EntryPoint = "SendMessageW")]
    public static extern IntPtr SendMessageText(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr GetMenu(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr GetSubMenu(IntPtr hMenu, int nPos);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern int GetMenuItemCount(IntPtr hMenu);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode, EntryPoint = "GetMenuStringW")]
    public static extern int GetMenuString(IntPtr hMenu, uint uIDItem, StringBuilder lpString, int nMaxCount, uint uFlag);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint GetMenuState(IntPtr hMenu, uint uId, uint uFlags);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint SendInput(uint nInputs, WinSmokeInput[] pInputs, int cbSize);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern int GetSystemMetrics(int nIndex);
}

[StructLayout(LayoutKind.Sequential)]
public struct WinSmokeRect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
}

[StructLayout(LayoutKind.Sequential)]
public struct WinSmokeInput {
    public uint type;
    public WinSmokeInputUnion U;
}

[StructLayout(LayoutKind.Explicit)]
public struct WinSmokeInputUnion {
    [FieldOffset(0)]
    public WinSmokeMouseInput mi;
}

[StructLayout(LayoutKind.Sequential)]
public struct WinSmokeMouseInput {
    public int dx;
    public int dy;
    public uint mouseData;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;
}
"@

$WM_COMMAND = 0x0111
$WM_CONTEXTMENU = 0x007B
$WM_KEYDOWN = 0x0100
$WM_BUTTON_DRAG_EVENT = 0x8004
$BUTTON_DRAG_EVENT_DOWN = 1
$BUTTON_DRAG_EVENT_MOVE = 2
$BUTTON_DRAG_EVENT_UP = 3
$WM_SETTEXT = 0x000C
$BM_CLICK = 0x00F5
$EM_SETSEL = 0x00B1
$EM_REPLACESEL = 0x00C2
$GWL_STYLE = -16
$BS_DEFPUSHBUTTON = 0x00000001
$MF_BYCOMMAND = 0x00000000
$MF_BYPOSITION = 0x00000400
$MF_GRAYED = 0x00000001
$MF_DISABLED = 0x00000002
$MF_CHECKED = 0x00000008
$MF_SEPARATOR = 0x00000800
$IDOK = 1
$IDCANCEL = 2
$IDYES = 6
$IDNO = 7
$ID_EDIT_NAME = 20001
$ID_EDIT_PATH = 20002
$ID_EDIT_PARAMS = 20003
$ID_EDIT_OK = 20007
$ID_TEXT_INPUT = 20102
$ID_TEXT_OK = 20103
$ID_LAYOUT_ROWS = 20201
$ID_LAYOUT_COLS = 20202
$ID_LAYOUT_APPLY = 20203
$ID_HIDDEN_CLOSE = 20303
$ID_ABOUT_NAME = 20401
$ID_ABOUT_VERSION = 20402
$ID_ABOUT_LINK = 20403
$ID_ABOUT_CLOSE = 20404
$VK_F5 = 0x74
$VK_RETURN = 0x0D
$INPUT_MOUSE = 0
$MOUSEEVENTF_MOVE = 0x0001
$MOUSEEVENTF_LEFTDOWN = 0x0002
$MOUSEEVENTF_LEFTUP = 0x0004
$MOUSEEVENTF_RIGHTDOWN = 0x0008
$MOUSEEVENTF_RIGHTUP = 0x0010
$MOUSEEVENTF_ABSOLUTE = 0x8000
$MOUSEEVENTF_VIRTUALDESK = 0x4000
$SM_XVIRTUALSCREEN = 76
$SM_YVIRTUALSCREEN = 77
$SM_CXVIRTUALSCREEN = 78
$SM_CYVIRTUALSCREEN = 79
$Menu = @{
    AddFolderTab = 100
    AddManualTab = 101
    SetTabFolder = 102
    TabLayout    = 103
    RenameTab    = 104
    DeleteTab    = 105
    MoveLeft     = 106
    MoveRight    = 107
    SelectPrev   = 108
    SelectNext   = 109
    Sort         = 110
    Refresh      = 111
    Reset        = 112
    ManageHidden = 113
    DarkTheme    = 114
    Exit         = 115
    About        = 116
}

$script:RunAppCaseCount = 0

function Write-JsonFile {
    param([string]$Path, [object]$Value)
    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 12), $encoding)
}

function Write-SmokeConfig {
    param([string]$Path)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "tab-one"; tab_type = "manual"; title = "One"; folder_path = ""
                rows = 1; cols = 2; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @(
                    [ordered]@{
                        item_id = ""; source_name = ""; source_path = ""; is_dir = $false
                        name = "Copy"; path = "safe"; params = "text"; admin = $false
                        action = 1; auto_enter = $false
                    }
                )
            },
            [ordered]@{
                id = "tab-two"; tab_type = "manual"; title = "Two"; folder_path = ""
                rows = 1; cols = 2; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @()
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-SingleTabSmokeConfig {
    param([string]$Path)
    Write-SmokeConfig $Path
    $config = Read-Config $Path
    $config.FolderTabs = @($config.FolderTabs[0])
    Write-JsonFile $Path $config
}

function Write-FolderConfig {
    param([string]$Path, [string]$FolderPath, [switch]$CustomResetState)
    $hidden = @()
    $buttons = @(
        [ordered]@{
            item_id = (Join-Path $FolderPath "zulu.txt").ToLowerInvariant()
            source_name = "zulu.txt"; source_path = Join-Path $FolderPath "zulu.txt"; is_dir = $false
            name = "Zulu"; path = Join-Path $FolderPath "zulu.txt"; params = ""; admin = $false
            action = 0; auto_enter = $false
        },
        [ordered]@{
            item_id = (Join-Path $FolderPath "alpha.txt").ToLowerInvariant()
            source_name = "alpha.txt"; source_path = Join-Path $FolderPath "alpha.txt"; is_dir = $false
            name = "Alpha"; path = Join-Path $FolderPath "alpha.txt"; params = ""; admin = $false
            action = 0; auto_enter = $false
        }
    )
    if ($CustomResetState) {
        $hidden = @("missing-hidden")
        $buttons[0]["name"] = "CustomZ"
        $buttons[0]["params"] = "--kept"
        $buttons[1]["name"] = "CustomA"
        $buttons[1]["params"] = "--kept"
    }
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "folder-tab"; tab_type = "folder"; title = "Folder"; folder_path = $FolderPath
                rows = 1; cols = 2; hidden_item_ids = $hidden; slot_positions = [ordered]@{}
                buttons = $buttons
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-LaunchConfig {
    param([string]$Path, [string]$ToolPath)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "launch-tab"; tab_type = "manual"; title = "Launch"; folder_path = ""
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @(
                    [ordered]@{
                        item_id = ""; source_name = $ToolPath; source_path = $ToolPath; is_dir = $false
                        name = "Launch"; path = $ToolPath; params = "launch-ok"; admin = $false
                        action = 0; auto_enter = $false
                    }
                )
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-DragConfig {
    param([string]$Path)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "drag-tab"; tab_type = "manual"; title = "Drag"; folder_path = ""
                rows = 1; cols = 2; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @(
                    [ordered]@{
                        item_id = ""; source_name = ""; source_path = ""; is_dir = $false
                        name = "First"; path = "first"; params = ""; admin = $false
                        action = 1; auto_enter = $false
                    },
                    [ordered]@{
                        item_id = ""; source_name = ""; source_path = ""; is_dir = $false
                        name = "Second"; path = "second"; params = ""; admin = $false
                        action = 1; auto_enter = $false
                    }
                )
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-OpenExplorerMissingConfig {
    param([string]$Path, [string]$MissingPath)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "open-missing-tab"; tab_type = "manual"; title = "Open"; folder_path = ""
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @(
                    [ordered]@{
                        item_id = ""; source_name = $MissingPath; source_path = $MissingPath; is_dir = $false
                        name = "MissingExplorer"; path = $MissingPath; params = ""; admin = $false
                        action = 0; auto_enter = $false
                    }
                )
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-OpenExplorerFolderConfig {
    param([string]$Path, [string]$FolderPath)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "open-folder-tab"; tab_type = "manual"; title = "Open"; folder_path = ""
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @(
                    [ordered]@{
                        item_id = ""; source_name = $FolderPath; source_path = $FolderPath; is_dir = $true
                        name = "OpenFolder"; path = $FolderPath; params = ""; admin = $false
                        action = 0; auto_enter = $false
                    }
                )
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-AdminDirectoryConfig {
    param([string]$Path, [string]$DirectoryPath)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "admin-dir-tab"; tab_type = "manual"; title = "Admin"; folder_path = ""
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @(
                    [ordered]@{
                        item_id = ""; source_name = $DirectoryPath; source_path = $DirectoryPath; is_dir = $true
                        name = "AdminDir"; path = $DirectoryPath; params = ""; admin = $true
                        action = 0; auto_enter = $false
                    }
                )
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-DuplicateFolderFocusConfig {
    param([string]$Path, [string]$ExistingFolder, [string]$OtherFolder)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "manual-tab"; tab_type = "manual"; title = "Manual"; folder_path = ""
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @()
            },
            [ordered]@{
                id = "existing-folder-tab"; tab_type = "folder"; title = "ExistingFolder"; folder_path = $ExistingFolder
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @()
            },
            [ordered]@{
                id = "other-folder-tab"; tab_type = "folder"; title = "OtherFolder"; folder_path = $OtherFolder
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @()
            }
        )
    }
    Write-JsonFile $Path $config
}

function Write-SetFolderDuplicateFocusConfig {
    param([string]$Path, [string]$CurrentFolder, [string]$ExistingFolder)
    $config = [ordered]@{
        Window = [ordered]@{ Geometry = "360x260+100+100" }
        FolderTabs = @(
            [ordered]@{
                id = "current-folder-tab"; tab_type = "folder"; title = "CurrentFolder"; folder_path = $CurrentFolder
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @()
            },
            [ordered]@{
                id = "existing-folder-tab"; tab_type = "folder"; title = "ExistingFolder"; folder_path = $ExistingFolder
                rows = 1; cols = 1; hidden_item_ids = @(); slot_positions = [ordered]@{}
                buttons = @()
            }
        )
    }
    Write-JsonFile $Path $config
}

function Read-Config {
    param([string]$Path)
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Get-TabTitles {
    param([string]$Path)
    ((Read-Config $Path).FolderTabs | ForEach-Object { $_.title }) -join ","
}

function Wait-Until {
    param([scriptblock]$Predicate, [string]$Failure)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (& $Predicate) { return }
        Start-Sleep -Milliseconds 100
    }
    throw $Failure
}

function Assert-Equal {
    param([object]$Actual, [object]$Expected, [string]$Message)
    if ("$Actual" -ne "$Expected") {
        throw "$Message expected '$Expected', got '$Actual'"
    }
}

function Get-CargoVersion {
    $manifest = Join-Path $RepoRoot "Cargo.toml"
    $match = Select-String -Path $manifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if ($null -eq $match) {
        throw "Cargo.toml package version not found"
    }
    return $match.Matches[0].Groups[1].Value
}

function Get-MenuItemText {
    param([IntPtr]$MenuHandle, [int]$Position)
    $buffer = New-Object System.Text.StringBuilder 512
    [void][WinSmokeUser32]::GetMenuString(
        $MenuHandle,
        [uint32]$Position,
        $buffer,
        $buffer.Capacity,
        [uint32]$MF_BYPOSITION
    )
    return $buffer.ToString()
}

function Get-MenuStateByPosition {
    param([IntPtr]$MenuHandle, [int]$Position)
    $state = [WinSmokeUser32]::GetMenuState($MenuHandle, [uint32]$Position, [uint32]$MF_BYPOSITION)
    if ($state -eq [uint32]::MaxValue) {
        throw "menu item not found at position $Position"
    }
    return $state
}

function Get-MenuStateByCommand {
    param([IntPtr]$MenuHandle, [int]$CommandId)
    $state = [WinSmokeUser32]::GetMenuState($MenuHandle, [uint32]$CommandId, [uint32]$MF_BYCOMMAND)
    if ($state -eq [uint32]::MaxValue) {
        throw "menu command not found: $CommandId"
    }
    return $state
}

function Assert-MenuCommandEnabled {
    param([IntPtr]$MenuHandle, [int]$CommandId, [bool]$ExpectedEnabled, [string]$Label)
    $state = Get-MenuStateByCommand $MenuHandle $CommandId
    $enabled = (($state -band $MF_GRAYED) -eq 0) -and (($state -band $MF_DISABLED) -eq 0)
    if ($enabled -ne $ExpectedEnabled) {
        throw "menu command '$Label' enabled expected '$ExpectedEnabled', got '$enabled'"
    }
}

function Assert-MenuCommandChecked {
    param([IntPtr]$MenuHandle, [int]$CommandId, [bool]$ExpectedChecked, [string]$Label)
    $state = Get-MenuStateByCommand $MenuHandle $CommandId
    $checked = (($state -band $MF_CHECKED) -ne 0)
    if ($checked -ne $ExpectedChecked) {
        throw "menu command '$Label' checked expected '$ExpectedChecked', got '$checked'"
    }
}

function Get-MainFileMenu {
    param($App)
    $mainMenu = [WinSmokeUser32]::GetMenu($App.Hwnd)
    if ($mainMenu -eq [IntPtr]::Zero) {
        throw "main menu handle not found"
    }
    Assert-Equal ([WinSmokeUser32]::GetMenuItemCount($mainMenu)) 2 "Top-level menu count"
    Assert-Equal (Get-MenuItemText $mainMenu 0) "File" "Top-level menu label"
    Assert-Equal (Get-MenuItemText $mainMenu 1) "About" "Top-level About menu label"
    $fileMenu = [WinSmokeUser32]::GetSubMenu($mainMenu, 0)
    if ($fileMenu -eq [IntPtr]::Zero) {
        throw "File submenu handle not found"
    }
    $aboutMenu = [WinSmokeUser32]::GetSubMenu($mainMenu, 1)
    if ($aboutMenu -eq [IntPtr]::Zero) {
        throw "About submenu handle not found"
    }
    Assert-Equal ([WinSmokeUser32]::GetMenuItemCount($aboutMenu)) 1 "About menu item count"
    Assert-Equal (Get-MenuItemText $aboutMenu 0) "About j3Launcher..." "About menu label"
    return $fileMenu
}

function Assert-FileMenuStructure {
    param($App)
    $fileMenu = Get-MainFileMenu $App
    $expected = @(
        [pscustomobject]@{ Text = "Add Folder Tab..."; Separator = $false },
        [pscustomobject]@{ Text = "Add Tab"; Separator = $false },
        [pscustomobject]@{ Text = "Set Current Tab Folder..."; Separator = $false },
        [pscustomobject]@{ Text = ""; Separator = $true },
        [pscustomobject]@{ Text = "Current Tab Layout..."; Separator = $false },
        [pscustomobject]@{ Text = "Rename Current Tab..."; Separator = $false },
        [pscustomobject]@{ Text = "Delete Current Tab..."; Separator = $false },
        [pscustomobject]@{ Text = ""; Separator = $true },
        [pscustomobject]@{ Text = "Move Tab Left`tCtrl+Shift+Left"; Separator = $false },
        [pscustomobject]@{ Text = "Move Tab Right`tCtrl+Shift+Right"; Separator = $false },
        [pscustomobject]@{ Text = "Select Previous Tab`tCtrl+PageUp"; Separator = $false },
        [pscustomobject]@{ Text = "Select Next Tab`tCtrl+PageDown"; Separator = $false },
        [pscustomobject]@{ Text = ""; Separator = $true },
        [pscustomobject]@{ Text = "Sorting Current Tab`tF5"; Separator = $false },
        [pscustomobject]@{ Text = "Refresh Current Tab"; Separator = $false },
        [pscustomobject]@{ Text = "Reset Current Tab"; Separator = $false },
        [pscustomobject]@{ Text = "Manage Hidden Items..."; Separator = $false },
        [pscustomobject]@{ Text = ""; Separator = $true },
        [pscustomobject]@{ Text = "Dark Theme"; Separator = $false },
        [pscustomobject]@{ Text = ""; Separator = $true },
        [pscustomobject]@{ Text = "Exit"; Separator = $false }
    )

    Assert-Equal ([WinSmokeUser32]::GetMenuItemCount($fileMenu)) $expected.Count "File menu item count"
    for ($i = 0; $i -lt $expected.Count; $i++) {
        $state = Get-MenuStateByPosition $fileMenu $i
        $isSeparator = (($state -band $MF_SEPARATOR) -ne 0)
        if ($isSeparator -ne [bool]$expected[$i].Separator) {
            throw "File menu position $i separator expected '$($expected[$i].Separator)', got '$isSeparator'"
        }
        if (-not $expected[$i].Separator) {
            Assert-Equal (Get-MenuItemText $fileMenu $i) $expected[$i].Text "File menu label at position $i"
        }
    }
    return $fileMenu
}

function Assert-ManualTwoTabMenuState {
    param([IntPtr]$MenuHandle, [bool]$OnSecondTab, [bool]$DarkTheme)
    Assert-MenuCommandEnabled $MenuHandle $Menu.AddFolderTab $true "Add Folder Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.AddManualTab $true "Add Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.SetTabFolder $false "Set Current Tab Folder"
    Assert-MenuCommandEnabled $MenuHandle $Menu.TabLayout $true "Current Tab Layout"
    Assert-MenuCommandEnabled $MenuHandle $Menu.RenameTab $true "Rename Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.DeleteTab $true "Delete Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.MoveLeft $OnSecondTab "Move Tab Left"
    Assert-MenuCommandEnabled $MenuHandle $Menu.MoveRight (-not $OnSecondTab) "Move Tab Right"
    Assert-MenuCommandEnabled $MenuHandle $Menu.SelectPrev $OnSecondTab "Select Previous Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.SelectNext (-not $OnSecondTab) "Select Next Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Sort $false "Sorting Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Refresh $false "Refresh Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Reset $false "Reset Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.ManageHidden $false "Manage Hidden Items"
    Assert-MenuCommandEnabled $MenuHandle $Menu.DarkTheme $true "Dark Theme"
    Assert-MenuCommandChecked $MenuHandle $Menu.DarkTheme $DarkTheme "Dark Theme"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Exit $true "Exit"
}

function Assert-FolderSingleTabMenuState {
    param([IntPtr]$MenuHandle)
    Assert-MenuCommandEnabled $MenuHandle $Menu.AddFolderTab $true "Add Folder Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.AddManualTab $true "Add Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.SetTabFolder $true "Set Current Tab Folder"
    Assert-MenuCommandEnabled $MenuHandle $Menu.TabLayout $true "Current Tab Layout"
    Assert-MenuCommandEnabled $MenuHandle $Menu.RenameTab $true "Rename Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.DeleteTab $true "Delete Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.MoveLeft $false "Move Tab Left"
    Assert-MenuCommandEnabled $MenuHandle $Menu.MoveRight $false "Move Tab Right"
    Assert-MenuCommandEnabled $MenuHandle $Menu.SelectPrev $false "Select Previous Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.SelectNext $false "Select Next Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Sort $true "Sorting Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Refresh $true "Refresh Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Reset $true "Reset Current Tab"
    Assert-MenuCommandEnabled $MenuHandle $Menu.ManageHidden $true "Manage Hidden Items"
    Assert-MenuCommandEnabled $MenuHandle $Menu.DarkTheme $true "Dark Theme"
    Assert-MenuCommandChecked $MenuHandle $Menu.DarkTheme $false "Dark Theme"
    Assert-MenuCommandEnabled $MenuHandle $Menu.Exit $true "Exit"
}

function Get-WindowText {
    param([IntPtr]$Hwnd)
    $buffer = New-Object System.Text.StringBuilder 512
    [void][WinSmokeUser32]::GetWindowText($Hwnd, $buffer, $buffer.Capacity)
    $buffer.ToString()
}

function Get-WindowClassName {
    param([IntPtr]$Hwnd)
    $buffer = New-Object System.Text.StringBuilder 512
    [void][WinSmokeUser32]::GetClassName($Hwnd, $buffer, $buffer.Capacity)
    $buffer.ToString()
}

function Find-ProcessWindow {
    param([int]$ProcessId, [string]$Title = "", [string]$ClassName = "")
    $matches = New-Object 'System.Collections.Generic.List[System.IntPtr]'
    $callback = [WinSmokeUser32+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$lparam)
        $windowPid = [uint32]0
        [void][WinSmokeUser32]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
        if ($windowPid -ne [uint32]$ProcessId) { return $true }
        if (-not [WinSmokeUser32]::IsWindowVisible($hwnd)) { return $true }
        if ($Title -ne "" -and (Get-WindowText $hwnd) -ne $Title) { return $true }
        if ($ClassName -ne "" -and (Get-WindowClassName $hwnd) -ne $ClassName) { return $true }
        $matches.Add($hwnd)
        return $true
    }
    [void][WinSmokeUser32]::EnumWindows($callback, [IntPtr]::Zero)
    if ($matches.Count -eq 0) { return [IntPtr]::Zero }
    return $matches[0]
}

function Find-ChildWindow {
    param([IntPtr]$Parent, [string]$Title = "", [string]$ClassName = "")
    $matches = New-Object 'System.Collections.Generic.List[System.IntPtr]'
    $callback = [WinSmokeUser32+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$lparam)
        if ($Title -ne "" -and (Get-WindowText $hwnd) -ne $Title) { return $true }
        if ($ClassName -ne "" -and (Get-WindowClassName $hwnd) -ne $ClassName) { return $true }
        $matches.Add($hwnd)
        return $true
    }
    [void][WinSmokeUser32]::EnumChildWindows($Parent, $callback, [IntPtr]::Zero)
    if ($matches.Count -eq 0) { return [IntPtr]::Zero }
    return $matches[0]
}

function Find-ChildWindows {
    param([IntPtr]$Parent, [string]$Title = "", [string]$ClassName = "")
    $matches = New-Object 'System.Collections.Generic.List[System.IntPtr]'
    $callback = [WinSmokeUser32+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$lparam)
        if ($Title -ne "" -and (Get-WindowText $hwnd) -ne $Title) { return $true }
        if ($ClassName -ne "" -and (Get-WindowClassName $hwnd) -ne $ClassName) { return $true }
        $matches.Add($hwnd)
        return $true
    }
    [void][WinSmokeUser32]::EnumChildWindows($Parent, $callback, [IntPtr]::Zero)
    return @($matches)
}

function Wait-ProcessWindow {
    param([int]$ProcessId, [string]$Title, [string]$ClassName = "")
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $hwnd = Find-ProcessWindow -ProcessId $ProcessId -Title $Title -ClassName $ClassName
        if ($hwnd -ne [IntPtr]::Zero) { return $hwnd }
        Start-Sleep -Milliseconds 100
    }
    throw "window did not open for process ${ProcessId}: title='$Title' class='$ClassName'"
}

function Assert-NoProcessWindow {
    param([int]$ProcessId, [string]$Title, [string]$Message)
    Start-Sleep -Milliseconds 300
    $hwnd = Find-ProcessWindow -ProcessId $ProcessId -Title $Title
    if ($hwnd -ne [IntPtr]::Zero) {
        throw $Message
    }
}

function Wait-ExplorerFolderWindow {
    param([string]$FolderPath)
    $expected = [System.IO.Path]::GetFullPath($FolderPath).TrimEnd('\')
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $shell = New-Object -ComObject Shell.Application
        foreach ($window in @($shell.Windows())) {
            try {
                $path = $window.Document.Folder.Self.Path
                if ($null -ne $path -and ([System.IO.Path]::GetFullPath($path).TrimEnd('\') -ieq $expected)) {
                    return $window
                }
            } catch {
            }
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Explorer folder window did not open: $FolderPath"
}

function Assert-NoUnexpectedAppLogs {
    param($App)
    if ($null -eq $App) { return }
    $patterns = @(
        "thread '.*' panicked",
        "panicked at",
        "stack backtrace",
        "RUST_BACKTRACE",
        "\bpanic\b",
        "\bERROR\b",
        "\bCRITICAL\b",
        "\bfatal\b"
    )
    foreach ($logPath in @($App.StdoutPath, $App.StderrPath)) {
        if ($null -eq $logPath -or -not (Test-Path -LiteralPath $logPath)) {
            continue
        }
        $content = Get-Content -LiteralPath $logPath -Raw
        if ([string]::IsNullOrWhiteSpace($content)) {
            continue
        }
        foreach ($pattern in $patterns) {
            if ($content -match $pattern) {
                throw "unexpected app diagnostic in $logPath matching '$pattern': $($Matches[0])"
            }
        }
    }
}

function Start-SmokeApp {
    param([string]$ConfigPath)
    $quotedConfigPath = '"' + $ConfigPath.Replace('"', '\"') + '"'
    $stdoutPath = [System.IO.Path]::ChangeExtension($ConfigPath, ".stdout.log")
    $stderrPath = [System.IO.Path]::ChangeExtension($ConfigPath, ".stderr.log")
    Remove-Item -LiteralPath $stdoutPath, $stderrPath -ErrorAction SilentlyContinue
    $process = Start-Process `
        -FilePath $AppPath `
        -ArgumentList $quotedConfigPath `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath `
        -PassThru
    $hwnd = Wait-ProcessWindow -ProcessId $process.Id -Title "" -ClassName "j3Launcher.Win32.MainWindow"
    [void][WinSmokeUser32]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 200
    return [pscustomobject]@{ Process = $process; Hwnd = $hwnd; StdoutPath = $stdoutPath; StderrPath = $stderrPath }
}

function Stop-SmokeApp {
    param($App)
    if ($null -eq $App) { return }
    if (-not $App.Process.HasExited) {
        $App.Process.Kill()
        $App.Process.WaitForExit()
    }
}

function Invoke-MenuCommand {
    param($App, [int]$CommandId)
    [void][WinSmokeUser32]::SetForegroundWindow($App.Hwnd)
    $wParam = [UIntPtr]::new([uint32]$CommandId)
    if (-not [WinSmokeUser32]::PostMessage($App.Hwnd, $WM_COMMAND, $wParam, [IntPtr]::Zero)) {
        throw "PostMessage WM_COMMAND failed for id $CommandId"
    }
    Start-Sleep -Milliseconds 250
}

function Invoke-KeyDown {
    param($App, [int]$VirtualKey)
    [void][WinSmokeUser32]::SetForegroundWindow($App.Hwnd)
    $wParam = [UIntPtr]::new([uint32]$VirtualKey)
    if (-not [WinSmokeUser32]::PostMessage($App.Hwnd, $WM_KEYDOWN, $wParam, [IntPtr]::Zero)) {
        throw "PostMessage WM_KEYDOWN failed for virtual key $VirtualKey"
    }
    Start-Sleep -Milliseconds 250
}

function Invoke-ButtonClick {
    param($App, [string]$ButtonText)
    $button = Find-ChildWindow -Parent $App.Hwnd -Title $ButtonText -ClassName "Button"
    if ($button -eq [IntPtr]::Zero) {
        throw "button not found: $ButtonText"
    }
    [void][WinSmokeUser32]::SendMessage($button, $BM_CLICK, [UIntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 300
}

function Invoke-ButtonClickAsync {
    param($App, [string]$ButtonText)
    $button = Find-ChildWindow -Parent $App.Hwnd -Title $ButtonText -ClassName "Button"
    if ($button -eq [IntPtr]::Zero) {
        throw "button not found: $ButtonText"
    }
    if (-not [WinSmokeUser32]::PostMessage($button, $BM_CLICK, [UIntPtr]::Zero, [IntPtr]::Zero)) {
        throw "PostMessage BM_CLICK failed for button $ButtonText"
    }
    Start-Sleep -Milliseconds 300
}

function Invoke-ButtonContextCommand {
    param($App, [string]$ButtonText)
    $button = Find-ChildWindow -Parent $App.Hwnd -Title $ButtonText -ClassName "Button"
    if ($button -eq [IntPtr]::Zero) {
        throw "button not found: $ButtonText"
    }
    $wParam = [UIntPtr]::new([uint64]$button.ToInt64())
    if (-not [WinSmokeUser32]::PostMessage($App.Hwnd, $WM_CONTEXTMENU, $wParam, [IntPtr](-1))) {
        throw "PostMessage WM_CONTEXTMENU failed for button $ButtonText"
    }
    Start-Sleep -Milliseconds 300
}

function Get-WindowCenter {
    param([IntPtr]$Hwnd)
    $rect = New-Object WinSmokeRect
    if (-not [WinSmokeUser32]::GetWindowRect($Hwnd, [ref]$rect)) {
        throw "GetWindowRect failed"
    }
    return [pscustomobject]@{
        X = [int](($rect.Left + $rect.Right) / 2)
        Y = [int](($rect.Top + $rect.Bottom) / 2)
    }
}

function Get-WindowRectObject {
    param([IntPtr]$Hwnd)
    $rect = New-Object WinSmokeRect
    if (-not [WinSmokeUser32]::GetWindowRect($Hwnd, [ref]$rect)) {
        throw "GetWindowRect failed"
    }
    return $rect
}

function Assert-VisibleRect {
    param([IntPtr]$Hwnd, [int]$MinWidth, [int]$MinHeight, [string]$Message)
    if (-not [WinSmokeUser32]::IsWindowVisible($Hwnd)) {
        throw "$Message is not visible"
    }
    $rect = Get-WindowRectObject $Hwnd
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -lt $MinWidth -or $height -lt $MinHeight) {
        throw "$Message rect too small: ${width}x$height"
    }
    return $rect
}

function Assert-RectInside {
    param($ChildRect, $ParentRect, [string]$Message)
    if ($ChildRect.Left -lt $ParentRect.Left -or
        $ChildRect.Top -lt $ParentRect.Top -or
        $ChildRect.Right -gt $ParentRect.Right -or
        $ChildRect.Bottom -gt $ParentRect.Bottom) {
        throw "$Message is outside parent rect"
    }
}

function Assert-WindowSizeAtLeast {
    param([IntPtr]$Hwnd, [int]$MinWidth, [int]$MinHeight, [string]$Message)
    $rect = Assert-VisibleRect $Hwnd $MinWidth $MinHeight $Message
    return $rect
}

function Assert-MainLayout {
    param($App, [string]$CaseName)
    $mainRect = Assert-VisibleRect $App.Hwnd 300 200 "$CaseName main window"
    $tab = Find-ChildWindow -Parent $App.Hwnd -ClassName "SysTabControl32"
    if ($tab -eq [IntPtr]::Zero) {
        throw "$CaseName tab control not found"
    }
    $tabRect = Assert-VisibleRect $tab 240 120 "$CaseName tab control"
    Assert-RectInside $tabRect $mainRect "$CaseName tab control"

    $buttons = @(Find-ChildWindows -Parent $App.Hwnd -ClassName "Button")
    if ($buttons.Count -lt 1) {
        throw "$CaseName buttons not found"
    }
    $copy = Find-ChildWindow -Parent $App.Hwnd -Title "Copy" -ClassName "Button"
    if ($copy -eq [IntPtr]::Zero) {
        throw "$CaseName Copy button not found"
    }
    $copyRect = Assert-VisibleRect $copy 40 25 "$CaseName Copy button"
    Assert-RectInside $copyRect $mainRect "$CaseName Copy button"
}

function Assert-DialogControlLayout {
    param([IntPtr]$Dialog, [int[]]$ControlIds, [string]$CaseName)
    $dialogRect = Assert-VisibleRect $Dialog 180 100 "$CaseName dialog"
    foreach ($id in $ControlIds) {
        $control = Get-DialogItem $Dialog $id
        $controlRect = Assert-VisibleRect $control 12 12 "$CaseName control $id"
        Assert-RectInside $controlRect $dialogRect "$CaseName control $id"
    }
}

function Convert-ToAbsoluteMousePoint {
    param([int]$X, [int]$Y)
    $left = [WinSmokeUser32]::GetSystemMetrics($SM_XVIRTUALSCREEN)
    $top = [WinSmokeUser32]::GetSystemMetrics($SM_YVIRTUALSCREEN)
    $width = [Math]::Max(1, [WinSmokeUser32]::GetSystemMetrics($SM_CXVIRTUALSCREEN) - 1)
    $height = [Math]::Max(1, [WinSmokeUser32]::GetSystemMetrics($SM_CYVIRTUALSCREEN) - 1)
    return [pscustomobject]@{
        X = [int][Math]::Round((($X - $left) * 65535.0) / $width)
        Y = [int][Math]::Round((($Y - $top) * 65535.0) / $height)
    }
}

function Send-MouseInput {
    param([int]$Flags, [int]$Dx = 0, [int]$Dy = 0)
    $input = New-Object WinSmokeInput
    $input.type = [uint32]$INPUT_MOUSE
    $input.U.mi = New-Object WinSmokeMouseInput
    $input.U.mi.dx = $Dx
    $input.U.mi.dy = $Dy
    $input.U.mi.dwFlags = [uint32]$Flags
    $sent = [WinSmokeUser32]::SendInput(1, [WinSmokeInput[]]@($input), [System.Runtime.InteropServices.Marshal]::SizeOf([type][WinSmokeInput]))
    if ($sent -ne 1) {
        throw "SendInput failed for mouse flags $Flags"
    }
}

function Send-MouseMoveAbsolute {
    param([int]$X, [int]$Y)
    $point = Convert-ToAbsoluteMousePoint $X $Y
    Send-MouseInput ($MOUSEEVENTF_MOVE -bor $MOUSEEVENTF_ABSOLUTE -bor $MOUSEEVENTF_VIRTUALDESK) $point.X $point.Y
    Start-Sleep -Milliseconds 80
}

function Invoke-PhysicalMouseClick {
    param([int]$X, [int]$Y, [switch]$Right)
    Send-MouseMoveAbsolute $X $Y
    if ($Right) {
        Send-MouseInput $MOUSEEVENTF_RIGHTDOWN
        Start-Sleep -Milliseconds 60
        Send-MouseInput $MOUSEEVENTF_RIGHTUP
    } else {
        Send-MouseInput $MOUSEEVENTF_LEFTDOWN
        Start-Sleep -Milliseconds 60
        Send-MouseInput $MOUSEEVENTF_LEFTUP
    }
    Start-Sleep -Milliseconds 200
}

function Invoke-PhysicalButtonContextItem {
    param($App, [string]$ButtonText, [int]$ItemIndex)
    $button = Find-ChildWindow -Parent $App.Hwnd -Title $ButtonText -ClassName "Button"
    if ($button -eq [IntPtr]::Zero) {
        throw "button not found: $ButtonText"
    }
    [void][WinSmokeUser32]::SetForegroundWindow($App.Hwnd)
    Start-Sleep -Milliseconds 100
    $center = Get-WindowCenter $button
    Invoke-PhysicalMouseClick $center.X $center.Y -Right
    $menu = Wait-ProcessWindow -ProcessId $App.Process.Id -Title "" -ClassName "#32768"
    $rect = New-Object WinSmokeRect
    if (-not [WinSmokeUser32]::GetWindowRect($menu, [ref]$rect)) {
        throw "GetWindowRect failed for popup menu"
    }
    $itemHeight = [Math]::Max(1, [int](($rect.Bottom - $rect.Top) / 3))
    $x = $rect.Left + 24
    $y = $rect.Top + [int](($ItemIndex * $itemHeight) + ($itemHeight / 2))
    Invoke-PhysicalMouseClick $x $y
}

function Send-ButtonDragEvent {
    param($App, [IntPtr]$SourceButton, [int]$Event)
    [void][WinSmokeUser32]::SendMessage(
        $App.Hwnd,
        $WM_BUTTON_DRAG_EVENT,
        [UIntPtr]::new([uint32]$Event),
        $SourceButton
    )
}

function Invoke-ButtonDragSwap {
    param($App, [string]$SourceText, [string]$TargetText)
    $source = Find-ChildWindow -Parent $App.Hwnd -Title $SourceText -ClassName "Button"
    if ($source -eq [IntPtr]::Zero) {
        throw "source button not found: $SourceText"
    }
    $target = Find-ChildWindow -Parent $App.Hwnd -Title $TargetText -ClassName "Button"
    if ($target -eq [IntPtr]::Zero) {
        throw "target button not found: $TargetText"
    }

    $sourceCenter = Get-WindowCenter $source
    $targetCenter = Get-WindowCenter $target
    [void][WinSmokeUser32]::SetCursorPos($sourceCenter.X, $sourceCenter.Y)
    Send-ButtonDragEvent $App $source $BUTTON_DRAG_EVENT_DOWN
    [void][WinSmokeUser32]::SetCursorPos($targetCenter.X, $targetCenter.Y)
    Send-ButtonDragEvent $App $source $BUTTON_DRAG_EVENT_MOVE
    Send-ButtonDragEvent $App $source $BUTTON_DRAG_EVENT_UP
    Start-Sleep -Milliseconds 300
}

function Invoke-ButtonPhysicalDragSwap {
    param($App, [string]$SourceText, [string]$TargetText)
    $source = Find-ChildWindow -Parent $App.Hwnd -Title $SourceText -ClassName "Button"
    if ($source -eq [IntPtr]::Zero) {
        throw "source button not found: $SourceText"
    }
    $target = Find-ChildWindow -Parent $App.Hwnd -Title $TargetText -ClassName "Button"
    if ($target -eq [IntPtr]::Zero) {
        throw "target button not found: $TargetText"
    }

    [void][WinSmokeUser32]::SetForegroundWindow($App.Hwnd)
    Start-Sleep -Milliseconds 100
    $sourceCenter = Get-WindowCenter $source
    $targetCenter = Get-WindowCenter $target
    Send-MouseMoveAbsolute $sourceCenter.X $sourceCenter.Y
    Send-MouseInput $MOUSEEVENTF_LEFTDOWN
    for ($i = 1; $i -le 10; $i++) {
        $x = [int]($sourceCenter.X + (($targetCenter.X - $sourceCenter.X) * $i / 10))
        $y = [int]($sourceCenter.Y + (($targetCenter.Y - $sourceCenter.Y) * $i / 10))
        Send-MouseMoveAbsolute $x $y
    }
    Send-MouseInput $MOUSEEVENTF_LEFTUP
    Start-Sleep -Milliseconds 500
}

function Resize-SmokeWindow {
    param($App, [int]$Width, [int]$Height)
    if (-not [WinSmokeUser32]::MoveWindow($App.Hwnd, 120, 120, $Width, $Height, $true)) {
        throw "MoveWindow failed"
    }
    [void][WinSmokeUser32]::SetForegroundWindow($App.Hwnd)
    Start-Sleep -Milliseconds 300
}

function Clear-PickerOverride {
    Remove-Item Env:\J3LAUNCHER_TEST_PICK_FOLDER -ErrorAction SilentlyContinue
    Remove-Item Env:\J3LAUNCHER_TEST_PICK_FOLDER_ERROR -ErrorAction SilentlyContinue
    Remove-Item Env:\J3LAUNCHER_TEST_CONTEXT_MENU_COMMAND -ErrorAction SilentlyContinue
}

function Set-PickFolderOverride {
    param([string]$FolderPath)
    Remove-Item Env:\J3LAUNCHER_TEST_PICK_FOLDER_ERROR -ErrorAction SilentlyContinue
    $env:J3LAUNCHER_TEST_PICK_FOLDER = $FolderPath
}

function Set-PickFolderErrorOverride {
    param([string]$Message)
    Remove-Item Env:\J3LAUNCHER_TEST_PICK_FOLDER -ErrorAction SilentlyContinue
    $env:J3LAUNCHER_TEST_PICK_FOLDER_ERROR = $Message
}

function Set-ContextCommandOverride {
    param([string]$Command)
    $env:J3LAUNCHER_TEST_CONTEXT_MENU_COMMAND = $Command
}

function Get-DialogItem {
    param([IntPtr]$Dialog, [int]$ControlId)
    $item = [WinSmokeUser32]::GetDlgItem($Dialog, $ControlId)
    if ($item -eq [IntPtr]::Zero) {
        throw "dialog item not found: title control id $ControlId"
    }
    return $item
}

function Get-DialogItemText {
    param([IntPtr]$Dialog, [int]$ControlId)
    $item = Get-DialogItem $Dialog $ControlId
    return Get-WindowText $item
}

function Click-DialogButton {
    param([IntPtr]$Dialog, [int]$ButtonId)
    $button = Get-DialogItem $Dialog $ButtonId
    [void][WinSmokeUser32]::SendMessage($button, $BM_CLICK, [UIntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 300
}

function Set-DialogText {
    param([IntPtr]$Dialog, [int]$ControlId, [string]$Value)
    $edit = Get-DialogItem $Dialog $ControlId
    [void][WinSmokeUser32]::SendMessage($edit, $EM_SETSEL, [UIntPtr]::Zero, [IntPtr](-1))
    [void][WinSmokeUser32]::SendMessageText($edit, $EM_REPLACESEL, [UIntPtr]::new([uint32]1), $Value)
}

function Assert-DefaultDialogButton {
    param([IntPtr]$Dialog, [int]$ButtonId, [string]$Message)
    $button = Get-DialogItem $Dialog $ButtonId
    $style = [WinSmokeUser32]::GetWindowLongPtr($button, $GWL_STYLE).ToInt64()
    if (($style -band $BS_DEFPUSHBUTTON) -eq 0) {
        throw $Message
    }
}

function Set-TextDialogValueAndClick {
    param($App, [string]$Title, [int]$TextControlId, [string]$Value, [int]$ButtonId)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title $Title
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    Set-DialogText $dialog $TextControlId $Value
    Click-DialogButton $dialog $ButtonId
}

function Click-DialogByTitle {
    param($App, [string]$Title, [int]$ButtonId)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title $Title
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    Click-DialogButton $dialog $ButtonId
}

function Send-DialogCommandByTitle {
    param($App, [string]$Title, [int]$CommandId)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title $Title
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    $wParam = [UIntPtr]::new([uint32]$CommandId)
    if (-not [WinSmokeUser32]::PostMessage($dialog, $WM_COMMAND, $wParam, [IntPtr]::Zero)) {
        throw "PostMessage WM_COMMAND failed for dialog command $CommandId"
    }
    Start-Sleep -Milliseconds 300
}

function Send-DialogCommandByClass {
    param($App, [string]$ClassName, [int]$CommandId)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title "" -ClassName $ClassName
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    $wParam = [UIntPtr]::new([uint32]$CommandId)
    if (-not [WinSmokeUser32]::PostMessage($dialog, $WM_COMMAND, $wParam, [IntPtr]::Zero)) {
        throw "PostMessage WM_COMMAND failed for dialog command $CommandId"
    }
    Start-Sleep -Milliseconds 300
}

function Cancel-NativeFolderPicker {
    param($App)
    Send-DialogCommandByClass $App "#32770" $IDCANCEL
}

function Select-NativeFolderPicker {
    param($App, [string]$FolderPath)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title "" -ClassName "#32770"
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    $edit = Find-ChildWindow -Parent $dialog -ClassName "Edit"
    if ($edit -eq [IntPtr]::Zero) {
        throw "native folder picker edit field not found"
    }
    [void][WinSmokeUser32]::SetFocus($edit)
    [void][WinSmokeUser32]::SendMessage($edit, $EM_SETSEL, [UIntPtr]::Zero, [IntPtr](-1))
    [void][WinSmokeUser32]::SendMessageText($edit, $EM_REPLACESEL, [UIntPtr]::new([uint32]1), $FolderPath)
    [void][WinSmokeUser32]::PostMessage($edit, $WM_KEYDOWN, [UIntPtr]::new([uint32]$VK_RETURN), [IntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    if ([WinSmokeUser32]::IsWindow($dialog)) {
        Click-DialogButton $dialog $IDOK
    }
}

function Click-DefaultNoDialog {
    param($App, [string]$Title)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title $Title
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    Assert-DefaultDialogButton $dialog $IDNO "$Title default button is not No"
    Click-DialogButton $dialog $IDNO
}

function Set-TabLayoutDialog {
    param($App, [int]$Rows, [int]$Cols)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title "Tab Layout"
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    Set-DialogText $dialog $ID_LAYOUT_ROWS "$Rows"
    Set-DialogText $dialog $ID_LAYOUT_COLS "$Cols"
    Click-DialogButton $dialog $ID_LAYOUT_APPLY
}

function Set-EditButtonDialog {
    param($App, [string]$Name, [string]$Path, [string]$Params)
    $dialog = Wait-ProcessWindow -ProcessId $App.Process.Id -Title "Edit Button"
    [void][WinSmokeUser32]::SetForegroundWindow($dialog)
    Start-Sleep -Milliseconds 100
    Set-DialogText $dialog $ID_EDIT_NAME $Name
    Set-DialogText $dialog $ID_EDIT_PATH $Path
    Set-DialogText $dialog $ID_EDIT_PARAMS $Params
    Click-DialogButton $dialog $ID_EDIT_OK
}

function Run-AppCase {
    param([string]$Name, [scriptblock]$Setup, [scriptblock]$Body)
    if ($Only.Count -gt 0 -and ($Only -notcontains $Name)) {
        return
    }
    $script:RunAppCaseCount += 1
    $configPath = Join-Path $WorkDir "$Name.json"
    $app = $null
    $caseFailed = $false
    Clear-PickerOverride
    try {
        & $Setup $configPath
        $app = Start-SmokeApp $configPath
        & $Body $configPath $app
        Write-Host "${Name}: ok"
    } catch {
        $caseFailed = $true
        throw
    } finally {
        Stop-SmokeApp $app
        if (-not $caseFailed) {
            Assert-NoUnexpectedAppLogs $app
        }
        Clear-PickerOverride
    }
}

function Run-PhysicalInputCase {
    param([string]$Name, [scriptblock]$Setup, [scriptblock]$Body)
    if (-not $IncludePhysicalInput) {
        if ($Only.Count -gt 0 -and ($Only -contains $Name)) {
            throw "physical input smoke case requires -IncludePhysicalInput: $Name"
        }
        return
    }
    Run-AppCase $Name $Setup $Body
}

function Run-DebugOnlyCase {
    param([string]$Name, [scriptblock]$Setup, [scriptblock]$Body)
    if ($SkipDebugOnlyCases) {
        if ($Only.Count -gt 0 -and ($Only -contains $Name)) {
            throw "debug-only smoke case skipped by -SkipDebugOnly: $Name"
        }
        return
    }
    Run-AppCase $Name $Setup $Body
}

Run-AppCase "add-tab" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.AddManualTab
    Wait-Until { (Read-Config $path).FolderTabs.Count -eq 3 } "Add Tab did not persist"
    Assert-Equal (Read-Config $path).FolderTabs[-1].title "Tab 3" "Add Tab title"
}

Run-AppCase "move-tab" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.MoveRight
    Wait-Until { (Get-TabTitles $path) -eq "Two,One" } "Move Right did not persist"
    Invoke-MenuCommand $app $Menu.MoveLeft
    Wait-Until { (Get-TabTitles $path) -eq "One,Two" } "Move Left did not persist"
}

Run-AppCase "select-tab" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.SelectNext
    Invoke-MenuCommand $app $Menu.MoveLeft
    Wait-Until { (Get-TabTitles $path) -eq "Two,One" } "Select Next then Move Left did not target tab two"
}

Run-AppCase "state-restore-after-restart" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Resize-SmokeWindow $app 540 380
    Invoke-MenuCommand $app $Menu.DarkTheme
    Invoke-MenuCommand $app $Menu.MoveRight
    Invoke-MenuCommand $app $Menu.AddManualTab
    Wait-Until {
        (Get-TabTitles $path) -eq "Two,One,Tab 3" -and
            [bool]((Read-Config $path).Window.DarkTheme)
    } "Initial state changes were not persisted before restart"

    Invoke-MenuCommand $app $Menu.Exit
    Wait-Until { $app.Process.Refresh(); $app.Process.HasExited } "First app instance did not exit before restart"
    $saved = Read-Config $path
    if ($saved.Window.Geometry -notmatch '^\d+x\d+[+-]\d+[+-]\d+$') {
        throw "Window geometry was not saved with size and position: $($saved.Window.Geometry)"
    }

    $restarted = $null
    $restartFailed = $false
    try {
        $restarted = Start-SmokeApp $path
        Assert-WindowSizeAtLeast $restarted.Hwnd 500 330 "Restored main window" | Out-Null
        Assert-Equal (Get-TabTitles $path) "Two,One,Tab 3" "Restored tab order"
        if (-not [bool]((Read-Config $path).Window.DarkTheme)) {
            throw "Restored dark theme flag was not true"
        }
        Invoke-MenuCommand $restarted $Menu.AddManualTab
        Wait-Until { (Read-Config $path).FolderTabs.Count -eq 4 } "Restarted app did not accept a follow-up menu command"
        Invoke-MenuCommand $restarted $Menu.Exit
        Wait-Until { $restarted.Process.Refresh(); $restarted.Process.HasExited } "Restarted app did not exit"
    } catch {
        $restartFailed = $true
        throw
    } finally {
        Stop-SmokeApp $restarted
        if (-not $restartFailed) {
            Assert-NoUnexpectedAppLogs $restarted
        }
    }
}

Run-AppCase "menu-structure-and-state" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    $fileMenu = Assert-FileMenuStructure $app
    Assert-ManualTwoTabMenuState $fileMenu $false $false
    Invoke-MenuCommand $app $Menu.DarkTheme
    Assert-ManualTwoTabMenuState $fileMenu $false $true
    Invoke-MenuCommand $app $Menu.SelectNext
    Assert-ManualTwoTabMenuState $fileMenu $true $true
}

Run-AppCase "menu-folder-command-state" {
    param($path)
    $folder = Join-Path $WorkDir "menu-folder-state-target"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder
} {
    param($path, $app)
    $fileMenu = Assert-FileMenuStructure $app
    Wait-Until {
        try {
            Assert-FolderSingleTabMenuState $fileMenu
            return $true
        } catch {
            return $false
        }
    } "Folder tab menu state did not become enabled"
}

Run-AppCase "main-layout-rects" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Assert-MainLayout $app "initial"
    Resize-SmokeWindow $app 520 360
    Assert-MainLayout $app "resized"
    Invoke-MenuCommand $app $Menu.DarkTheme
    Assert-MainLayout $app "dark theme"
}

Run-AppCase "disabled-single-tab-commands" { param($path) Write-SingleTabSmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.MoveLeft
    Invoke-MenuCommand $app $Menu.MoveRight
    Invoke-MenuCommand $app $Menu.SelectPrev
    Invoke-MenuCommand $app $Menu.SelectNext
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "One" "Disabled single-tab movement commands"
}

Run-AppCase "disabled-manual-folder-commands" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.Sort
    Invoke-MenuCommand $app $Menu.Refresh
    Invoke-MenuCommand $app $Menu.Reset
    Invoke-MenuCommand $app $Menu.ManageHidden
    Assert-NoProcessWindow $app.Process.Id "Reset Tab" "Disabled Reset opened a dialog on a manual tab"
    Assert-NoProcessWindow $app.Process.Id "Manage Hidden Items" "Disabled Manage Hidden Items opened on a manual tab"
    Assert-Equal (Get-TabTitles $path) "One,Two" "Disabled manual-folder commands"
}

Run-AppCase "native-add-folder-tab-cancel" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.AddFolderTab
    Cancel-NativeFolderPicker $app
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "One,Two" "Native Add Folder Tab cancel mutated tabs"
    Invoke-MenuCommand $app $Menu.AddManualTab
    Wait-Until { (Read-Config $path).FolderTabs.Count -eq 3 } "Main window did not resume commands after native Add Folder Tab cancel"
}

Run-AppCase "native-add-folder-tab-select" {
    param($path)
    $folder = Join-Path $WorkDir "native-add-folder-target"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "native-a.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "native-b.txt") -Value "b"
    Write-SmokeConfig $path
} {
    param($path, $app)
    $folder = Join-Path $WorkDir "native-add-folder-target"
    Invoke-MenuCommand $app $Menu.AddFolderTab
    Select-NativeFolderPicker $app $folder
    Wait-Until {
        $tabs = (Read-Config $path).FolderTabs
        $tabs.Count -eq 3 -and
            $tabs[-1].folder_path -eq $folder -and
            (($tabs[-1].buttons | ForEach-Object { $_.source_name } | Sort-Object) -join ",") -eq "native-a.txt,native-b.txt"
    } "Native Add Folder Tab did not scan and persist the selected folder"
}

Run-DebugOnlyCase "add-folder-tab" {
    param($path)
    $folder = Join-Path $WorkDir "add-folder-target"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "a.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "b.txt") -Value "b"
    Write-SmokeConfig $path
    Set-PickFolderOverride $folder
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.AddFolderTab
    Wait-Until {
        $tabs = (Read-Config $path).FolderTabs
        $tabs.Count -eq 3 -and
            $tabs[-1].folder_path -eq (Join-Path $WorkDir "add-folder-target") -and
            (($tabs[-1].buttons | ForEach-Object { $_.source_name } | Sort-Object) -join ",") -eq "a.txt,b.txt"
    } "Add Folder Tab did not scan and persist the selected folder"
}

Run-AppCase "native-set-current-tab-folder-cancel" {
    param($path)
    $oldFolder = Join-Path $WorkDir "native-set-folder-cancel-old"
    New-Item -ItemType Directory -Path $oldFolder -Force | Out-Null
    Write-FolderConfig $path $oldFolder
} {
    param($path, $app)
    $oldFolder = Join-Path $WorkDir "native-set-folder-cancel-old"
    Invoke-MenuCommand $app $Menu.SetTabFolder
    Cancel-NativeFolderPicker $app
    Start-Sleep -Milliseconds 300
    $tab = (Read-Config $path).FolderTabs[0]
    Assert-Equal $tab.folder_path $oldFolder "Native Set Current Tab Folder cancel changed folder path"
    Invoke-MenuCommand $app $Menu.Sort
    Wait-Until { (((Read-Config $path).FolderTabs[0].buttons | ForEach-Object { $_.name }) -join ",") -eq "Alpha,Zulu" } "Main window did not resume commands after native Set Current Tab Folder cancel"
}

Run-AppCase "native-set-current-tab-folder-select" {
    param($path)
    $oldFolder = Join-Path $WorkDir "native-set-folder-old"
    $newFolder = Join-Path $WorkDir "native-set-folder-new"
    New-Item -ItemType Directory -Path $oldFolder -Force | Out-Null
    New-Item -ItemType Directory -Path $newFolder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $newFolder "native-new.txt") -Value "n"
    Write-FolderConfig $path $oldFolder
} {
    param($path, $app)
    $newFolder = Join-Path $WorkDir "native-set-folder-new"
    Invoke-MenuCommand $app $Menu.SetTabFolder
    Select-NativeFolderPicker $app $newFolder
    Wait-Until {
        $tab = (Read-Config $path).FolderTabs[0]
        $tab.folder_path -eq $newFolder -and
            (($tab.buttons | ForEach-Object { $_.source_name }) -join ",") -eq "native-new.txt"
    } "Native Set Current Tab Folder did not scan and persist the selected folder"
}

Run-DebugOnlyCase "add-folder-tab-cancel" {
    param($path)
    Write-SmokeConfig $path
    Set-PickFolderOverride "__CANCEL__"
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.AddFolderTab
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "One,Two" "Add Folder Tab cancel mutated tabs"
}

Run-DebugOnlyCase "add-folder-tab-error" {
    param($path)
    Write-SmokeConfig $path
    Set-PickFolderErrorOverride "no local path"
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.AddFolderTab
    Send-DialogCommandByTitle $app "Add Folder Tab" $IDOK
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "One,Two" "Add Folder Tab error mutated tabs"
}

Run-DebugOnlyCase "add-folder-tab-duplicate-focus" {
    param($path)
    $existing = Join-Path $WorkDir "add-duplicate-existing"
    $other = Join-Path $WorkDir "add-duplicate-other"
    New-Item -ItemType Directory -Path $existing -Force | Out-Null
    New-Item -ItemType Directory -Path $other -Force | Out-Null
    Write-DuplicateFolderFocusConfig $path $existing $other
    Set-PickFolderOverride $existing
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.AddFolderTab
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "Manual,ExistingFolder,OtherFolder" "Duplicate Add Folder Tab changed tab order"
    Invoke-MenuCommand $app $Menu.MoveLeft
    Wait-Until { (Get-TabTitles $path) -eq "ExistingFolder,Manual,OtherFolder" } "Duplicate Add Folder Tab did not focus the existing folder tab"
}

Run-DebugOnlyCase "set-current-tab-folder" {
    param($path)
    $oldFolder = Join-Path $WorkDir "set-folder-old"
    $newFolder = Join-Path $WorkDir "set-folder-new"
    New-Item -ItemType Directory -Path $oldFolder -Force | Out-Null
    New-Item -ItemType Directory -Path $newFolder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $newFolder "new.txt") -Value "n"
    Write-FolderConfig $path $oldFolder
    Set-PickFolderOverride $newFolder
} {
    param($path, $app)
    $newFolder = Join-Path $WorkDir "set-folder-new"
    Invoke-MenuCommand $app $Menu.SetTabFolder
    Wait-Until {
        $tab = (Read-Config $path).FolderTabs[0]
        $tab.folder_path -eq $newFolder -and
            (($tab.buttons | ForEach-Object { $_.source_name }) -join ",") -eq "new.txt"
    } "Set Current Tab Folder did not scan and persist the selected folder"
}

Run-DebugOnlyCase "set-current-tab-folder-cancel" {
    param($path)
    $oldFolder = Join-Path $WorkDir "set-folder-cancel-old"
    New-Item -ItemType Directory -Path $oldFolder -Force | Out-Null
    Write-FolderConfig $path $oldFolder
    Set-PickFolderOverride "__CANCEL__"
} {
    param($path, $app)
    $oldFolder = Join-Path $WorkDir "set-folder-cancel-old"
    Invoke-MenuCommand $app $Menu.SetTabFolder
    Start-Sleep -Milliseconds 300
    $tab = (Read-Config $path).FolderTabs[0]
    Assert-Equal $tab.folder_path $oldFolder "Set Current Tab Folder cancel changed folder path"
}

Run-DebugOnlyCase "set-current-tab-folder-error" {
    param($path)
    $oldFolder = Join-Path $WorkDir "set-folder-error-old"
    New-Item -ItemType Directory -Path $oldFolder -Force | Out-Null
    Write-FolderConfig $path $oldFolder
    Set-PickFolderErrorOverride "no local path"
} {
    param($path, $app)
    $oldFolder = Join-Path $WorkDir "set-folder-error-old"
    Invoke-MenuCommand $app $Menu.SetTabFolder
    Send-DialogCommandByTitle $app "Set Tab Folder" $IDOK
    Start-Sleep -Milliseconds 300
    $tab = (Read-Config $path).FolderTabs[0]
    Assert-Equal $tab.folder_path $oldFolder "Set Current Tab Folder error changed folder path"
}

Run-DebugOnlyCase "set-current-tab-folder-duplicate-focus" {
    param($path)
    $current = Join-Path $WorkDir "set-duplicate-current"
    $existing = Join-Path $WorkDir "set-duplicate-existing"
    New-Item -ItemType Directory -Path $current -Force | Out-Null
    New-Item -ItemType Directory -Path $existing -Force | Out-Null
    Write-SetFolderDuplicateFocusConfig $path $current $existing
    Set-PickFolderOverride $existing
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.SetTabFolder
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "CurrentFolder,ExistingFolder" "Duplicate Set Current Tab Folder changed tab order"
    Invoke-MenuCommand $app $Menu.MoveLeft
    Wait-Until { (Get-TabTitles $path) -eq "ExistingFolder,CurrentFolder" } "Duplicate Set Current Tab Folder did not focus the existing folder tab"
}

Run-AppCase "sort-accelerator-after-resize-focus" {
    param($path)
    $folder = Join-Path $WorkDir "sort-accelerator-case"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder -CustomResetState
} {
    param($path, $app)
    Resize-SmokeWindow $app 420 300
    Invoke-KeyDown $app $VK_F5
    Wait-Until { (((Read-Config $path).FolderTabs[0].buttons | ForEach-Object { $_.name }) -join ",") -eq "CustomA,CustomZ" } "F5 accelerator did not sort after resize/focus"
}

Run-AppCase "rename-tab" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.RenameTab
    Set-TextDialogValueAndClick $app "Rename Tab" $ID_TEXT_INPUT "Renamed" $ID_TEXT_OK
    Wait-Until { (Read-Config $path).FolderTabs[0].title -eq "Renamed" } "Rename did not persist"
}

Run-AppCase "tab-layout" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.TabLayout
    Set-TabLayoutDialog $app 2 3
    Wait-Until {
        $tab = (Read-Config $path).FolderTabs[0]
        $tab.rows -eq 2 -and $tab.cols -eq 3 -and $tab.buttons.Count -eq 6
    } "Tab Layout did not persist"
}

Run-AppCase "delete-tab" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.DeleteTab
    Click-DialogByTitle $app "Delete Tab" $IDYES
    Wait-Until { (Get-TabTitles $path) -eq "Two" } "Delete Tab did not persist"
}

Run-AppCase "delete-default-no" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.DeleteTab
    Click-DefaultNoDialog $app "Delete Tab"
    Start-Sleep -Milliseconds 300
    Assert-Equal (Get-TabTitles $path) "One,Two" "Delete default response"
}

Run-AppCase "refresh-folder" {
    param($path)
    $folder = Join-Path $WorkDir "refresh-case"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "beta.txt") -Value "b"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.Refresh
    Wait-Until {
        $names = ((Read-Config $path).FolderTabs[0].buttons | ForEach-Object { $_.source_name } | Sort-Object) -join ","
        $names -eq "alpha.txt,beta.txt,zulu.txt"
    } "Refresh did not scan folder contents"
}

Run-AppCase "reset-default-no" {
    param($path)
    $folder = Join-Path $WorkDir "reset-default-case"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder -CustomResetState
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.Reset
    Click-DefaultNoDialog $app "Reset Tab"
    Start-Sleep -Milliseconds 300
    $tab = (Read-Config $path).FolderTabs[0]
    Assert-Equal (($tab.buttons | ForEach-Object { $_.name }) -join ",") "CustomZ,CustomA" "Reset default response button names"
    Assert-Equal (($tab.hidden_item_ids | ForEach-Object { $_ }) -join ",") "missing-hidden" "Reset default response hidden ids"
}

Run-AppCase "sort-refresh-reset" {
    param($path)
    $folder = Join-Path $WorkDir "folder-case"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder -CustomResetState
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.Sort
    Wait-Until { (((Read-Config $path).FolderTabs[0].buttons | ForEach-Object { $_.name }) -join ",") -eq "CustomA,CustomZ" } "Sort did not persist"
    Invoke-MenuCommand $app $Menu.Reset
    Click-DialogByTitle $app "Reset Tab" $IDYES
    Wait-Until {
        $tab = (Read-Config $path).FolderTabs[0]
        (($tab.buttons | ForEach-Object { $_.source_name } | Sort-Object) -join ",") -eq "alpha.txt,zulu.txt" -and
            (($tab.hidden_item_ids | ForEach-Object { $_ }) -join ",") -eq ""
    } "Reset did not rebuild folder tab"
}

Run-AppCase "manage-hidden-opens" {
    param($path)
    $folder = Join-Path $WorkDir "hidden-case"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder -CustomResetState
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.ManageHidden
    Click-DialogByTitle $app "Manage Hidden Items" $ID_HIDDEN_CLOSE
}

Run-DebugOnlyCase "dialog-layout-rects" {
    param($path)
    $folder = Join-Path $WorkDir "dialog-layout-folder"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Write-FolderConfig $path $folder
    Set-ContextCommandOverride "Edit"
} {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.RenameTab
    $dialog = Wait-ProcessWindow -ProcessId $app.Process.Id -Title "Rename Tab"
    Assert-DialogControlLayout $dialog @($ID_TEXT_INPUT, $ID_TEXT_OK) "Rename Tab"
    Click-DialogButton $dialog $ID_TEXT_OK

    Invoke-MenuCommand $app $Menu.TabLayout
    $dialog = Wait-ProcessWindow -ProcessId $app.Process.Id -Title "Tab Layout"
    Assert-DialogControlLayout $dialog @($ID_LAYOUT_ROWS, $ID_LAYOUT_COLS, $ID_LAYOUT_APPLY) "Tab Layout"
    Click-DialogButton $dialog $ID_LAYOUT_APPLY

    Invoke-MenuCommand $app $Menu.ManageHidden
    $dialog = Wait-ProcessWindow -ProcessId $app.Process.Id -Title "Manage Hidden Items"
    Assert-DialogControlLayout $dialog @($ID_HIDDEN_CLOSE) "Manage Hidden Items"
    Click-DialogButton $dialog $ID_HIDDEN_CLOSE

    Invoke-ButtonContextCommand $app "Zulu"
    $dialog = Wait-ProcessWindow -ProcessId $app.Process.Id -Title "Edit Button"
    Assert-DialogControlLayout $dialog @($ID_EDIT_NAME, $ID_EDIT_PATH, $ID_EDIT_PARAMS, $ID_EDIT_OK) "Edit Button"
    Click-DialogButton $dialog $ID_EDIT_OK
}

Run-DebugOnlyCase "context-edit-save" {
    param($path)
    Write-SmokeConfig $path
    Set-ContextCommandOverride "Edit"
} {
    param($path, $app)
    Invoke-ButtonContextCommand $app "Copy"
    Set-EditButtonDialog $app "Edited" "edited-path" "edited params"
    Wait-Until {
        $button = (Read-Config $path).FolderTabs[0].buttons[0]
        $button.name -eq "Edited" -and
            $button.path -eq "edited-path" -and
            $button.params -eq "edited params" -and
            $button.action -eq 1
    } "Context menu Edit did not persist the edited button"
}

Run-PhysicalInputCase "context-edit-save-physical-pointer" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-PhysicalButtonContextItem $app "Copy" 0
    Set-EditButtonDialog $app "PointerEdited" "pointer-edited-path" "pointer params"
    Wait-Until {
        $button = (Read-Config $path).FolderTabs[0].buttons[0]
        $button.name -eq "PointerEdited" -and
            $button.path -eq "pointer-edited-path" -and
            $button.params -eq "pointer params" -and
            $button.action -eq 1
    } "Physical pointer context menu Edit did not persist the edited button"
}

Run-DebugOnlyCase "context-hide" {
    param($path)
    $folder = Join-Path $WorkDir "context-hide-folder"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder "alpha.txt") -Value "a"
    Set-Content -LiteralPath (Join-Path $folder "zulu.txt") -Value "z"
    Write-FolderConfig $path $folder
    Set-ContextCommandOverride "Hide"
} {
    param($path, $app)
    $folder = Join-Path $WorkDir "context-hide-folder"
    $expected = (Join-Path $folder "zulu.txt").ToLowerInvariant()
    Invoke-ButtonContextCommand $app "Zulu"
    Wait-Until {
        (((Read-Config $path).FolderTabs[0].hidden_item_ids | ForEach-Object { $_ }) -join ",") -eq $expected
    } "Context menu Hide did not persist the hidden item"
}

Run-DebugOnlyCase "context-open-explorer-missing" {
    param($path)
    $missing = Join-Path $WorkDir "missing-explorer-target.txt"
    Write-OpenExplorerMissingConfig $path $missing
    Set-ContextCommandOverride "OpenInExplorer"
} {
    param($path, $app)
    Invoke-ButtonContextCommand $app "MissingExplorer"
    Send-DialogCommandByClass $app "#32770" $IDOK
    Invoke-MenuCommand $app $Menu.AddManualTab
    Wait-Until { (Read-Config $path).FolderTabs.Count -eq 2 } "Main window did not resume commands after Open in Explorer failure"
}

Run-DebugOnlyCase "context-open-explorer-folder-success" {
    param($path)
    $folder = Join-Path $WorkDir "open-explorer-folder-target"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Write-OpenExplorerFolderConfig $path $folder
    Set-ContextCommandOverride "OpenInExplorer"
} {
    param($path, $app)
    $folder = Join-Path $WorkDir "open-explorer-folder-target"
    $explorerWindow = $null
    try {
        Invoke-ButtonContextCommand $app "OpenFolder"
        $explorerWindow = Wait-ExplorerFolderWindow $folder
    } finally {
        if ($null -ne $explorerWindow) {
            $explorerWindow.Quit()
        }
    }
    Invoke-MenuCommand $app $Menu.AddManualTab
    Wait-Until { (Read-Config $path).FolderTabs.Count -eq 2 } "Main window did not resume commands after Open in Explorer success"
}

Run-AppCase "button-drag-swap" { param($path) Write-DragConfig $path } {
    param($path, $app)
    Invoke-ButtonDragSwap $app "First" "Second"
    Wait-Until {
        (((Read-Config $path).FolderTabs[0].buttons | ForEach-Object { $_.name }) -join ",") -eq "Second,First"
    } "Button drag did not swap and persist button slots"
}

Run-PhysicalInputCase "button-physical-drag-swap" { param($path) Write-DragConfig $path } {
    param($path, $app)
    Invoke-ButtonPhysicalDragSwap $app "First" "Second"
    Wait-Until {
        (((Read-Config $path).FolderTabs[0].buttons | ForEach-Object { $_.name }) -join ",") -eq "Second,First"
    } "Physical button drag did not swap and persist button slots"
}

Run-AppCase "button-admin-directory-error" {
    param($path)
    $dir = Join-Path $WorkDir "admin-directory-target"
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    Write-AdminDirectoryConfig $path $dir
} {
    param($path, $app)
    Invoke-ButtonClickAsync $app "AdminDir"
    Send-DialogCommandByTitle $app "Run as administrator" $IDOK
    Invoke-MenuCommand $app $Menu.DarkTheme
    Wait-Until { [bool]((Read-Config $path).Window.DarkTheme) } "Main window did not resume commands after admin directory error"
}

Run-AppCase "button-copy-click" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Set-Clipboard -Value "before-copy-smoke"
    Invoke-ButtonClick $app "Copy"
    Wait-Until { (Get-Clipboard -Raw) -eq "safe text" } "Button Copy did not update the clipboard"
}

Run-AppCase "button-launch-click" {
    param($path)
    $tool = Join-Path $WorkDir "launch-button.cmd"
    Set-Content -LiteralPath $tool -Encoding ASCII -Value @(
        "@echo off",
        "echo %*>launch-marker.txt"
    )
    Write-LaunchConfig $path $tool
} {
    param($path, $app)
    $marker = Join-Path $WorkDir "launch-marker.txt"
    Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue
    Invoke-ButtonClick $app "Launch"
    Wait-Until {
        (Test-Path -LiteralPath $marker) -and ((Get-Content -LiteralPath $marker -Raw).Trim() -eq "launch-ok")
    } "Button Launch did not run the smoke command"
}

Run-AppCase "dark-theme" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.DarkTheme
    Wait-Until { [bool]((Read-Config $path).Window.DarkTheme) } "Dark Theme did not persist"
}

Run-AppCase "about-dialog" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.About
    $dialog = Wait-ProcessWindow -ProcessId $app.Process.Id -Title "About j3Launcher"
    Assert-Equal (Get-DialogItemText $dialog $ID_ABOUT_NAME) "j3Launcher" "About app name"
    Assert-Equal (Get-DialogItemText $dialog $ID_ABOUT_VERSION) ("Version " + (Get-CargoVersion)) "About version"
    Assert-Equal (Get-DialogItemText $dialog $ID_ABOUT_LINK) "https://github.com/edgarp9" "About link"
    Click-DialogButton $dialog $ID_ABOUT_CLOSE
    Invoke-MenuCommand $app $Menu.DarkTheme
    Wait-Until { [bool]((Read-Config $path).Window.DarkTheme) } "Main window did not resume commands after About"
}

Run-AppCase "exit" { param($path) Write-SmokeConfig $path } {
    param($path, $app)
    Invoke-MenuCommand $app $Menu.Exit
    Wait-Until { $app.Process.Refresh(); $app.Process.HasExited } "Exit did not close the app"
}

if ($Only.Count -gt 0 -and $script:RunAppCaseCount -eq 0) {
    throw "no smoke case matched -Only: $($Only -join ',')"
}

Write-Host "windows ui smoke: ok"
