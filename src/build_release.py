#!/usr/bin/env python3
from __future__ import annotations

import fnmatch
import json
import os
import platform
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


class BuildReleaseError(Exception):
    pass


NOTICE_FILES = [
    "LICENSE",
    "THIRD_PARTY_NOTICES.txt",
    "about.txt",
]

SOURCE_ARCHIVE_EXCLUDED_DIRS = {
    ".git",
    ".idea",
    ".my",
    ".vscode",
    "coverage",
    "criterion",
    "dist",
    "target",
}

SOURCE_ARCHIVE_EXCLUDED_NAMES = {
    ".DS_Store",
    "Desktop.ini",
    "Thumbs.db",
    "cargo-tarpaulin-report.xml",
    "flamegraph.svg",
    "tarpaulin-report.html",
}

SOURCE_ARCHIVE_EXCLUDED_PATTERNS = [
    "*.bak",
    "*.ilk",
    "*.log",
    "*.pdb",
    "*.profdata",
    "*.profraw",
    "*.rlib",
    "*.rmeta",
    "*.swo",
    "*.swp",
    "*.tmp",
    "*~",
]


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    open_release_folder = True
    for arg in argv:
        if arg == "--no-open":
            open_release_folder = False
            continue
        print(f"error: unknown argument: {arg}", file=sys.stderr)
        return 2

    root_dir = Path(__file__).resolve().parent
    cargo = shutil.which("cargo")
    if cargo is None:
        print("error: cargo was not found in PATH.", file=sys.stderr)
        return 1

    try:
        metadata = read_cargo_metadata(cargo, root_dir)
        package = root_package(metadata, root_dir / "Cargo.toml")
        release_dir = target_release_dir(metadata)
        binary_names = package_binary_names(package)

        print(f"Project root: {root_dir}")
        print("Running release build...")
        run_command([cargo, "build", "--release"], root_dir)

        if not release_dir.is_dir():
            raise BuildReleaseError(f"release directory was not created: {release_dir}")

        copy_notice_files(root_dir, release_dir)
        print_binaries(release_dir, binary_names)
        create_source_archive(root_dir, release_dir, package)
        create_binary_archive(release_dir, binary_names, package)
        if open_release_folder:
            open_folder(release_dir)
        print(f"Release folder: {release_dir}")
        return 0
    except BuildReleaseError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("error: build interrupted.", file=sys.stderr)
        return 130


