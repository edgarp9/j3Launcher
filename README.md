# j3Launcher

j3Launcher is a desktop launcher for organizing frequently used applications, folders, files, and URLs into tabs and buttons.
It uses a Win32 UI on Windows and a GTK4 UI on Linux.

## Development Notes

- This project was built with AI assistance using an in-house tool.
- Test coverage is currently limited. Manual verification is recommended before using or distributing a build.
- Linux support was developed on Debian.

## Features

- Tab-based launcher UI
- Button lists generated from folder scans
- Manual tabs and user-defined buttons
- Button launch, run as administrator, open folder, and copy path plus parameters
- Button editing, hidden item management, sorting, refresh, and reset
- Dark theme support
- Compatibility with the existing Windows JSON configuration format
- Native Win32 UI on Windows and native GTK4 UI on Linux
- Small Windows footprint: about a 1 MB executable and under 3 MB memory usage in typical Windows use
- Linux memory usage is significantly higher because the Linux UI uses GTK

## Project Layout

```text
.
|-- LICENSE
|-- README.md
`-- src/
    |-- Cargo.toml
    |-- README.md
    |-- docs/
    |-- src/
    `-- tests/
```

The Rust crate and application source code live in the `src/` directory.

## Requirements

- Windows 10/11 or Linux desktop
- Rust 1.88 or later
- Windows: MSVC-based Rust toolchain
- Linux: GTK4 development packages and `pkg-config`

GTK4 development package names may differ by Linux distribution.

## Build

```powershell
cd src
cargo build --release
```

On Windows, the release executable is created at:

```text
src/target/release/j3launcher.exe
```

On Linux, the same command creates `src/target/release/j3launcher`.

## Run

Development run:

```powershell
cd src
cargo run
```

Run the release binary:

```powershell
cd src
.\target\release\j3launcher.exe
```

You can also pass a specific configuration file:

```powershell
.\target\release\j3launcher.exe 111.json
.\target\release\j3launcher.exe 222.json
```

## Verification

Run the following commands before preparing a release:

```powershell
cd src
cargo fmt --check
cargo test
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

If you need to check Windows-target compilation from Linux and a resource compiler is not available, you can skip resource embedding:

```bash
cd src
J3LAUNCHER_SKIP_WINDOWS_RESOURCES=1 cargo check --target x86_64-pc-windows-msvc
```

## Configuration

The default configuration file is `j3Launcher.json` in the executable directory.
If a JSON file name is passed as the first command-line argument, that file is used as the configuration file.

- If the selected configuration file does not exist, `j3Launcher_win.json` in the same directory is used as the initial seed.
- If no seed file exists, the app starts with the default configuration.
- Window size and position are stored in the JSON configuration.
- Windows-style relative paths are handled on Linux where possible for compatibility.
- Saves use a lock file and a temporary file before replacing the configuration file.

## Linux Desktop Registration

To register the app menu entry, icon, and taskbar mapping on Linux, run:

```bash
cd src
./target/release/j3launcher --install
```

To remove the registration:

```bash
cd src
./target/release/j3launcher --uninstall
```

`--install` installs `icon.svg` first and falls back to `icon.png` if the SVG is not available.

## Documentation

- [Rust crate README](src/README.md)
- [Domain notes](src/docs/domain.md)
- [Linux parity report](src/docs/linux-parity-report.md)
- [Release readiness notes](src/docs/release-readiness.md)

## License

This project is licensed under the [GPL-3.0 License](LICENSE).

## Third-Party Notices

The launcher icon uses [Google Fonts Icons](https://fonts.google.com/icons), which are available under the Apache License Version 2.0.
Thanks to Google Fonts and the Material Symbols team for providing these icon resources.
