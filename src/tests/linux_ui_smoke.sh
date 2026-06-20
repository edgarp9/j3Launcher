#!/usr/bin/env bash
set -euo pipefail

APP="${APP:-target/debug/j3launcher}"
WINDOW_NAME="${WINDOW_NAME:-j3Launcher}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-8}"
GDK_BACKEND="${J3LAUNCHER_UI_SMOKE_BACKEND:-x11}"
export GDK_BACKEND
GTK_IM_MODULE="${J3LAUNCHER_UI_SMOKE_GTK_IM_MODULE:-gtk-im-context-simple}"
export GTK_IM_MODULE
SKIP_DEBUG_ONLY="${J3LAUNCHER_UI_SMOKE_SKIP_DEBUG_ONLY:-0}"

require_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required tool: $1" >&2
        exit 2
    fi
}

require_tool xdotool
require_tool jq
require_tool update-desktop-database

if [[ ! -x "$APP" ]]; then
    echo "app binary is not executable: $APP" >&2
    exit 2
fi

require_debug_app_for_picker_override() {
    case "$APP" in
        target/debug/*|*/target/debug/*)
            ;;
        *)
            echo "folder picker override smoke requires a debug build APP, got: $APP" >&2
            exit 2
            ;;
    esac
}

run_debug_only_smokes() {
    [[ "$SKIP_DEBUG_ONLY" != "1" ]]
}

skip_debug_only_smoke() {
    echo "$1: skipped (debug-only smoke disabled)"
}

tmpdir="$(mktemp -d)"
app_pid=""
window_id=""

dump_logs_on_failure() {
    if [[ "${J3LAUNCHER_UI_SMOKE_DUMP_LOGS:-0}" != "1" ]]; then
        return
    fi

    local found_log=0
    while IFS= read -r log_path; do
        found_log=1
        echo "---- $log_path ----" >&2
        cat "$log_path" >&2 || true
    done < <(find "$tmpdir" -maxdepth 1 -type f -name '*.log' -print | sort)

    if [[ "$found_log" == "0" ]]; then
        echo "no smoke logs found in $tmpdir" >&2
    fi
}

assert_no_unexpected_app_log_errors() {
    local diagnostics_path="$tmpdir/unexpected-app-log-diagnostics.txt"
    local pattern='(CRITICAL|ERROR|thread .* panicked|panicked at|stack backtrace)'
    local allowed='Unable to connect to the accessibility bus'
    local found=0
    local log_path

    : >"$diagnostics_path"
    while IFS= read -r log_path; do
        local matches
        matches="$(grep -E "$pattern" "$log_path" | grep -Ev "$allowed" || true)"
        if [[ -n "$matches" ]]; then
            found=1
            {
                echo "---- $log_path ----"
                printf '%s\n' "$matches"
            } >>"$diagnostics_path"
        fi
    done < <(find "$tmpdir" -maxdepth 1 -type f -name '*.log' -print | sort)

    if [[ "$found" != "0" ]]; then
        echo "unexpected app log diagnostics found:" >&2
        cat "$diagnostics_path" >&2
        exit 1
    fi
}

cleanup() {
    local status=$?
    unset J3LAUNCHER_TEST_PICK_FOLDER J3LAUNCHER_TEST_PICK_FOLDER_ERROR \
        J3LAUNCHER_TEST_SCAN_DELAY_MS J3LAUNCHER_TEST_SCAN_DELAY_MARKER || true
    if [[ "$status" != "0" ]]; then
        dump_logs_on_failure
    fi
    if [[ -n "$app_pid" ]] && kill -0 "$app_pid" >/dev/null 2>&1; then
        kill "$app_pid" >/dev/null 2>&1 || true
        wait "$app_pid" >/dev/null 2>&1 || true
    fi
    rm -rf "$tmpdir"
}
trap cleanup EXIT

write_smoke_config() {
    local config_path="$1"
    cat >"$config_path" <<'JSON'
{
  "Window": {
    "Geometry": "360x260+100+100"
  },
  "FolderTabs": [
    {
      "id": "tab-one",
      "tab_type": "manual",
      "title": "One",
      "folder_path": "",
      "rows": 1,
      "cols": 2,
      "hidden_item_ids": [],
      "slot_positions": {},
      "buttons": [
        {
          "item_id": "",
          "source_name": "",
          "source_path": "",
          "is_dir": false,
          "name": "Copy",
          "path": "safe",
          "params": "text",
          "admin": false,
          "action": 1,
          "auto_enter": false
        }
      ]
    },
    {
      "id": "tab-two",
      "tab_type": "manual",
      "title": "Two",
      "folder_path": "",
      "rows": 1,
      "cols": 2,
      "hidden_item_ids": [],
      "slot_positions": {},
      "buttons": []
    }
  ]
}
JSON
}

write_single_tab_smoke_config() {
    local config_path="$1"
    local tmp_path="${config_path}.tmp"
    write_smoke_config "$config_path"
    jq '.FolderTabs = [.FolderTabs[0]]' "$config_path" >"$tmp_path"
    mv "$tmp_path" "$config_path"
}

write_drag_smoke_config() {
    local config_path="$1"
    cat >"$config_path" <<'JSON'
{
  "Window": {
    "Geometry": "360x260+100+100"
  },
  "FolderTabs": [
    {
      "id": "tab-one",
      "tab_type": "manual",
      "title": "One",
      "folder_path": "",
      "rows": 1,
      "cols": 2,
      "hidden_item_ids": [],
      "slot_positions": {},
      "buttons": [
        {
          "item_id": "",
          "source_name": "",
          "source_path": "",
          "is_dir": false,
          "name": "First",
          "path": "first",
          "params": "",
          "admin": false,
          "action": 1,
          "auto_enter": false
        },
        {
          "item_id": "",
          "source_name": "",
          "source_path": "",
          "is_dir": false,
          "name": "Second",
          "path": "second",
          "params": "",
          "admin": false,
          "action": 1,
          "auto_enter": false
        }
      ]
    }
  ]
}
JSON
}

write_sort_smoke_config() {
    local config_path="$1"
    cat >"$config_path" <<'JSON'
{
  "Window": {
    "Geometry": "360x260+100+100"
  },
  "FolderTabs": [
    {
      "id": "tab-one",
      "tab_type": "folder",
      "title": "One",
      "folder_path": "/tmp",
      "rows": 1,
      "cols": 3,
      "hidden_item_ids": [],
      "slot_positions": {},
      "buttons": [
        {
          "item_id": "b",
          "source_name": "b.txt",
          "source_path": "/tmp/b.txt",
          "is_dir": false,
          "name": "Zulu",
          "path": "/tmp/b.txt",
          "params": "",
          "admin": false,
          "action": 0,
          "auto_enter": false
        },
        {
          "item_id": "a",
          "source_name": "a.txt",
          "source_path": "/tmp/a.txt",
          "is_dir": false,
          "name": "Alpha",
          "path": "/tmp/a.txt",
          "params": "",
          "admin": false,
          "action": 0,
          "auto_enter": false
        }
      ]
    }
  ]
}
JSON
}

write_refresh_smoke_config() {
    local config_path="$1"
    local folder_path="$2"
    jq -n --arg folder "$folder_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "folder",
          title: "One",
          folder_path: $folder,
          rows: 1,
          cols: 3,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        }
      ]
    }' >"$config_path"
}

write_active_scan_guard_config() {
    local config_path="$1"
    local folder_path="$2"
    jq -n --arg folder "$folder_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-scan",
          tab_type: "folder",
          title: "Scan",
          folder_path: $folder,
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "first",
              source_name: "first.txt",
              source_path: ($folder + "/first.txt"),
              is_dir: false,
              name: "First",
              path: ($folder + "/first.txt"),
              params: "",
              admin: false,
              action: 0,
              auto_enter: false
            },
            {
              item_id: "second",
              source_name: "second.txt",
              source_path: ($folder + "/second.txt"),
              is_dir: false,
              name: "Second",
              path: ($folder + "/second.txt"),
              params: "",
              admin: false,
              action: 0,
              auto_enter: false
            }
          ]
        },
        {
          id: "tab-manual",
          tab_type: "manual",
          title: "Manual",
          folder_path: "",
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        }
      ]
    }' >"$config_path"
}

write_duplicate_folder_focus_config() {
    local config_path="$1"
    local existing_folder="$2"
    local other_folder="$3"
    jq -n --arg existing "$existing_folder" --arg other "$other_folder" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "Manual",
          folder_path: "",
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        },
        {
          id: "tab-existing",
          tab_type: "folder",
          title: "ExistingFolder",
          folder_path: $existing,
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        },
        {
          id: "tab-other",
          tab_type: "folder",
          title: "OtherFolder",
          folder_path: $other,
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        }
      ]
    }' >"$config_path"
}

write_set_folder_duplicate_focus_config() {
    local config_path="$1"
    local current_folder="$2"
    local existing_folder="$3"
    jq -n --arg current "$current_folder" --arg existing "$existing_folder" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-current",
          tab_type: "folder",
          title: "CurrentFolder",
          folder_path: $current,
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        },
        {
          id: "tab-existing",
          tab_type: "folder",
          title: "ExistingFolder",
          folder_path: $existing,
          rows: 1,
          cols: 2,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: []
        }
      ]
    }' >"$config_path"
}

write_reset_smoke_config() {
    local config_path="$1"
    local folder_path="$2"
    jq -n --arg folder "$folder_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "folder",
          title: "One",
          folder_path: $folder,
          rows: 1,
          cols: 3,
          hidden_item_ids: ["missing-hidden"],
          slot_positions: {},
          buttons: [
            {
              item_id: "b",
              source_name: "b.txt",
              source_path: ($folder + "/b.txt"),
              is_dir: false,
              name: "CustomB",
              path: ($folder + "/b.txt"),
              params: "--old",
              admin: false,
              action: 0,
              auto_enter: false
            },
            {
              item_id: "a",
              source_name: "a.txt",
              source_path: ($folder + "/a.txt"),
              is_dir: false,
              name: "CustomA",
              path: ($folder + "/a.txt"),
              params: "--old",
              admin: false,
              action: 0,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_hidden_smoke_config() {
    local config_path="$1"
    local folder_path="$2"
    jq -n --arg folder "$folder_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "folder",
          title: "One",
          folder_path: $folder,
          rows: 1,
          cols: 3,
          hidden_item_ids: ["hidden"],
          slot_positions: {},
          buttons: [
            {
              item_id: "visible",
              source_name: "visible.txt",
              source_path: ($folder + "/visible.txt"),
              is_dir: false,
              name: "Visible",
              path: ($folder + "/visible.txt"),
              params: "",
              admin: false,
              action: 0,
              auto_enter: false
            },
            {
              item_id: "hidden",
              source_name: "hidden.txt",
              source_path: ($folder + "/hidden.txt"),
              is_dir: false,
              name: "Hidden",
              path: ($folder + "/hidden.txt"),
              params: "",
              admin: false,
              action: 0,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_launch_smoke_config() {
    local config_path="$1"
    local tool_path="$2"
    local params='alpha "two words" "" tail'
    local admin="${3:-false}"
    jq -n --arg tool "$tool_path" --arg params "$params" --argjson admin "$admin" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "One",
          folder_path: "",
          rows: 1,
          cols: 1,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "",
              source_name: "",
              source_path: "",
              is_dir: false,
              name: "Launch",
              path: $tool,
              params: $params,
              admin: $admin,
              action: 0,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_associated_open_smoke_config() {
    local config_path="$1"
    local file_path="$2"
    jq -n --arg file "$file_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "One",
          folder_path: "",
          rows: 1,
          cols: 1,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "",
              source_name: "",
              source_path: "",
              is_dir: false,
              name: "Associated",
              path: $file,
              params: "--ignored-by-gio",
              admin: false,
              action: 0,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_protocol_open_smoke_config() {
    local config_path="$1"
    local uri="$2"
    jq -n --arg uri "$uri" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "One",
          folder_path: "",
          rows: 1,
          cols: 1,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "",
              source_name: "",
              source_path: "",
              is_dir: false,
              name: "Protocol",
              path: $uri,
              params: "--ignored-by-gio",
              admin: false,
              action: 0,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_copy_smoke_config() {
    local config_path="$1"
    local copy_path="$2"
    local copy_params="$3"
    jq -n --arg path "$copy_path" --arg params "$copy_params" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "One",
          folder_path: "",
          rows: 1,
          cols: 1,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "",
              source_name: "",
              source_path: "",
              is_dir: false,
              name: "Copy",
              path: $path,
              params: $params,
              admin: false,
              action: 1,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_open_guard_smoke_config() {
    local config_path="$1"
    local windows_path='C:\Tools\app.exe'
    jq -n --arg path "$windows_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "One",
          folder_path: "",
          rows: 1,
          cols: 1,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "",
              source_name: "",
              source_path: "",
              is_dir: false,
              name: "OpenGuard",
              path: $path,
              params: "",
              admin: false,
              action: 1,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

write_open_folder_smoke_config() {
    local config_path="$1"
    local folder_path="$2"
    jq -n --arg folder "$folder_path" '{
      Window: { Geometry: "360x260+100+100" },
      FolderTabs: [
        {
          id: "tab-one",
          tab_type: "manual",
          title: "One",
          folder_path: "",
          rows: 1,
          cols: 1,
          hidden_item_ids: [],
          slot_positions: {},
          buttons: [
            {
              item_id: "",
              source_name: "",
              source_path: "",
              is_dir: true,
              name: "OpenFolder",
              path: $folder,
              params: "",
              admin: false,
              action: 1,
              auto_enter: false
            }
          ]
        }
      ]
    }' >"$config_path"
}

wait_for_window() {
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    local found=""
    local window_pid=""
    while ((SECONDS < deadline)); do
        while IFS= read -r found; do
            window_pid="$(xdotool getwindowpid "$found" 2>/dev/null || true)"
            if [[ "$window_pid" == "$app_pid" ]]; then
                echo "$found"
                return 0
            fi
        done < <(xdotool search --onlyvisible --name "$WINDOW_NAME" 2>/dev/null || true)
        sleep 0.1
    done
    return 1
}

focus_app_window() {
    xdotool windowactivate --sync "$window_id" || true
    xdotool windowfocus --sync "$window_id" || true
    sleep 0.1
}

clear_modifiers() {
    xdotool keyup Control_L keyup Control_R keyup Shift_L keyup Shift_R \
        keyup Alt_L keyup Alt_R keyup Super_L keyup Super_R >/dev/null 2>&1 || true
}

start_app() {
    local config_path="$1"
    local log_path="$2"
    "$APP" "$config_path" >"$log_path" 2>&1 &
    app_pid="$!"
    window_id="$(wait_for_window)" || {
        echo "failed to find j3Launcher window; log follows:" >&2
        cat "$log_path" >&2 || true
        exit 1
    }
    focus_app_window
    xdotool windowmove "$window_id" 100 100
    xdotool windowsize "$window_id" 360 260
    focus_app_window
    sleep 0.2
}

stop_app() {
    if [[ -n "$app_pid" ]] && kill -0 "$app_pid" >/dev/null 2>&1; then
        kill "$app_pid" >/dev/null 2>&1 || true
        wait "$app_pid" >/dev/null 2>&1 || true
    fi
    app_pid=""
    window_id=""
}

tab_titles() {
    jq -r '.FolderTabs[].title' "$1" | paste -sd ',' -
}

assert_titles() {
    local config_path="$1"
    local expected="$2"
    local actual
    actual="$(tab_titles "$config_path")"
    if [[ "$actual" != "$expected" ]]; then
        echo "unexpected tab order in $config_path: expected '$expected', got '$actual'" >&2
        exit 1
    fi
}

wait_for_titles() {
    local config_path="$1"
    local expected="$2"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while ((SECONDS < deadline)); do
        if [[ "$(tab_titles "$config_path")" == "$expected" ]]; then
            return 0
        fi
        sleep 0.1
    done
    assert_titles "$config_path" "$expected"
}

press_key_until_titles() {
    local config_path="$1"
    local key_combo="$2"
    local expected="$3"
    local attempt
    for attempt in 1 2 3; do
        focus_app_window
        sleep 0.1
        press_key_combo "$key_combo"
        local deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(tab_titles "$config_path")" == "$expected" ]]; then
                return 0
            fi
            sleep 0.1
        done
    done
    assert_titles "$config_path" "$expected"
}

press_select_next_then_move_left_until_titles() {
    local config_path="$1"
    local expected="$2"
    local attempt
    for attempt in 1 2 3 4 5; do
        focus_app_window
        press_key_combo ctrl+Next
        press_key_combo ctrl+shift+Left
        if [[ "$(tab_titles "$config_path")" == "$expected" ]]; then
            return 0
        fi
    done
    assert_titles "$config_path" "$expected"
}

press_key_combo() {
    local key_combo="$1"
    clear_modifiers
    case "$key_combo" in
        ctrl+shift+Left)
            xdotool keydown Control_L keydown Shift_L key Left keyup Shift_L keyup Control_L
            ;;
        ctrl+shift+Right)
            xdotool keydown Control_L keydown Shift_L key Right keyup Shift_L keyup Control_L
            ;;
        ctrl+Prior)
            xdotool keydown Control_L key Prior keyup Control_L
            ;;
        ctrl+Next)
            xdotool keydown Control_L key Next keyup Control_L
            ;;
        *)
            xdotool key --delay 80 --clearmodifiers "$key_combo"
            ;;
    esac
    sleep 0.1
    clear_modifiers
}

json_value() {
    local config_path="$1"
    local query="$2"
    jq -r "$query" "$config_path"
}

assert_json_value() {
    local config_path="$1"
    local query="$2"
    local expected="$3"
    local actual
    actual="$(json_value "$config_path" "$query")"
    if [[ "$actual" != "$expected" ]]; then
        echo "unexpected JSON value in $config_path for $query: expected '$expected', got '$actual'" >&2
        exit 1
    fi
}

wait_for_json_value() {
    local config_path="$1"
    local query="$2"
    local expected="$3"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while ((SECONDS < deadline)); do
        if [[ "$(json_value "$config_path" "$query")" == "$expected" ]]; then
            return 0
        fi
        sleep 0.1
    done
    assert_json_value "$config_path" "$query" "$expected"
}

assert_file_content() {
    local path="$1"
    local expected="$2"
    local actual
    actual="$(cat "$path" 2>/dev/null || true)"
    if [[ "$actual" != "$expected" ]]; then
        echo "unexpected file content in $path: expected '$expected', got '$actual'" >&2
        exit 1
    fi
}

wait_for_file_content() {
    local path="$1"
    local expected="$2"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while ((SECONDS < deadline)); do
        if [[ -f "$path" ]] && [[ "$(cat "$path")" == "$expected" ]]; then
            return 0
        fi
        sleep 0.1
    done
    assert_file_content "$path" "$expected"
}

wait_for_named_window() {
    local name="$1"
    local timeout="${2:-$TIMEOUT_SECONDS}"
    local deadline=$((SECONDS + timeout))
    local found=""
    while ((SECONDS < deadline)); do
        found="$(xdotool search --onlyvisible --name "$name" 2>/dev/null | tail -n 1 || true)"
        if [[ -n "$found" ]]; then
            echo "$found"
            return 0
        fi
        sleep 0.1
    done
    return 1
}

open_rename_tab_dialog() {
    local log_path="$1"
    local context="$2"
    local rename_window=""
    local attempt
    for attempt in 1 2 3; do
        local down_count
        for down_count in 3 4; do
            activate_file_menu_item "$down_count" >/dev/null 2>&1
            if rename_window="$(wait_for_named_window "Rename Tab" 2)"; then
                echo "$rename_window"
                return 0
            fi
            focus_app_window >/dev/null 2>&1
            xdotool key Escape >/dev/null 2>&1 || true
            sleep 0.2
        done
    done

    echo "Rename Tab dialog did not open $context; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

open_context_edit_dialog() {
    local log_path="$1"
    local context="$2"
    local edit_window=""
    local attempt
    for attempt in 1 2 3 4 5; do
        focus_app_window >/dev/null 2>&1
        clear_modifiers
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.1
        xdotool mousemove --window "$window_id" 70 96 click 1 >/dev/null 2>&1
        sleep 0.2
        xdotool key --clearmodifiers shift+F10 >/dev/null 2>&1
        sleep 0.3
        xdotool key --clearmodifiers Return >/dev/null 2>&1
        if edit_window="$(wait_for_named_window "Edit Button" 2)"; then
            echo "$edit_window"
            return 0
        fi
        focus_app_window >/dev/null 2>&1
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done

    echo "Edit Button dialog did not open $context; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

open_pointer_context_edit_dialog() {
    local log_path="$1"
    local context="$2"
    local edit_window=""
    local attempt
    for attempt in 1 2 3 4 5; do
        focus_app_window >/dev/null 2>&1
        clear_modifiers
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.1
        xdotool mousemove --window "$window_id" 70 96 click 3 >/dev/null 2>&1
        sleep 0.3
        xdotool key --clearmodifiers Return >/dev/null 2>&1
        if edit_window="$(wait_for_named_window "Edit Button" 2)"; then
            echo "$edit_window"
            return 0
        fi
        focus_app_window >/dev/null 2>&1
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done

    echo "Edit Button dialog did not open $context; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

open_file_menu_dialog() {
    local log_path="$1"
    local context="$2"
    local down_count="$3"
    local title="$4"
    local dialog_window=""
    local attempt
    for attempt in 1 2 3 4 5; do
        activate_file_menu_item "$down_count" >/dev/null 2>&1
        if dialog_window="$(wait_for_named_window "$title" 2)"; then
            echo "$dialog_window"
            return 0
        fi
        focus_app_window >/dev/null 2>&1
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done

    echo "$title dialog did not open $context; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

open_file_menu_dialog_any() {
    local log_path="$1"
    local context="$2"
    local title="$3"
    shift 3
    local dialog_window=""
    local attempt down_count
    for attempt in 1 2 3 4 5; do
        for down_count in "$@"; do
            activate_file_menu_item "$down_count" >/dev/null 2>&1
            if dialog_window="$(wait_for_named_window "$title" 2)"; then
                echo "$dialog_window"
                return 0
            fi
            focus_app_window >/dev/null 2>&1
            xdotool key Escape >/dev/null 2>&1 || true
            sleep 0.2
        done
    done

    echo "$title dialog did not open $context; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

open_file_menu_message() {
    local log_path="$1"
    local context="$2"
    local down_count="$3"
    local title="$4"
    local message_window=""
    local attempt
    for attempt in 1 2 3 4 5; do
        activate_file_menu_item "$down_count" >/dev/null 2>&1
        if message_window="$(wait_for_named_window "$title" 2)"; then
            echo "$message_window"
            return 0
        fi
        focus_app_window >/dev/null 2>&1
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done

    echo "$context dialog did not open; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

wait_for_app_exit() {
    if app_exited_within "$TIMEOUT_SECONDS"; then
        return 0
    fi
    echo "app did not exit within ${TIMEOUT_SECONDS}s" >&2
    exit 1
}

app_exited_within() {
    local timeout="$1"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    if [[ "$timeout" != "$TIMEOUT_SECONDS" ]]; then
        deadline=$((SECONDS + timeout))
    fi
    while ((SECONDS < deadline)); do
        if [[ -z "$app_pid" ]] || ! kill -0 "$app_pid" >/dev/null 2>&1; then
            app_pid=""
            window_id=""
            return 0
        fi
        sleep 0.1
    done
    return 1
}

replace_text_input_dialog_value() {
    local dialog_window="$1"
    local value="$2"
    xdotool windowactivate --sync "$dialog_window"
    sleep 0.2
    xdotool mousemove --window "$dialog_window" 140 76 click 1
    sleep 0.1
    clear_modifiers
    xdotool key ctrl+a
    xdotool type --delay 1 "$value"
    xdotool key Return
}

activate_file_menu_item() {
    local down_count="$1"
    focus_app_window
    clear_modifiers
    xdotool key Escape || true
    sleep 0.2
    xdotool mousemove --window "$window_id" 18 13
    sleep 0.1
    xdotool click 1
    sleep 0.6
    clear_modifiers
    xdotool key --clearmodifiers Home
    sleep 0.15
    xdotool key --clearmodifiers Home
    sleep 0.15
    local index
    for ((index = 0; index < down_count; index += 1)); do
        xdotool key --clearmodifiers Down
        sleep 0.05
    done
    xdotool key --clearmodifiers Return
    sleep 0.2
}

run_disabled_move_left_smoke() {
    local config_path="$tmpdir/disabled-left.json"
    local log_path="$tmpdir/disabled-left.log"
    write_single_tab_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    focus_app_window
    press_key_combo ctrl+Prior
    sleep 0.2
    press_key_combo ctrl+shift+Left
    sleep 0.3
    stop_app
    assert_titles "$config_path" "One"
    echo "disabled accelerator smoke: ok"
}

run_move_right_smoke() {
    local config_path="$tmpdir/move-right.json"
    local log_path="$tmpdir/move-right.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    press_key_until_titles "$config_path" ctrl+shift+Right "Two,One"
    stop_app
    echo "enabled accelerator smoke: ok"
}

run_resize_focus_repeated_accelerator_smoke() {
    local config_path="$tmpdir/resize-focus-wide.json"
    local log_path="$tmpdir/resize-focus-wide.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    xdotool windowsize "$window_id" 520 360
    sleep 0.4
    press_key_until_titles "$config_path" ctrl+shift+Right "Two,One"
    stop_app

    config_path="$tmpdir/resize-focus-narrow.json"
    log_path="$tmpdir/resize-focus-narrow.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    xdotool windowsize "$window_id" 320 220
    sleep 0.4
    press_select_next_then_move_left_until_titles "$config_path" "Two,One"
    stop_app

    config_path="$tmpdir/resize-focus-repeat.json"
    log_path="$tmpdir/resize-focus-repeat.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    xdotool windowsize "$window_id" 420 300
    sleep 0.4
    press_select_next_then_move_left_until_titles "$config_path" "Two,One"

    stop_app
    echo "resize/focus repeated accelerator smoke: ok"
}

run_keyboard_context_menu_smoke() {
    local config_path="$tmpdir/context-menu.json"
    local log_path="$tmpdir/context-menu.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local edit_window
    edit_window="$(open_context_edit_dialog "$log_path" "from keyboard context menu")"
    xdotool windowactivate --sync "$edit_window" || true
    xdotool key Escape || true
    sleep 0.2
    stop_app
    echo "keyboard context menu smoke: ok"
}

run_context_menu_repeated_dismiss_smoke() {
    local config_path="$tmpdir/context-menu-dismiss.json"
    local log_path="$tmpdir/context-menu-dismiss.log"
    local attempt
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    for attempt in 1 2 3; do
        focus_app_window
        xdotool mousemove --window "$window_id" 36 96 click 1
        sleep 0.2
        xdotool key shift+F10
        sleep 0.2
        xdotool key Escape
        sleep 0.2
    done

    local edit_window
    edit_window="$(open_context_edit_dialog "$log_path" "after repeated context menu dismiss")"
    xdotool windowactivate --sync "$edit_window"
    xdotool key Escape
    sleep 0.2

    assert_titles "$config_path" "One,Two"
    stop_app
    echo "context menu repeated dismiss smoke: ok"
}

run_pointer_context_menu_smoke() {
    local config_path="$tmpdir/pointer-context-menu.json"
    local log_path="$tmpdir/pointer-context-menu.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local edit_window
    edit_window="$(open_pointer_context_edit_dialog "$log_path" "from pointer context menu")"
    xdotool windowactivate --sync "$edit_window"
    xdotool key Escape
    sleep 0.2
    stop_app
    echo "pointer context menu smoke: ok"
}

run_button_drag_drop_smoke() {
    local config_path="$tmpdir/button-drag-drop.json"
    local log_path="$tmpdir/button-drag-drop.log"
    write_drag_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local query='.FolderTabs[0].buttons | map(.name) | join(",")'
    local attempt deadline
    for attempt in 1 2 3 4 5; do
        focus_app_window
        clear_modifiers
        xdotool mouseup 1 >/dev/null 2>&1 || true
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.1
        xdotool mousemove --window "$window_id" 70 96
        sleep 0.1
        xdotool mousedown 1
        sleep 0.5
        xdotool mousemove --window "$window_id" 105 96
        sleep 0.2
        xdotool mousemove --window "$window_id" 150 96
        sleep 0.2
        xdotool mousemove --window "$window_id" 220 96
        sleep 0.2
        xdotool mousemove --window "$window_id" 280 96
        sleep 0.8
        xdotool mouseup 1

        deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(json_value "$config_path" "$query")" == "Second,First" ]]; then
                stop_app
                echo "button drag/drop smoke: ok"
                return
            fi
            sleep 0.1
        done
    done

    assert_json_value "$config_path" "$query" "Second,First"
    stop_app
    echo "button drag/drop smoke: ok"
}

run_context_menu_edit_save_smoke() {
    local config_path="$tmpdir/context-menu-edit-save.json"
    local log_path="$tmpdir/context-menu-edit-save.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local edit_window
    edit_window="$(open_pointer_context_edit_dialog "$log_path" "for edit-save smoke")"
    xdotool windowactivate --sync "$edit_window"
    sleep 0.2
    xdotool mousemove --window "$edit_window" 140 32 click 1
    sleep 0.1
    xdotool key ctrl+a
    xdotool type --delay 1 Edited
    xdotool key Tab
    xdotool key ctrl+a
    xdotool type --delay 1 edited-path
    xdotool key Tab
    xdotool key ctrl+a
    xdotool type --delay 1 edited-params
    xdotool key Tab
    xdotool key space
    xdotool key Tab
    xdotool key space
    xdotool key Return

    wait_for_json_value "$config_path" '.FolderTabs[0].buttons[0].name' "Edited"
    assert_json_value "$config_path" '.FolderTabs[0].buttons[0].path' "edited-path"
    assert_json_value "$config_path" '.FolderTabs[0].buttons[0].params' "edited-params"
    assert_json_value "$config_path" '.FolderTabs[0].buttons[0].admin' "true"
    assert_json_value "$config_path" '.FolderTabs[0].buttons[0].action' "0"
    stop_app
    echo "context menu edit save smoke: ok"
}

run_context_menu_open_guard_smoke() {
    local config_path="$tmpdir/context-menu-open-guard.json"
    local log_path="$tmpdir/context-menu-open-guard.log"
    local attempt
    write_open_guard_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    for attempt in 1 2 3; do
        focus_app_window
        xdotool mousemove --window "$window_id" 36 96 click 1
        sleep 0.2
        xdotool key shift+F10
        sleep 0.3
        xdotool key Home
        xdotool key Down
        xdotool key Return

        local message_window
        if message_window="$(wait_for_named_window "탐색기에서 열기" 2)"; then
            xdotool windowactivate --sync "$message_window"
            xdotool key Return
            sleep 0.2
            assert_json_value "$config_path" '.FolderTabs[0].buttons[0].path' 'C:\Tools\app.exe'
            stop_app
            echo "context menu open guard smoke: ok"
            return
        fi
        focus_app_window
        xdotool key Escape || true
        sleep 0.2
    done

    echo "Open in Explorer guard dialog did not open; log follows:" >&2
    cat "$log_path" >&2 || true
    exit 1
}

run_context_menu_open_folder_smoke() {
    local config_path="$tmpdir/context-menu-open-folder.json"
    local log_path="$tmpdir/context-menu-open-folder.log"
    local xdg_data="$tmpdir/open-folder-xdg-data"
    local xdg_config="$tmpdir/open-folder-xdg-config"
    local app_dir="$xdg_data/applications"
    local handler_dir="$tmpdir/open-folder-handler"
    local handler_path="$handler_dir/capture-folder.sh"
    local output_path="$handler_dir/open-folder.out"
    local folder_path="$handler_dir/folder-target"
    local expected_output
    local old_xdg_data="${XDG_DATA_HOME:-}"
    local old_xdg_config="${XDG_CONFIG_HOME:-}"
    local attempt
    mkdir -p "$app_dir" "$xdg_config" "$folder_path"
    cat >"$handler_path" <<'SH'
#!/usr/bin/env bash
tmp_path="${OUT_PATH}.tmp.$$"
{
    printf '[%s]\n' "$@"
    printf 'PWD=%s\n' "$PWD"
} >"$tmp_path"
mv "$tmp_path" "$OUT_PATH"
SH
    chmod +x "$handler_path"
    cat >"$app_dir/j3launcher-dir-handler.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=j3Launcher Dir Handler
Exec=env OUT_PATH=$output_path $handler_path %u
MimeType=inode/directory;
NoDisplay=true
EOF
    cat >"$xdg_config/mimeapps.list" <<'EOF'
[Default Applications]
inode/directory=j3launcher-dir-handler.desktop
EOF
    write_open_folder_smoke_config "$config_path" "$folder_path"
    expected_output=$'['"$folder_path"$']\nPWD='"$(pwd)"

    export XDG_DATA_HOME="$xdg_data"
    export XDG_CONFIG_HOME="$xdg_config"
    start_app "$config_path" "$log_path"
    if [[ -n "$old_xdg_data" ]]; then
        export XDG_DATA_HOME="$old_xdg_data"
    else
        unset XDG_DATA_HOME
    fi
    if [[ -n "$old_xdg_config" ]]; then
        export XDG_CONFIG_HOME="$old_xdg_config"
    else
        unset XDG_CONFIG_HOME
    fi

    for attempt in 1 2 3; do
        focus_app_window
        xdotool mousemove --window "$window_id" 36 96 click 1
        sleep 0.2
        xdotool key shift+F10
        sleep 0.3
        xdotool key Home
        xdotool key Down
        xdotool key Return

        local deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ -f "$output_path" ]] && [[ "$(cat "$output_path")" == "$expected_output" ]]; then
                stop_app
                echo "context menu open folder smoke: ok"
                return
            fi
            sleep 0.1
        done
        focus_app_window
        xdotool key Escape || true
        sleep 0.2
    done

    assert_file_content "$output_path" "$expected_output"
    stop_app
    echo "context menu open folder smoke: ok"
}

run_context_menu_hide_smoke() {
    local config_path="$tmpdir/context-menu-hide.json"
    local log_path="$tmpdir/context-menu-hide.log"
    local folder_path="$tmpdir/context-hide-folder"
    local edit_window=""
    local attempt
    mkdir -p "$folder_path"
    printf x >"$folder_path/visible.txt"
    printf x >"$folder_path/hidden.txt"
    write_hidden_smoke_config "$config_path" "$folder_path"
    local config_tmp="$config_path.tmp"
    jq '.FolderTabs[0].buttons[0].path = "" | .FolderTabs[0].buttons[0].source_path = ""' \
        "$config_path" >"$config_tmp"
    mv "$config_tmp" "$config_path"
    start_app "$config_path" "$log_path"

    for attempt in 1 2 3; do
        focus_app_window
        xdotool mousemove --window "$window_id" 36 96 click 1
        sleep 0.2
        xdotool key shift+F10
        sleep 0.3
        xdotool key End
        sleep 0.1
        xdotool key Return

        local deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(json_value "$config_path" '.FolderTabs[0].hidden_item_ids | join(",")')" == "hidden,visible" ]]; then
                stop_app
                echo "context menu hide smoke: ok"
                return
            fi
            sleep 0.1
        done

        edit_window="$(xdotool search --onlyvisible --name "Edit Button" 2>/dev/null | tail -n 1 || true)"
        if [[ -n "$edit_window" ]]; then
            xdotool windowactivate --sync "$edit_window" || true
            xdotool key Escape || true
            sleep 0.2
        fi
        focus_app_window
        xdotool key Escape || true
        sleep 0.2
    done

    assert_json_value "$config_path" '.FolderTabs[0].hidden_item_ids | join(",")' "hidden,visible"
    stop_app
    echo "context menu hide smoke: ok"
}

run_button_launch_smoke() {
    local config_path="$tmpdir/button-launch.json"
    local log_path="$tmpdir/button-launch.log"
    local tool_dir="$tmpdir/launch-tool"
    local tool_path="$tool_dir/capture.sh"
    local output_path="$tool_dir/launch.out"
    mkdir -p "$tool_dir"
    cat >"$tool_path" <<SH
#!/usr/bin/env bash
tmp_path="$output_path.tmp.\$\$"
{
    printf '[%s]\n' "\$@"
    printf 'PWD=%s\n' "\$PWD"
} >"\$tmp_path"
mv "\$tmp_path" "$output_path"
SH
    chmod +x "$tool_path"
    write_launch_smoke_config "$config_path" "$tool_path"
    start_app "$config_path" "$log_path"

    xdotool mousemove --window "$window_id" 36 96 click 1
    wait_for_file_content "$output_path" $'[alpha]\n[two words]\n[]\n[tail]\nPWD='"$tool_dir"

    stop_app
    echo "button launch smoke: ok"
}

run_button_admin_fake_pkexec_smoke() {
    local config_path="$tmpdir/button-admin.json"
    local log_path="$tmpdir/button-admin.log"
    local tool_dir="$tmpdir/admin-tool"
    local tool_path="$tool_dir/admin-target.sh"
    local fake_bin="$tmpdir/fake-bin"
    local pkexec_path="$fake_bin/pkexec"
    local output_path="$tool_dir/pkexec.out"
    local old_path="$PATH"
    mkdir -p "$tool_dir" "$fake_bin"
    cat >"$tool_path" <<'SH'
#!/usr/bin/env bash
exit 0
SH
    chmod +x "$tool_path"
    cat >"$pkexec_path" <<SH
#!/usr/bin/env bash
tmp_path="$output_path.tmp.\$\$"
{
    printf '[%s]\n' "\$@"
    printf 'PWD=%s\n' "\$PWD"
} >"\$tmp_path"
mv "\$tmp_path" "$output_path"
exit 0
SH
    chmod +x "$pkexec_path"
    write_launch_smoke_config "$config_path" "$tool_path" true

    PATH="$fake_bin:$old_path"
    start_app "$config_path" "$log_path"
    PATH="$old_path"

    xdotool mousemove --window "$window_id" 36 96 click 1
    wait_for_file_content "$output_path" $'['"$tool_path"$']\n[alpha]\n[two words]\n[]\n[tail]\nPWD='"$(pwd)"

    stop_app
    echo "button admin fake pkexec smoke: ok"
}

run_button_admin_fake_pkexec_cancel_smoke() {
    local config_path="$tmpdir/button-admin-cancel.json"
    local log_path="$tmpdir/button-admin-cancel.log"
    local tool_dir="$tmpdir/admin-cancel-tool"
    local tool_path="$tool_dir/admin-target.sh"
    local fake_bin="$tmpdir/fake-cancel-bin"
    local pkexec_path="$fake_bin/pkexec"
    local old_path="$PATH"
    mkdir -p "$tool_dir" "$fake_bin"
    cat >"$tool_path" <<'SH'
#!/usr/bin/env bash
exit 0
SH
    chmod +x "$tool_path"
    cat >"$pkexec_path" <<'SH'
#!/usr/bin/env bash
exit 126
SH
    chmod +x "$pkexec_path"
    write_launch_smoke_config "$config_path" "$tool_path" true

    PATH="$fake_bin:$old_path"
    start_app "$config_path" "$log_path"
    PATH="$old_path"

    xdotool mousemove --window "$window_id" 36 96 click 1
    local message_window
    message_window="$(wait_for_named_window "Run as administrator")" || {
        echo "Run as administrator cancellation dialog did not open; log follows:" >&2
        cat "$log_path" >&2 || true
        exit 1
    }
    xdotool windowactivate --sync "$message_window"
    xdotool key Return
    sleep 0.2

    stop_app
    echo "button admin fake pkexec cancel smoke: ok"
}

run_button_associated_open_smoke() {
    local config_path="$tmpdir/button-associated-open.json"
    local log_path="$tmpdir/button-associated-open.log"
    local xdg_data="$tmpdir/associated-xdg-data"
    local xdg_config="$tmpdir/associated-xdg-config"
    local app_dir="$xdg_data/applications"
    local handler_dir="$tmpdir/associated-handler"
    local handler_path="$handler_dir/capture-associated.sh"
    local output_path="$handler_dir/associated.out"
    local file_path="$handler_dir/sample.txt"
    local old_xdg_data="${XDG_DATA_HOME:-}"
    local old_xdg_config="${XDG_CONFIG_HOME:-}"
    mkdir -p "$app_dir" "$xdg_config" "$handler_dir"
    printf sample >"$file_path"
    chmod 0644 "$file_path"
    cat >"$handler_path" <<'SH'
#!/usr/bin/env bash
tmp_path="${OUT_PATH}.tmp.$$"
{
    printf '[%s]\n' "$@"
    printf 'PWD=%s\n' "$PWD"
} >"$tmp_path"
mv "$tmp_path" "$OUT_PATH"
SH
    chmod +x "$handler_path"
    cat >"$app_dir/j3launcher-smoke-handler.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=j3Launcher Smoke Handler
Exec=env OUT_PATH=$output_path $handler_path %u
MimeType=text/plain;
NoDisplay=true
EOF
    cat >"$xdg_config/mimeapps.list" <<'EOF'
[Default Applications]
text/plain=j3launcher-smoke-handler.desktop
EOF
    write_associated_open_smoke_config "$config_path" "$file_path"

    export XDG_DATA_HOME="$xdg_data"
    export XDG_CONFIG_HOME="$xdg_config"
    start_app "$config_path" "$log_path"
    if [[ -n "$old_xdg_data" ]]; then
        export XDG_DATA_HOME="$old_xdg_data"
    else
        unset XDG_DATA_HOME
    fi
    if [[ -n "$old_xdg_config" ]]; then
        export XDG_CONFIG_HOME="$old_xdg_config"
    else
        unset XDG_CONFIG_HOME
    fi

    xdotool mousemove --window "$window_id" 36 96 click 1
    wait_for_file_content "$output_path" $'['"$file_path"$']\nPWD='"$(pwd)"

    stop_app
    echo "button associated open smoke: ok"
}

run_button_protocol_open_smoke() {
    local config_path="$tmpdir/button-protocol-open.json"
    local log_path="$tmpdir/button-protocol-open.log"
    local xdg_data="$tmpdir/protocol-xdg-data"
    local xdg_config="$tmpdir/protocol-xdg-config"
    local app_dir="$xdg_data/applications"
    local handler_dir="$tmpdir/protocol-handler"
    local handler_path="$handler_dir/capture-protocol.sh"
    local output_path="$handler_dir/protocol.out"
    local uri="j3launchersmoke://open/path?value=1"
    local old_xdg_data="${XDG_DATA_HOME:-}"
    local old_xdg_config="${XDG_CONFIG_HOME:-}"
    mkdir -p "$app_dir" "$xdg_config" "$handler_dir"
    cat >"$handler_path" <<'SH'
#!/usr/bin/env bash
tmp_path="${OUT_PATH}.tmp.$$"
{
    printf '[%s]\n' "$@"
    printf 'PWD=%s\n' "$PWD"
} >"$tmp_path"
mv "$tmp_path" "$OUT_PATH"
SH
    chmod +x "$handler_path"
    cat >"$app_dir/j3launcher-protocol-handler.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=j3Launcher Protocol Handler
Exec=env OUT_PATH=$output_path $handler_path %u
MimeType=x-scheme-handler/j3launchersmoke;
NoDisplay=true
EOF
    cat >"$xdg_config/mimeapps.list" <<'EOF'
[Default Applications]
x-scheme-handler/j3launchersmoke=j3launcher-protocol-handler.desktop
EOF
    update-desktop-database "$app_dir" >/dev/null
    write_protocol_open_smoke_config "$config_path" "$uri"

    export XDG_DATA_HOME="$xdg_data"
    export XDG_CONFIG_HOME="$xdg_config"
    start_app "$config_path" "$log_path"
    if [[ -n "$old_xdg_data" ]]; then
        export XDG_DATA_HOME="$old_xdg_data"
    else
        unset XDG_DATA_HOME
    fi
    if [[ -n "$old_xdg_config" ]]; then
        export XDG_CONFIG_HOME="$old_xdg_config"
    else
        unset XDG_CONFIG_HOME
    fi

    local expected_output=$'['"$uri"$']\nPWD='"$(pwd)"
    local attempt deadline
    for attempt in 1 2 3; do
        rm -f "$output_path"
        focus_app_window
        xdotool mousemove --window "$window_id" 36 96 click 1
        deadline=$((SECONDS + 3))
        while ((SECONDS < deadline)); do
            if [[ -f "$output_path" ]] && [[ "$(cat "$output_path")" == "$expected_output" ]]; then
                stop_app
                echo "button protocol open smoke: ok"
                return
            fi
            sleep 0.1
        done
    done

    assert_file_content "$output_path" "$expected_output"
    stop_app
    echo "button protocol open smoke: ok"
}

run_button_copy_paste_smoke() {
    local config_path="$tmpdir/button-copy-paste.json"
    local log_path="$tmpdir/button-copy-paste.log"
    local copy_path="copy-$RANDOM"
    local copy_params="paste-value"
    local expected_title="$copy_path $copy_params"
    local attempt
    write_copy_smoke_config "$config_path" "$copy_path" "$copy_params"
    start_app "$config_path" "$log_path"

    for attempt in 1 2 3; do
        focus_app_window
        xdotool mousemove --window "$window_id" 36 96 click 1
        sleep 0.4
        local rename_window
        rename_window="$(open_rename_tab_dialog "$log_path" "after copy attempt $attempt")"
        xdotool windowactivate --sync "$rename_window"
        xdotool key ctrl+a
        xdotool key ctrl+v
        xdotool key Return

        local deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(json_value "$config_path" '.FolderTabs[0].title')" == "$expected_title" ]]; then
                stop_app
                echo "button copy/paste smoke: ok"
                return
            fi
            sleep 0.1
        done
    done

    wait_for_json_value "$config_path" '.FolderTabs[0].title' "$expected_title"
    stop_app
    echo "button copy/paste smoke: ok"
}

run_file_menu_add_folder_tab_smoke() {
    local config_path="$tmpdir/menu-add-folder-tab.json"
    local log_path="$tmpdir/menu-add-folder-tab.log"
    local folder_path="$tmpdir/add-folder-target"
    require_debug_app_for_picker_override
    mkdir -p "$folder_path"
    printf x >"$folder_path/a.txt"
    printf x >"$folder_path/b.txt"
    write_smoke_config "$config_path"
    export J3LAUNCHER_TEST_PICK_FOLDER="$folder_path"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    activate_file_menu_item 0

    wait_for_json_value "$config_path" '.FolderTabs | length' "3"
    assert_json_value "$config_path" '.FolderTabs[-1].folder_path' "$folder_path"
    assert_json_value "$config_path" '.FolderTabs[-1].buttons | map(.source_name) | sort | join(",")' "a.txt,b.txt"
    stop_app
    echo "file menu add folder tab smoke: ok"
}

run_file_menu_add_folder_tab_cancel_smoke() {
    local config_path="$tmpdir/menu-add-folder-tab-cancel.json"
    local log_path="$tmpdir/menu-add-folder-tab-cancel.log"
    require_debug_app_for_picker_override
    write_smoke_config "$config_path"
    export J3LAUNCHER_TEST_PICK_FOLDER="__CANCEL__"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    activate_file_menu_item 0
    sleep 0.3

    assert_json_value "$config_path" '.FolderTabs | length' "2"
    assert_titles "$config_path" "One,Two"
    stop_app
    echo "file menu add folder tab cancel smoke: ok"
}

run_file_menu_add_folder_tab_error_smoke() {
    local config_path="$tmpdir/menu-add-folder-tab-error.json"
    local log_path="$tmpdir/menu-add-folder-tab-error.log"
    require_debug_app_for_picker_override
    write_smoke_config "$config_path"
    export J3LAUNCHER_TEST_PICK_FOLDER_ERROR="no local path"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER_ERROR

    local message_window
    message_window="$(open_file_menu_message "$log_path" "Add Folder Tab error" 0 "Add Folder Tab")"
    xdotool windowactivate --sync "$message_window"
    xdotool key Return
    sleep 0.2

    assert_json_value "$config_path" '.FolderTabs | length' "2"
    assert_titles "$config_path" "One,Two"
    stop_app
    echo "file menu add folder tab error smoke: ok"
}

run_file_menu_add_folder_tab_duplicate_focus_smoke() {
    local config_path="$tmpdir/menu-add-folder-tab-duplicate.json"
    local log_path="$tmpdir/menu-add-folder-tab-duplicate.log"
    local existing_folder="$tmpdir/add-duplicate-existing"
    local other_folder="$tmpdir/add-duplicate-other"
    require_debug_app_for_picker_override
    mkdir -p "$existing_folder" "$other_folder"
    write_duplicate_folder_focus_config "$config_path" "$existing_folder" "$other_folder"
    export J3LAUNCHER_TEST_PICK_FOLDER="$existing_folder"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    activate_file_menu_item 0
    sleep 0.3
    assert_json_value "$config_path" '.FolderTabs | length' "3"
    press_key_until_titles "$config_path" ctrl+shift+Left "ExistingFolder,Manual,OtherFolder"

    stop_app
    echo "file menu add folder tab duplicate focus smoke: ok"
}

run_file_menu_add_tab_smoke() {
    local config_path="$tmpdir/menu-add-tab.json"
    local log_path="$tmpdir/menu-add-tab.log"
    local attempt
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    for attempt in 1 2 3 4 5 6; do
        activate_file_menu_item 1
        local deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(json_value "$config_path" '.FolderTabs | length')" == "3" ]]; then
                assert_json_value "$config_path" '.FolderTabs[-1].title' "Tab 3"
                stop_app
                echo "file menu add tab smoke: ok"
                return
            fi
            sleep 0.1
        done
        focus_app_window
        xdotool key Escape || true
        sleep 0.2
    done
    wait_for_json_value "$config_path" '.FolderTabs | length' "3"
    assert_json_value "$config_path" '.FolderTabs[-1].title' "Tab 3"
    stop_app
    echo "file menu add tab smoke: ok"
}

run_file_menu_set_current_tab_folder_smoke() {
    local config_path="$tmpdir/menu-set-folder.json"
    local log_path="$tmpdir/menu-set-folder.log"
    local old_folder="$tmpdir/set-folder-old"
    local new_folder="$tmpdir/set-folder-new"
    require_debug_app_for_picker_override
    mkdir -p "$old_folder" "$new_folder"
    printf x >"$new_folder/new.txt"
    write_refresh_smoke_config "$config_path" "$old_folder"
    export J3LAUNCHER_TEST_PICK_FOLDER="$new_folder"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    local attempt
    for attempt in 1 2 3; do
        activate_file_menu_item 2
        local deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(json_value "$config_path" '.FolderTabs[0].folder_path')" == "$new_folder" ]]; then
                assert_json_value "$config_path" '.FolderTabs[0].buttons | map(.source_name) | join(",")' "new.txt"
                stop_app
                echo "file menu set current tab folder smoke: ok"
                return
            fi
            sleep 0.1
        done
        focus_app_window
        xdotool key Escape || true
        sleep 0.2
    done

    wait_for_json_value "$config_path" '.FolderTabs[0].folder_path' "$new_folder"
    assert_json_value "$config_path" '.FolderTabs[0].buttons | map(.source_name) | join(",")' "new.txt"
    stop_app
    echo "file menu set current tab folder smoke: ok"
}

run_file_menu_set_current_tab_folder_cancel_smoke() {
    local config_path="$tmpdir/menu-set-folder-cancel.json"
    local log_path="$tmpdir/menu-set-folder-cancel.log"
    local old_folder="$tmpdir/set-folder-cancel-old"
    require_debug_app_for_picker_override
    mkdir -p "$old_folder"
    write_refresh_smoke_config "$config_path" "$old_folder"
    export J3LAUNCHER_TEST_PICK_FOLDER="__CANCEL__"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    activate_file_menu_item 2
    sleep 0.3

    assert_json_value "$config_path" '.FolderTabs | length' "1"
    assert_json_value "$config_path" '.FolderTabs[0].folder_path' "$old_folder"
    assert_json_value "$config_path" '.FolderTabs[0].buttons | length' "0"
    stop_app
    echo "file menu set current tab folder cancel smoke: ok"
}

run_file_menu_set_current_tab_folder_error_smoke() {
    local config_path="$tmpdir/menu-set-folder-error.json"
    local log_path="$tmpdir/menu-set-folder-error.log"
    local old_folder="$tmpdir/set-folder-error-old"
    require_debug_app_for_picker_override
    mkdir -p "$old_folder"
    write_refresh_smoke_config "$config_path" "$old_folder"
    export J3LAUNCHER_TEST_PICK_FOLDER_ERROR="no local path"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER_ERROR

    local message_window
    message_window="$(open_file_menu_message "$log_path" "Set Current Tab Folder error" 2 "Set Tab Folder")"
    xdotool windowactivate --sync "$message_window"
    xdotool key Return
    sleep 0.2

    assert_json_value "$config_path" '.FolderTabs | length' "1"
    assert_json_value "$config_path" '.FolderTabs[0].folder_path' "$old_folder"
    assert_json_value "$config_path" '.FolderTabs[0].buttons | length' "0"
    stop_app
    echo "file menu set current tab folder error smoke: ok"
}

run_file_menu_set_current_tab_folder_duplicate_focus_smoke() {
    local config_path="$tmpdir/menu-set-folder-duplicate.json"
    local log_path="$tmpdir/menu-set-folder-duplicate.log"
    local current_folder="$tmpdir/set-duplicate-current"
    local existing_folder="$tmpdir/set-duplicate-existing"
    require_debug_app_for_picker_override
    mkdir -p "$current_folder" "$existing_folder"
    write_set_folder_duplicate_focus_config "$config_path" "$current_folder" "$existing_folder"
    export J3LAUNCHER_TEST_PICK_FOLDER="$existing_folder"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    activate_file_menu_item 2
    sleep 0.3
    assert_json_value "$config_path" '.FolderTabs | length' "2"
    assert_json_value "$config_path" '.FolderTabs[0].folder_path' "$current_folder"
    press_key_until_titles "$config_path" ctrl+shift+Left "ExistingFolder,CurrentFolder"

    stop_app
    echo "file menu set current tab folder duplicate focus smoke: ok"
}

run_file_menu_tab_layout_smoke() {
    local config_path="$tmpdir/menu-layout.json"
    local log_path="$tmpdir/menu-layout.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local layout_window
    layout_window="$(open_file_menu_dialog "$log_path" "for tab layout smoke" 2 "Tab Layout")"
    xdotool windowactivate --sync "$layout_window"
    xdotool key ctrl+a
    xdotool type --delay 1 2
    xdotool key Tab
    xdotool key ctrl+a
    xdotool type --delay 1 3
    xdotool key Return

    wait_for_json_value "$config_path" '"\(.FolderTabs[0].rows)x\(.FolderTabs[0].cols):\(.FolderTabs[0].buttons | length)"' "2x3:6"
    stop_app
    echo "file menu tab layout smoke: ok"
}

run_file_menu_rename_smoke() {
    local config_path="$tmpdir/menu-rename.json"
    local log_path="$tmpdir/menu-rename.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    local rename_window
    rename_window="$(open_rename_tab_dialog "$log_path" "for rename smoke")"
    replace_text_input_dialog_value "$rename_window" Renamed

    wait_for_json_value "$config_path" '.FolderTabs[0].title' "Renamed"
    stop_app
    echo "file menu rename smoke: ok"
}

run_modal_blocks_main_accelerator_smoke() {
    local config_path="$tmpdir/modal-blocks-accelerator.json"
    local log_path="$tmpdir/modal-blocks-accelerator.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local rename_window
    rename_window="$(open_rename_tab_dialog "$log_path" "for modal accelerator smoke")"
    xdotool windowactivate --sync "$rename_window"
    press_key_combo ctrl+Next
    replace_text_input_dialog_value "$rename_window" ModalSafe

    wait_for_json_value "$config_path" '.FolderTabs[0].title' "ModalSafe"
    assert_json_value "$config_path" '.FolderTabs[1].title' "Two"
    stop_app
    echo "modal blocks main accelerator smoke: ok"
}

run_select_prev_next_smoke() {
    local config_path="$tmpdir/select-prev-next.json"
    local log_path="$tmpdir/select-prev-next.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local attempt
    for attempt in 1 2 3; do
        focus_app_window
        press_key_combo ctrl+Next
        press_key_combo ctrl+shift+Left
        if [[ "$(tab_titles "$config_path")" == "Two,One" ]]; then
            break
        fi
    done
    wait_for_titles "$config_path" "Two,One"

    for attempt in 1 2 3; do
        focus_app_window
        press_key_combo ctrl+Next
        press_key_combo ctrl+Prior
        press_key_combo ctrl+shift+Right
        if [[ "$(tab_titles "$config_path")" == "One,Two" ]]; then
            break
        fi
    done
    wait_for_titles "$config_path" "One,Two"

    stop_app
    echo "select prev/next accelerator smoke: ok"
}

run_sort_current_tab_smoke() {
    local config_path="$tmpdir/sort-current-tab.json"
    local log_path="$tmpdir/sort-current-tab.log"
    write_sort_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    focus_app_window
    xdotool key F5
    wait_for_json_value "$config_path" '.FolderTabs[0].buttons | map(.name) | join(",")' "Alpha,Zulu"
    stop_app
    echo "sort current tab accelerator smoke: ok"
}

run_file_menu_move_tab_smoke() {
    local config_path="$tmpdir/menu-move-tab.json"
    local log_path="$tmpdir/menu-move-tab.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    activate_file_menu_item 5
    wait_for_titles "$config_path" "Two,One"
    activate_file_menu_item 5
    wait_for_titles "$config_path" "One,Two"

    stop_app
    echo "file menu move tab smoke: ok"
}

run_file_menu_select_tab_smoke() {
    local config_path="$tmpdir/menu-select-tab.json"
    local log_path="$tmpdir/menu-select-tab.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    activate_file_menu_item 6
    activate_file_menu_item 5
    wait_for_titles "$config_path" "Two,One"

    stop_app

    config_path="$tmpdir/menu-select-tab-prev.json"
    log_path="$tmpdir/menu-select-tab-prev.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    press_key_combo ctrl+Next
    activate_file_menu_item 6
    activate_file_menu_item 5
    wait_for_titles "$config_path" "Two,One"

    stop_app
    echo "file menu select tab smoke: ok"
}

run_file_menu_sort_current_tab_smoke() {
    local config_path="$tmpdir/menu-sort-current-tab.json"
    local log_path="$tmpdir/menu-sort-current-tab.log"
    write_sort_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    activate_file_menu_item 6
    wait_for_json_value "$config_path" '.FolderTabs[0].buttons | map(.name) | join(",")' "Alpha,Zulu"

    stop_app
    echo "file menu sort current tab smoke: ok"
}

run_refresh_current_tab_smoke() {
    local config_path="$tmpdir/refresh-current-tab.json"
    local log_path="$tmpdir/refresh-current-tab.log"
    local folder_path="$tmpdir/refresh-folder"
    mkdir -p "$folder_path"
    printf x >"$folder_path/a.txt"
    printf x >"$folder_path/b.txt"
    write_refresh_smoke_config "$config_path" "$folder_path"
    start_app "$config_path" "$log_path"
    activate_file_menu_item 7
    wait_for_json_value "$config_path" '.FolderTabs[0].buttons | map(.source_name) | sort | join(",")' "a.txt,b.txt"
    stop_app
    echo "refresh current tab smoke: ok"
}

run_active_scan_exit_smoke() {
    local config_path="$tmpdir/active-scan-exit.json"
    local log_path="$tmpdir/active-scan-exit.log"
    local marker_path="$tmpdir/active-scan-started"
    local folder_path="$tmpdir/active-scan-folder"
    require_debug_app_for_picker_override
    mkdir -p "$folder_path"
    printf x >"$folder_path/a.txt"
    write_refresh_smoke_config "$config_path" "$folder_path"
    export J3LAUNCHER_TEST_SCAN_DELAY_MS=15000
    export J3LAUNCHER_TEST_SCAN_DELAY_MARKER="$marker_path"
    start_app "$config_path" "$log_path"

    activate_file_menu_item 7
    wait_for_file_content "$marker_path" "started"
    activate_file_menu_item 1
    wait_for_app_exit
    unset J3LAUNCHER_TEST_SCAN_DELAY_MS J3LAUNCHER_TEST_SCAN_DELAY_MARKER

    echo "active scan exit smoke: ok"
}

run_active_scan_menu_drag_guard_smoke() {
    local config_path="$tmpdir/active-scan-menu-drag-guard.json"
    local log_path="$tmpdir/active-scan-menu-drag-guard.log"
    local marker_path="$tmpdir/active-scan-guard-started"
    local folder_path="$tmpdir/active-scan-guard-folder"
    require_debug_app_for_picker_override
    mkdir -p "$folder_path"
    printf x >"$folder_path/first.txt"
    printf x >"$folder_path/second.txt"
    write_active_scan_guard_config "$config_path" "$folder_path"
    export J3LAUNCHER_TEST_PICK_FOLDER="$folder_path"
    export J3LAUNCHER_TEST_SCAN_DELAY_MS=15000
    export J3LAUNCHER_TEST_SCAN_DELAY_MARKER="$marker_path"
    start_app "$config_path" "$log_path"
    unset J3LAUNCHER_TEST_PICK_FOLDER

    activate_file_menu_item 2
    wait_for_file_content "$marker_path" "started"

    press_key_combo ctrl+shift+Right
    sleep 0.3
    assert_titles "$config_path" "Scan,Manual"

    focus_app_window
    clear_modifiers
    xdotool mousemove --window "$window_id" 70 96
    sleep 0.1
    xdotool mousedown 1
    sleep 0.5
    xdotool mousemove --window "$window_id" 105 96
    sleep 0.2
    xdotool mousemove --window "$window_id" 150 96
    sleep 0.2
    xdotool mousemove --window "$window_id" 220 96
    sleep 0.5
    xdotool mouseup 1
    sleep 0.3
    assert_json_value "$config_path" '.FolderTabs[0].buttons | map(.name) | join(",")' "First,Second"

    activate_file_menu_item 0
    wait_for_json_value "$config_path" '.Window.DarkTheme // false' "true"

    activate_file_menu_item 1
    wait_for_app_exit
    unset J3LAUNCHER_TEST_SCAN_DELAY_MS J3LAUNCHER_TEST_SCAN_DELAY_MARKER

    echo "active scan menu/dark-theme/drag guard smoke: ok"
}

run_reset_current_tab_smoke() {
    local config_path="$tmpdir/reset-current-tab.json"
    local log_path="$tmpdir/reset-current-tab.log"
    local folder_path="$tmpdir/reset-folder"
    mkdir -p "$folder_path"
    printf x >"$folder_path/a.txt"
    printf x >"$folder_path/b.txt"
    write_reset_smoke_config "$config_path" "$folder_path"
    start_app "$config_path" "$log_path"

    local reset_window
    reset_window="$(open_file_menu_dialog "$log_path" "for reset current tab smoke" 8 "Reset Tab")"
    xdotool windowactivate --sync "$reset_window"
    xdotool key alt+y
    wait_for_json_value "$config_path" '.FolderTabs[0].buttons | map(.name) | join(",")' "a.txt,b.txt"
    assert_json_value "$config_path" '.FolderTabs[0].hidden_item_ids | join(",")' ""
    assert_json_value "$config_path" '.FolderTabs[0].buttons | map(.params) | join(",")' ","
    stop_app
    echo "reset current tab smoke: ok"
}

run_reset_default_no_smoke() {
    local config_path="$tmpdir/reset-default-no.json"
    local log_path="$tmpdir/reset-default-no.log"
    local folder_path="$tmpdir/reset-default-no-folder"
    mkdir -p "$folder_path"
    printf x >"$folder_path/a.txt"
    printf x >"$folder_path/b.txt"
    write_reset_smoke_config "$config_path" "$folder_path"
    start_app "$config_path" "$log_path"

    local reset_window
    reset_window="$(open_file_menu_dialog "$log_path" "for reset default-No smoke" 8 "Reset Tab")"
    xdotool windowactivate --sync "$reset_window"
    xdotool key Return
    sleep 0.3

    assert_json_value "$config_path" '.FolderTabs[0].buttons | map(.name) | join(",")' "CustomB,CustomA"
    assert_json_value "$config_path" '.FolderTabs[0].hidden_item_ids | join(",")' "missing-hidden"
    stop_app
    echo "reset default-No smoke: ok"
}

run_manage_hidden_items_smoke() {
    local config_path="$tmpdir/manage-hidden.json"
    local log_path="$tmpdir/manage-hidden.log"
    local folder_path="$tmpdir/hidden-folder"
    mkdir -p "$folder_path"
    printf x >"$folder_path/visible.txt"
    printf x >"$folder_path/hidden.txt"
    write_hidden_smoke_config "$config_path" "$folder_path"
    start_app "$config_path" "$log_path"

    local hidden_window attempt deadline
    for attempt in 1 2 3; do
        hidden_window="$(
            open_file_menu_dialog_any "$log_path" "for manage hidden items smoke" "Manage Hidden Items" 9 10
        )"
        xdotool windowactivate --sync "$hidden_window"
        sleep 0.3
        xdotool mousemove --window "$hidden_window" 120 96 click 1
        sleep 0.2
        xdotool key Tab
        sleep 0.1
        xdotool key space
        deadline=$((SECONDS + 2))
        while ((SECONDS < deadline)); do
            if [[ "$(json_value "$config_path" '.FolderTabs[0].hidden_item_ids | join(",")')" == "" ]]; then
                stop_app
                echo "manage hidden items smoke: ok"
                return
            fi
            sleep 0.1
        done
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done

    assert_json_value "$config_path" '.FolderTabs[0].hidden_item_ids | join(",")' ""
    stop_app
    echo "manage hidden items smoke: ok"
}

run_file_menu_delete_default_smoke() {
    local config_path="$tmpdir/menu-delete-default.json"
    local log_path="$tmpdir/menu-delete-default.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"

    local delete_window
    delete_window="$(open_file_menu_dialog "$log_path" "for delete default smoke" 4 "Delete Tab")"
    xdotool windowactivate --sync "$delete_window"
    xdotool key Return
    sleep 0.3

    assert_titles "$config_path" "One,Two"
    stop_app
    echo "file menu delete default smoke: ok"
}

run_file_menu_delete_current_tab_smoke() {
    local config_path="$tmpdir/menu-delete-current-tab.json"
    local log_path="$tmpdir/menu-delete-current-tab.log"
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    focus_app_window
    press_key_combo ctrl+Prior

    local delete_window
    delete_window="$(open_file_menu_dialog "$log_path" "for delete smoke" 4 "Delete Tab")"
    xdotool windowactivate --sync "$delete_window"
    xdotool key alt+y

    wait_for_titles "$config_path" "Two"
    stop_app
    echo "file menu delete current tab smoke: ok"
}

run_file_menu_dark_theme_smoke() {
    local config_path="$tmpdir/menu-dark-theme.json"
    local log_path="$tmpdir/menu-dark-theme.log"
    local attempt
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    for attempt in 1 2 3 4 5; do
        activate_file_menu_item 7
        if [[ "$(json_value "$config_path" '.Window.DarkTheme // false')" == "true" ]]; then
            break
        fi
        focus_app_window >/dev/null 2>&1
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done
    wait_for_json_value "$config_path" '.Window.DarkTheme // false' "true"
    stop_app
    echo "file menu dark theme smoke: ok"
}

run_file_menu_exit_smoke() {
    local config_path="$tmpdir/menu-exit.json"
    local log_path="$tmpdir/menu-exit.log"
    local attempt
    write_smoke_config "$config_path"
    start_app "$config_path" "$log_path"
    for attempt in 1 2 3; do
        activate_file_menu_item 8
        if app_exited_within 3; then
            echo "file menu exit smoke: ok"
            return
        fi
        focus_app_window
        xdotool key Escape >/dev/null 2>&1 || true
        sleep 0.2
    done
    wait_for_app_exit
    echo "file menu exit smoke: ok"
}

run_disabled_move_left_smoke
run_move_right_smoke
run_resize_focus_repeated_accelerator_smoke
run_select_prev_next_smoke
run_sort_current_tab_smoke
run_file_menu_move_tab_smoke
run_file_menu_select_tab_smoke
run_file_menu_sort_current_tab_smoke
run_refresh_current_tab_smoke
if run_debug_only_smokes; then
    run_active_scan_exit_smoke
    run_active_scan_menu_drag_guard_smoke
else
    skip_debug_only_smoke "active scan exit smoke"
    skip_debug_only_smoke "active scan menu/dark-theme/drag guard smoke"
fi
run_reset_default_no_smoke
run_reset_current_tab_smoke
run_manage_hidden_items_smoke
run_keyboard_context_menu_smoke
run_context_menu_repeated_dismiss_smoke
run_pointer_context_menu_smoke
run_button_drag_drop_smoke
run_context_menu_edit_save_smoke
run_context_menu_open_guard_smoke
run_context_menu_open_folder_smoke
run_context_menu_hide_smoke
run_button_launch_smoke
run_button_admin_fake_pkexec_smoke
run_button_admin_fake_pkexec_cancel_smoke
run_button_associated_open_smoke
run_button_protocol_open_smoke
run_button_copy_paste_smoke
if run_debug_only_smokes; then
    run_file_menu_add_folder_tab_smoke
    run_file_menu_add_folder_tab_cancel_smoke
    run_file_menu_add_folder_tab_error_smoke
    run_file_menu_add_folder_tab_duplicate_focus_smoke
else
    skip_debug_only_smoke "file menu add folder tab smoke"
    skip_debug_only_smoke "file menu add folder tab cancel smoke"
    skip_debug_only_smoke "file menu add folder tab error smoke"
    skip_debug_only_smoke "file menu add folder tab duplicate focus smoke"
fi
run_file_menu_add_tab_smoke
if run_debug_only_smokes; then
    run_file_menu_set_current_tab_folder_smoke
    run_file_menu_set_current_tab_folder_cancel_smoke
    run_file_menu_set_current_tab_folder_error_smoke
    run_file_menu_set_current_tab_folder_duplicate_focus_smoke
else
    skip_debug_only_smoke "file menu set current tab folder smoke"
    skip_debug_only_smoke "file menu set current tab folder cancel smoke"
    skip_debug_only_smoke "file menu set current tab folder error smoke"
    skip_debug_only_smoke "file menu set current tab folder duplicate focus smoke"
fi
run_file_menu_tab_layout_smoke
run_file_menu_rename_smoke
run_modal_blocks_main_accelerator_smoke
run_file_menu_delete_default_smoke
run_file_menu_delete_current_tab_smoke
run_file_menu_dark_theme_smoke
run_file_menu_exit_smoke
assert_no_unexpected_app_log_errors
