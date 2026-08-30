#!/usr/bin/env python3
"""Fail closed when the checked-in native-mobile evidence drifts."""

from __future__ import annotations

import argparse
import re
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANUAL = ROOT / "docs/manual/mobile-user-guide.md"
ASSETS = MANUAL.parent / "assets/mobile"


def png_size(path: Path) -> tuple[int, int]:
    with path.open("rb") as stream:
        header = stream.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise ValueError(f"not a valid PNG: {path.relative_to(ROOT)}")
    return struct.unpack(">II", header[16:24])


def verify(artifacts: bool) -> None:
    text = MANUAL.read_text()
    references = set(re.findall(r"assets/mobile/[^)\s]+", text))
    files = {f"assets/mobile/{path.name}" for path in ASSETS.glob("*.png")}
    if references != files:
        raise ValueError(
            f"mobile screenshot drift; missing={sorted(references - files)}, "
            f"unreferenced={sorted(files - references)}"
        )
    if len(files) != 37 or "当前手册包含 37 张实际运行截图" not in text:
        raise ValueError(f"manual must contain and declare exactly 37 screenshots, found {len(files)}")

    required = {
        "android-18-navigation-dark.png",
        "android-21-navigation-light.png",
        "ios-13-navigation-dark.png",
        "ios-14-navigation-light.png",
    }
    if not required.issubset({Path(item).name for item in files}):
        raise ValueError("both native clients require light and dark Remote navigation evidence")
    for item in sorted(files):
        width, height = png_size(MANUAL.parent / item)
        if min(width, height) < 300:
            raise ValueError(f"undersized evidence {item}: {width}x{height}")

    for path in (
        ROOT / "mobile/androidApp/app/src/main/java/com/garive/android/MainActivity.kt",
        ROOT / "mobile/iosApp/GariveIOS.xcodeproj/project.pbxproj",
        ROOT / "mobile/iosApp/UITests/GariveIOSUITests.swift",
        ROOT / "spec/design/mobile-remote-work-client.md",
        ROOT / "spec/design/mobile-gateway-v1.md",
    ):
        if not path.is_file():
            raise ValueError(f"missing mobile delivery source: {path.relative_to(ROOT)}")

    status = (ROOT / "spec/STATUS.md").read_text()
    if "complete 37-screenshot user guide" not in status:
        raise ValueError("spec/STATUS.md does not match the checked-in mobile evidence")

    if artifacts:
        for path in (
            ROOT / "mobile/androidApp/app/build/outputs/apk/debug/app-debug.apk",
            ROOT / "mobile/androidApp/app/build/outputs/apk/release/app-release-unsigned.apk",
            ROOT / "mobile/iosApp/build/derived-data/Build/Products/Debug-iphonesimulator/Garive.app",
        ):
            if not path.exists():
                raise ValueError(f"missing built install artifact: {path.relative_to(ROOT)}")

    print(f"mobile evidence verified: {len(files)} PNGs, exact references, native projects and specs")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", action="store_true", help="also require locally built install artifacts")
    verify(parser.parse_args().artifacts)
