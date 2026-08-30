#!/usr/bin/env python3
"""Fail closed when the checked-in native-mobile evidence drifts."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANUAL = ROOT / "docs/manual/mobile-user-guide.md"
ASSETS = MANUAL.parent / "assets/mobile"
CANDIDATE_EVIDENCE = ASSETS / "candidate-evidence.json"


def png_size(path: Path) -> tuple[int, int]:
    with path.open("rb") as stream:
        header = stream.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise ValueError(f"not a valid PNG: {path.relative_to(ROOT)}")
    return struct.unpack(">II", header[16:24])


def digest_files(paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(path.relative_to(ROOT).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def android_source_digest() -> str:
    paths: list[Path] = []
    for base in (
        ROOT / "mobile/androidApp/app/src/main",
        ROOT / "mobile/shared/src/commonMain",
    ):
        paths.extend(path for path in base.rglob("*") if path.is_file())
    paths.extend((
        ROOT / "mobile/androidApp/app/build.gradle.kts",
        ROOT / "runtime/gateway/cmd/garive-mobile-demo-host/main.go",
    ))
    return digest_files(paths)


def ios_source_digest() -> str:
    paths: list[Path] = []
    for base in (
        ROOT / "mobile/iosApp/Sources/GariveIOS",
        ROOT / "mobile/shared/src/commonMain",
    ):
        paths.extend(path for path in base.rglob("*") if path.is_file())
    paths.extend((
        ROOT / "mobile/iosApp/GariveIOS.xcodeproj/project.pbxproj",
        ROOT / "runtime/gateway/cmd/garive-mobile-demo-host/main.go",
    ))
    return digest_files(paths)


def verify_candidate_evidence() -> None:
    evidence = json.loads(CANDIDATE_EVIDENCE.read_text())
    if evidence.get("schema_version") != 1:
        raise ValueError("unsupported mobile candidate-evidence schema")
    if evidence.get("android_source_digest") != android_source_digest():
        raise ValueError("Android core screenshots must be recaptured after candidate source changes")
    if evidence.get("ios_source_digest") != ios_source_digest():
        raise ValueError("iOS core screenshots must be recaptured after candidate source changes")
    required = {
        "android-03-sessions.png",
        "android-05-new-task.png",
        "android-06-approval.png",
        "android-09-steering.png",
        "android-22-code-result.png",
        "ios-03-sessions.png",
        "ios-05-new-task.png",
        "ios-17-steering.png",
        "ios-18-code-result.png",
    }
    screenshots = evidence.get("screenshots", {})
    if set(screenshots) != required:
        raise ValueError("native core candidate evidence is incomplete")
    for name, expected in screenshots.items():
        path = ASSETS / name
        actual_hash = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual_hash != expected.get("sha256"):
            raise ValueError(f"candidate screenshot digest drift: {name}")
        if list(png_size(path)) != [expected.get("width"), expected.get("height")]:
            raise ValueError(f"candidate screenshot dimensions drift: {name}")


def verify(artifacts: bool) -> None:
    text = MANUAL.read_text()
    references = set(re.findall(r"assets/mobile/[^)\s]+", text))
    files = {f"assets/mobile/{path.name}" for path in ASSETS.glob("*.png")}
    if references != files:
        raise ValueError(
            f"mobile screenshot drift; missing={sorted(references - files)}, "
            f"unreferenced={sorted(files - references)}"
        )
    if len(files) != 40 or "当前手册包含 40 张实际运行截图" not in text:
        raise ValueError(f"manual must contain and declare exactly 40 screenshots, found {len(files)}")

    required = {
        "android-02-work-light.png",
        "android-10-a11y-dark-work.png",
        "android-18-navigation-dark.png",
        "android-21-navigation-light.png",
        "ios-02-work-light.png",
        "ios-08-a11y-dark-work.png",
        "ios-13-navigation-dark.png",
        "ios-14-navigation-light.png",
    }
    if not required.issubset({Path(item).name for item in files}):
        raise ValueError("both native clients require light Work, dark accessible Work, and light/dark Remote evidence")
    for item in sorted(files):
        width, height = png_size(MANUAL.parent / item)
        if min(width, height) < 300:
            raise ValueError(f"undersized evidence {item}: {width}x{height}")
    verify_candidate_evidence()

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
    if "complete 40-screenshot user guide" not in status:
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
