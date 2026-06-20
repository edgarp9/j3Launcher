#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path


class BuildReleaseError(Exception):
    pass


def main() -> int:
    root_dir = Path(__file__).resolve().parent
    cargo = shutil.which("cargo")
    if cargo is None:
        print("error: cargo was not found in PATH.", file=sys.stderr)
        return 1

    try:
        metadata = read_cargo_metadata(cargo, root_dir)
        release_dir = target_release_dir(metadata)
        binary_names = root_binary_names(metadata, root_dir / "Cargo.toml")

        print(f"Project root: {root_dir}")
        print("Running release build...")
        run_command([cargo, "build", "--release"], root_dir)

        if not release_dir.is_dir():
            raise BuildReleaseError(f"release directory was not created: {release_dir}")

        print_binaries(release_dir, binary_names)
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


def root_binary_names(metadata: dict, root_manifest: Path) -> list[str]:
    root_manifest = root_manifest.resolve()
    packages = metadata.get("packages", [])
    if not isinstance(packages, list):
        return []

    for package in packages:
        manifest_path = package.get("manifest_path")
        if not isinstance(manifest_path, str):
            continue
        if Path(manifest_path).resolve() != root_manifest:
            continue

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

    return []


def run_command(command: list[str], cwd: Path) -> None:
    print(format_command(command))
    result = subprocess.run(command, cwd=cwd, check=False)
    if result.returncode != 0:
        raise BuildReleaseError(f"command failed with exit code {result.returncode}.")


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