def read_cargo_metadata(cargo: str, root_dir: Path) -> dict:
    command = [cargo, "metadata", "--format-version", "1", "--no-deps"]
    result = subprocess.run(
        command,
        cwd=root_dir,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        stderr = result.stderr.strip()
        detail = f": {stderr}" if stderr else ""
        raise BuildReleaseError(f"cargo metadata failed{detail}")

    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise BuildReleaseError(f"failed to parse cargo metadata: {error}") from error


def target_release_dir(metadata: dict) -> Path:
    target_dir = metadata.get("target_directory")
    if not isinstance(target_dir, str) or not target_dir:
        raise BuildReleaseError("cargo metadata did not include target_directory.")
    return Path(target_dir).resolve() / "release"


def root_package(metadata: dict, root_manifest: Path) -> dict:
    root_manifest = root_manifest.resolve()
    packages = metadata.get("packages", [])
    if not isinstance(packages, list):
        raise BuildReleaseError("cargo metadata packages field was not a list.")

    for package in packages:
        manifest_path = package.get("manifest_path")
        if not isinstance(manifest_path, str):
            continue
        if Path(manifest_path).resolve() != root_manifest:
            continue
        return package

    raise BuildReleaseError(f"root package was not found in cargo metadata: {root_manifest}")


def package_binary_names(package: dict) -> list[str]:
    targets = package.get("targets", [])
    if not isinstance(targets, list):
        return []

    names = []
    for target in targets:
        kinds = target.get("kind", [])
        name = target.get("name")
        if isinstance(kinds, list) and "bin" in kinds and isinstance(name, str):
            names.append(name)
    return names


def package_archive_stem(package: dict, suffix: str) -> str:
    name = required_package_string(package, "name")
    version = required_package_string(package, "version")
    return f"{name}-{version}-{suffix}"


def required_package_string(package: dict, key: str) -> str:
    value = package.get(key)
    if not isinstance(value, str) or not value:
        raise BuildReleaseError(f"cargo metadata package did not include {key}.")
    return value


def run_command(command: list[str], cwd: Path) -> None:
    print(format_command(command))
    result = subprocess.run(command, cwd=cwd, check=False)
    if result.returncode != 0:
        raise BuildReleaseError(f"command failed with exit code {result.returncode}.")


def copy_notice_files(root_dir: Path, release_dir: Path) -> None:
    for file_name in NOTICE_FILES:
        notice_file = root_dir / file_name
        if not notice_file.is_file():
            raise BuildReleaseError(f"notice file was not found: {notice_file}")

        destination = release_dir / notice_file.name
        shutil.copy2(notice_file, destination)
        print(f"Copied notice file: {destination}")


def create_source_archive(root_dir: Path, release_dir: Path, package: dict) -> Path:
    archive_path = release_dir / f"{package_archive_stem(package, 'source')}.zip"
    archive_root = package_archive_stem(package, "source")
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for source_file in iter_source_files(root_dir):
            relative_path = source_file.relative_to(root_dir)
            archive.write(source_file, (Path(archive_root) / relative_path).as_posix())

    print(f"Created source archive: {archive_path}")
    return archive_path


def iter_source_files(root_dir: Path):
    for current_dir, dir_names, file_names in os.walk(root_dir):
        dir_names[:] = [
            name
            for name in sorted(dir_names)
            if name not in SOURCE_ARCHIVE_EXCLUDED_DIRS
        ]
        current_path = Path(current_dir)
        for file_name in sorted(file_names):
            if should_exclude_source_file(file_name):
                continue
            yield current_path / file_name


def should_exclude_source_file(file_name: str) -> bool:
    if file_name in SOURCE_ARCHIVE_EXCLUDED_NAMES:
        return True
    return any(
        fnmatch.fnmatch(file_name, pattern)
        for pattern in SOURCE_ARCHIVE_EXCLUDED_PATTERNS
    )


def create_binary_archive(
    release_dir: Path, binary_names: list[str], package: dict
) -> Path:
    if not binary_names:
        raise BuildReleaseError("no binary targets were reported by cargo metadata.")

    archive_path = release_dir / f"{package_archive_stem(package, binary_archive_suffix())}.zip"
    archive_root = package_archive_stem(package, binary_archive_suffix())
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for binary_path in release_binary_paths(release_dir, binary_names):
            archive.write(binary_path, (Path(archive_root) / binary_path.name).as_posix())
        for file_name in NOTICE_FILES:
            notice_path = release_dir / file_name
            if not notice_path.is_file():
                raise BuildReleaseError(f"release notice file was not found: {notice_path}")
            archive.write(notice_path, (Path(archive_root) / notice_path.name).as_posix())

    print(f"Created binary archive: {archive_path}")
    return archive_path


def release_binary_paths(release_dir: Path, binary_names: list[str]) -> list[Path]:
    suffix = ".exe" if platform.system() == "Windows" else ""
    binary_paths = []
    for name in binary_names:
        binary_path = release_dir / f"{name}{suffix}"
        if not binary_path.is_file():
            raise BuildReleaseError(f"built binary was not found: {binary_path}")
        binary_paths.append(binary_path)
    return binary_paths


def binary_archive_suffix() -> str:
    system = platform.system().lower() or "unknown-os"
    machine = platform.machine().lower() or "unknown-arch"
    if machine == "amd64":
        machine = "x86_64"
    return f"{system}-{machine}"


def format_command(command: list[str]) -> str:
    return "$ " + " ".join(quote_arg(arg) for arg in command)


def quote_arg(arg: str) -> str:
    if not arg or any(char.isspace() for char in arg):
        return f'"{arg}"'
    return arg


def print_binaries(release_dir: Path, binary_names: list[str]) -> None:
    if not binary_names:
        print("No binary targets were reported by cargo metadata.")
        return

    suffix = ".exe" if platform.system() == "Windows" else ""
    print("Built binaries:")
    for name in binary_names:
        binary_path = release_dir / f"{name}{suffix}"
        status = "found" if binary_path.exists() else "missing"
        print(f"  - {binary_path} ({status})")


def open_folder(folder: Path) -> None:
    system = platform.system()
    try:
        if system == "Windows":
            os.startfile(folder)  # type: ignore[attr-defined]
            return
        if system == "Darwin":
            subprocess.Popen(["open", str(folder)])
            return

        opener = first_available(["xdg-open", "gio", "gnome-open", "kde-open"])
        if opener is None:
            print(f"Could not find a file manager opener. Open manually: {folder}")
            return

        command = [opener, str(folder)]
        if Path(opener).name == "gio":
            command = [opener, "open", str(folder)]
        subprocess.Popen(
            command,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except OSError as error:
        print(f"Could not open release folder automatically: {error}")


def first_available(commands: list[str]) -> str | None:
    for command in commands:
        path = shutil.which(command)
        if path is not None:
            return path
    return None


if __name__ == "__main__":
    raise SystemExit(main())
