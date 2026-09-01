#!/usr/bin/env python3
"""Assemble, sign, and verify the closed Garive process XPC bundle."""

from __future__ import annotations

import argparse
import os
import plistlib
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

SERVICE_EXECUTABLE = "GariveProcessIsolationService"
SERVICE_BUNDLE_IDENTIFIER = "com.garive.desktop.process-isolation-service"
BACKEND_REQUIREMENT_KEY = "GariveBackendCodeSigningRequirement"
VERSION_PATTERN = re.compile(r"[0-9]+(?:\.[0-9]+)*")


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--executable", required=True, type=Path)
    value.add_argument("--output", required=True, type=Path)
    value.add_argument("--bundle-identifier", required=True)
    value.add_argument("--bundle-version", required=True)
    value.add_argument("--short-version", required=True)
    value.add_argument("--backend-requirement", required=True)
    value.add_argument("--signing-identity", required=True)
    value.add_argument("--codesign-tool", required=True, type=Path)
    return value


def require_inputs(args: argparse.Namespace) -> tuple[Path, Path, Path]:
    executable = args.executable.resolve(strict=True)
    output = args.output.resolve(strict=False)
    codesign_tool = args.codesign_tool.resolve(strict=True)
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise ValueError("executable must be an executable regular file")
    if not codesign_tool.is_file() or not os.access(codesign_tool, os.X_OK):
        raise ValueError("codesign tool must be an executable regular file")
    if output.suffix != ".xpc" or not output.parent.is_dir() or output.exists():
        raise ValueError("output must be a new .xpc path under an existing directory")
    if args.bundle_identifier != SERVICE_BUNDLE_IDENTIFIER:
        raise ValueError("bundle identifier does not match the service contract")
    if not VERSION_PATTERN.fullmatch(args.bundle_version):
        raise ValueError("bundle version is invalid")
    if not VERSION_PATTERN.fullmatch(args.short_version):
        raise ValueError("short version is invalid")
    requirement = args.backend_requirement
    if not requirement.strip() or "\0" in requirement or len(requirement.encode()) > 4_096:
        raise ValueError("backend requirement is invalid")
    if not args.signing_identity.strip() or "\0" in args.signing_identity:
        raise ValueError("signing identity is invalid")
    return executable, output, codesign_tool


def write_unsigned_bundle(
    root: Path, executable: Path, args: argparse.Namespace
) -> Path:
    bundle = root / "GariveProcessIsolationService.xpc"
    macos = bundle / "Contents" / "MacOS"
    macos.mkdir(parents=True)
    installed_executable = macos / SERVICE_EXECUTABLE
    shutil.copy2(executable, installed_executable)
    installed_executable.chmod(0o755)
    metadata = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleExecutable": SERVICE_EXECUTABLE,
        "CFBundleIdentifier": args.bundle_identifier,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": SERVICE_EXECUTABLE,
        "CFBundlePackageType": "XPC!",
        "CFBundleShortVersionString": args.short_version,
        "CFBundleVersion": args.bundle_version,
        "LSMinimumSystemVersion": "14.0",
        BACKEND_REQUIREMENT_KEY: args.backend_requirement,
        "XPCService": {"ServiceType": "Application"},
    }
    with (bundle / "Contents" / "Info.plist").open("wb") as stream:
        plistlib.dump(metadata, stream, sort_keys=True)
    return bundle


def run_codesign(tool: Path, bundle: Path, identity: str) -> None:
    subprocess.run(
        [str(tool), "--force", "--sign", identity, "--options", "runtime", "--timestamp", str(bundle)],
        check=True,
        capture_output=True,
        text=True,
    )
    subprocess.run(
        [str(tool), "--verify", "--strict", "--verbose=2", str(bundle)],
        check=True,
        capture_output=True,
        text=True,
    )
    displayed = subprocess.run(
        [str(tool), "--display", "--verbose=4", str(bundle)],
        check=True,
        capture_output=True,
        text=True,
    )
    if f"Identifier={SERVICE_BUNDLE_IDENTIFIER}" not in displayed.stderr:
        raise ValueError("signed executable identifier does not match the service contract")


def validate_layout(bundle: Path) -> None:
    expected = {
        Path("Contents/Info.plist"),
        Path(f"Contents/MacOS/{SERVICE_EXECUTABLE}"),
        Path("Contents/_CodeSignature/CodeResources"),
    }
    actual = {value.relative_to(bundle) for value in bundle.rglob("*") if value.is_file()}
    if actual != expected:
        raise ValueError(f"unexpected signed bundle layout: {sorted(map(str, actual))}")


def main() -> int:
    args = parser().parse_args()
    executable, output, codesign_tool = require_inputs(args)
    with tempfile.TemporaryDirectory(prefix="garive-process-xpc-", dir=output.parent) as value:
        bundle = write_unsigned_bundle(Path(value), executable, args)
        run_codesign(codesign_tool, bundle, args.signing_identity)
        validate_layout(bundle)
        bundle.rename(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
