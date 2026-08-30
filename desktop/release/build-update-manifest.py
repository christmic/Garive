#!/usr/bin/env python3
"""Bind one signed Universal macOS updater archive to a static Tauri manifest."""

from __future__ import annotations

import argparse
import base64
import binascii
import ipaddress
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path
from urllib.parse import urlsplit


SCRIPT = Path(__file__).resolve()
REPO = SCRIPT.parents[2]
TARGET = (REPO / "target").resolve()
PLATFORMS = ("darwin-aarch64", "darwin-x86_64")
MAX_SIGNATURE_BYTES = 16 * 1024
MAX_URL_BYTES = 2_048
MAX_NOTES_BYTES = 16 * 1024
STABLE_SEMVER = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def fail(message: str) -> "NoReturn":
    raise SystemExit(message)


def run(*command: str) -> str:
    result = subprocess.run(
        command, cwd=REPO, text=True, capture_output=True, check=False
    )
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip()
        fail(f"command failed ({' '.join(command)}):\n{detail}")
    return result.stdout


def inside(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def clean_revision() -> tuple[str, str]:
    if run("git", "status", "--porcelain").strip():
        fail("update manifest requires a clean Git worktree")
    revision = run("git", "rev-parse", "HEAD").strip()
    commit_time = run("git", "show", "-s", "--format=%cI", revision).strip()
    return revision, commit_time


def admitted_file(argument: str, label: str) -> Path:
    raw = Path(argument)
    if raw.is_symlink():
        fail(f"{label} must not be a symlink")
    path = (REPO / raw).resolve() if not raw.is_absolute() else raw.resolve()
    if not inside(path, TARGET) or not path.is_file() or path.stat().st_size == 0:
        fail(f"{label} must be a nonempty file inside this checkout's target directory")
    return path


def admitted_archive(argument: str) -> Path:
    archive = admitted_file(argument, "updater archive")
    if not archive.name.endswith(".app.tar.gz"):
        fail("updater archive must end in .app.tar.gz")
    return archive


def admitted_signature(argument: str, archive: Path) -> bytes:
    signature = admitted_file(argument, "updater signature")
    if signature != Path(f"{archive}.sig"):
        fail("updater signature must be the archive's adjacent .sig file")
    encoded = signature.read_bytes()
    if len(encoded) > MAX_SIGNATURE_BYTES:
        fail("updater signature is oversized")
    try:
        text = encoded.decode("utf-8")
    except UnicodeDecodeError:
        fail("updater signature is not UTF-8")
    lines = text.splitlines()
    if len(lines) != 4 or not lines[0].startswith(
        "untrusted comment: signature from minisign secret key"
    ) or not lines[2].startswith("trusted comment: "):
        fail("updater signature is not a Minisign signature document")
    try:
        primary = base64.b64decode(lines[1], validate=True)
        global_signature = base64.b64decode(lines[3], validate=True)
    except (binascii.Error, ValueError):
        fail("updater signature contains invalid base64")
    if len(primary) != 74 or primary[:2] not in (b"Ed", b"ED"):
        fail("updater signature has an invalid primary signature shape")
    if len(global_signature) != 64:
        fail("updater signature has an invalid global signature shape")
    return encoded


def admitted_url(raw: str, archive: Path) -> str:
    if not raw or raw != raw.strip() or len(raw.encode("utf-8")) > MAX_URL_BYTES:
        fail("archive URL is empty, padded, or oversized")
    try:
        parsed = urlsplit(raw)
        port = parsed.port
    except ValueError:
        fail("archive URL is invalid")
    hostname = parsed.hostname
    if parsed.scheme != "https" or not hostname or parsed.fragment or parsed.query:
        fail("archive URL must be public HTTPS without a query or fragment")
    if parsed.username is not None or parsed.password is not None:
        fail("archive URL must not contain credentials")
    if port is not None and not 1 <= port <= 65_535:
        fail("archive URL port is invalid")
    lowered = hostname.rstrip(".").lower()
    if lowered == "localhost" or lowered.endswith(".localhost"):
        fail("archive URL must not use localhost")
    try:
        ipaddress.ip_address(lowered)
    except ValueError:
        pass
    else:
        fail("archive URL must use a public DNS name, not an IP literal")
    if Path(parsed.path).name != archive.name:
        fail("archive URL filename must match the exact signed archive")
    return raw


def admitted_output(argument: str) -> Path:
    raw = Path(argument)
    if raw.is_symlink():
        fail("update manifest output must not be a symlink")
    output = (REPO / raw).resolve() if not raw.is_absolute() else raw.resolve()
    if not inside(output, TARGET):
        fail("update manifest output must be inside this checkout's target directory")
    if output.exists():
        fail("update manifest output already exists; refusing to overwrite it")
    return output


def configured_version() -> str:
    version = json.loads((REPO / "desktop/backend/tauri.conf.json").read_text())["version"]
    if not isinstance(version, str) or not STABLE_SEMVER.fullmatch(version):
        fail("Tauri version must be stable SemVer without a prerelease")
    return version


def build(
    archive_argument: str,
    signature_argument: str,
    archive_url: str,
    notes: str,
    output_argument: str,
    revision: str,
    commit_time: str,
) -> Path:
    archive = admitted_archive(archive_argument)
    signature = admitted_signature(signature_argument, archive)
    url = admitted_url(archive_url, archive)
    if len(notes.encode("utf-8")) > MAX_NOTES_BYTES:
        fail("release notes are oversized")
    output = admitted_output(output_argument)
    platform = {
        "signature": base64.b64encode(signature).decode("ascii"),
        "url": url,
    }
    manifest = {
        "version": configured_version(),
        "notes": notes,
        "pub_date": commit_time,
        "platforms": {target: platform.copy() for target in PLATFORMS},
        "garive": {"git_revision": revision, "archive": archive.name},
    }
    encoded = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{output.name}.", dir=output.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, output)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise
    print(f"update_manifest={output.relative_to(REPO)}")
    print(f"update_revision={revision}")
    print(f"update_archive={archive.relative_to(REPO)}")
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True)
    parser.add_argument("--signature", required=True)
    parser.add_argument("--archive-url", required=True)
    parser.add_argument("--notes", default="")
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    revision, commit_time = clean_revision()
    build(arguments.archive, arguments.signature, arguments.archive_url, arguments.notes,
          arguments.output, revision, commit_time)


if __name__ == "__main__":
    main()
