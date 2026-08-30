#!/usr/bin/env python3
"""Build a fail-closed public Tauri updater configuration overlay."""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import ipaddress
import json
import os
import tempfile
from pathlib import Path
from urllib.parse import urlsplit


SCRIPT = Path(__file__).resolve()
REPO = SCRIPT.parents[2]
TARGET = (REPO / "target").resolve()
MAX_ENDPOINTS = 2
MAX_ENDPOINT_BYTES = 2_048
MAX_PUBLIC_KEY_BYTES = 16 * 1_024


def fail(message: str) -> "NoReturn":
    raise SystemExit(message)


def inside(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def admitted_endpoint(raw: str) -> str:
    if not raw or len(raw.encode("utf-8")) > MAX_ENDPOINT_BYTES or raw != raw.strip():
        fail("update endpoint is empty, padded, or oversized")
    try:
        parsed = urlsplit(raw)
        port = parsed.port
    except ValueError:
        fail("update endpoint is invalid")
    hostname = parsed.hostname
    if parsed.scheme != "https" or not hostname or parsed.fragment:
        fail("update endpoint must be public HTTPS without a fragment")
    if parsed.username is not None or parsed.password is not None:
        fail("update endpoint must not contain credentials")
    if port is not None and not 1 <= port <= 65_535:
        fail("update endpoint port is invalid")
    lowered = hostname.rstrip(".").lower()
    if lowered == "localhost" or lowered.endswith(".localhost"):
        fail("update endpoint must not use localhost")
    try:
        ipaddress.ip_address(lowered)
    except ValueError:
        pass
    else:
        fail("update endpoint must use a public DNS name, not an IP literal")
    return raw


def admitted_public_key(argument: str) -> str:
    path = Path(argument)
    if path.is_symlink():
        fail("updater public key must not be a symlink")
    path = (REPO / path).resolve() if not path.is_absolute() else path.resolve()
    if not path.is_file():
        fail("updater public key file is missing")
    encoded = path.read_bytes()
    if not encoded or len(encoded) > MAX_PUBLIC_KEY_BYTES:
        fail("updater public key is empty or oversized")
    try:
        text = encoded.decode("utf-8").strip()
    except UnicodeDecodeError:
        fail("updater public key is not UTF-8")
    lines = text.splitlines()
    if len(lines) != 2 or not lines[0].startswith("untrusted comment: minisign public key"):
        fail("updater public key is not a Minisign public-key document")
    try:
        raw_key = base64.b64decode(lines[1], validate=True)
    except (binascii.Error, ValueError):
        fail("updater public key payload is not valid base64")
    if len(raw_key) != 42 or raw_key[:2] != b"Ed":
        fail("updater public key payload has an invalid Minisign shape")
    return text


def output_path(argument: str) -> Path:
    raw = Path(argument)
    if raw.is_symlink():
        fail("updater configuration output must not be a symlink")
    output = (REPO / raw).resolve() if not raw.is_absolute() else raw.resolve()
    if not inside(output, TARGET):
        fail("updater configuration output must be inside this checkout's target directory")
    if output.exists():
        fail("updater configuration output already exists; refusing to overwrite it")
    return output


def build(endpoints: list[str], public_key_argument: str, output_argument: str) -> Path:
    if not 1 <= len(endpoints) <= MAX_ENDPOINTS or len(set(endpoints)) != len(endpoints):
        fail("provide one or two distinct update endpoints")
    admitted_endpoints = [admitted_endpoint(endpoint) for endpoint in endpoints]
    public_key = admitted_public_key(public_key_argument)
    output = output_path(output_argument)
    config = {
        "bundle": {"createUpdaterArtifacts": True},
        "plugins": {"updater": {
            "dangerousInsecureTransportProtocol": False,
            "dangerousAcceptInvalidCerts": False,
            "dangerousAcceptInvalidHostnames": False,
            "endpoints": admitted_endpoints,
            "pubkey": public_key,
        }},
    }
    encoded = (json.dumps(config, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
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
    print(f"updater_config={output.relative_to(REPO)}")
    print(f"updater_config_sha256={hashlib.sha256(encoded).hexdigest()}")
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--endpoint", action="append", required=True)
    parser.add_argument("--public-key", required=True)
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    build(arguments.endpoint, arguments.public_key, arguments.output)


if __name__ == "__main__":
    main()
