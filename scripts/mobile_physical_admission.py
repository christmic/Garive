#!/usr/bin/env python3
"""Fail-closed physical-device admission for the native mobile release."""

from __future__ import annotations

import argparse
import ipaddress
import json
import os
import plistlib
import re
import shutil
import socket
import ssl
import stat
import subprocess
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[1]
SCHEMA_VERSION = 1
PLATFORM_STEPS = (
    "install_launch",
    "pair",
    "discover_agents",
    "create_follow",
    "background_wake",
    "reconnect",
    "answer_suspension",
    "cancel_turn",
    "runtime_restart_terminal",
    "revoke_fail_closed",
    "repair",
    "sign_out",
    "accessibility_large_text",
    "rotation_wide_layout",
)
REQUIRED_STEPS = (("shared", "gateway_runtime_ready"),) + tuple(
    (platform, step) for platform in ("ios", "android") for step in PLATFORM_STEPS
)
CODE_PATTERN = re.compile(r"[a-z0-9][a-z0-9_.-]{0,63}\Z")
EVIDENCE_KEYS = {"schema_version", "revision", "started_at", "completed_at", "steps"}
STEP_KEYS = {"platform", "step", "result", "code", "timestamp"}


class AdmissionError(RuntimeError):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def timestamp(value: object, code: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise AdmissionError(code, "physical admission timestamp is invalid")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        raise AdmissionError(code, "physical admission timestamp is invalid") from exc
    if parsed.tzinfo != timezone.utc:
        raise AdmissionError(code, "physical admission timestamp must be UTC")
    return parsed


def command(*args: str) -> subprocess.CompletedProcess[bytes]:
    try:
        return subprocess.run(
            args,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as exc:
        raise AdmissionError("tool_unavailable", f"required tool is unavailable: {args[0]}") from exc


def require_success(result: subprocess.CompletedProcess[bytes], code: str, message: str) -> None:
    if result.returncode != 0:
        raise AdmissionError(code, message)


def env(name: str, minimum: int = 1) -> str:
    value = os.environ.get(name, "")
    if len(value) < minimum:
        raise AdmissionError("configuration_missing", f"required environment value is missing: {name}")
    return value


def private_file(name: str) -> Path:
    path = Path(env(name)).expanduser()
    if not path.is_file():
        raise AdmissionError("configuration_file_missing", f"required private file is missing: {name}")
    if stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise AdmissionError(
            "configuration_file_permissions",
            f"private file must not be group/world accessible: {name}",
        )
    return path


def public_file(name: str) -> Path:
    path = Path(env(name)).expanduser()
    if not path.is_file():
        raise AdmissionError("configuration_file_missing", f"required file is missing: {name}")
    return path


def release_path(name: str, directory: bool = False) -> Path:
    path = Path(env(name)).expanduser()
    valid = path.is_dir() if directory else path.is_file()
    if not valid:
        raise AdmissionError("release_artifact_missing", f"release artifact is missing: {name}")
    return path


def git_revision(require_clean: bool = True) -> str:
    revision = command("git", "rev-parse", "HEAD")
    require_success(revision, "git_revision_unavailable", "cannot resolve the release revision")
    value = revision.stdout.decode().strip()
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise AdmissionError("git_revision_invalid", "release revision is not an exact commit")
    if require_clean:
        status = command("git", "status", "--porcelain")
        require_success(status, "git_status_unavailable", "cannot inspect the release tree")
        if status.stdout:
            raise AdmissionError("git_tree_dirty", "physical admission requires a clean release tree")
    return value


def verify_tls_material(cert: Path, key: Path) -> None:
    cert_public = command("openssl", "x509", "-in", str(cert), "-pubkey", "-noout")
    key_public = command("openssl", "pkey", "-in", str(key), "-pubout")
    require_success(cert_public, "tls_certificate_invalid", "configured TLS certificate is invalid")
    require_success(key_public, "tls_private_key_invalid", "configured TLS private key is invalid")
    if cert_public.stdout != key_public.stdout:
        raise AdmissionError("tls_key_mismatch", "configured TLS certificate and private key do not match")


def public_tls_origin(cert: Path) -> None:
    parsed = urlsplit(env("GARIVE_GATEWAY_ORIGIN"))
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or parsed.username
        or parsed.password
        or parsed.query
        or parsed.fragment
        or parsed.path not in ("", "/")
    ):
        raise AdmissionError("gateway_origin_invalid", "Gateway origin must be a canonical HTTPS origin")
    try:
        addresses = {
            item[4][0]
            for item in socket.getaddrinfo(parsed.hostname, parsed.port or 443, type=socket.SOCK_STREAM)
        }
    except OSError as exc:
        raise AdmissionError("gateway_dns_unavailable", "Gateway DNS resolution failed") from exc
    if not addresses or any(not ipaddress.ip_address(value).is_global for value in addresses):
        raise AdmissionError("gateway_origin_not_public", "Gateway DNS must resolve only to public addresses")
    context = ssl.create_default_context()
    try:
        with socket.create_connection((parsed.hostname, parsed.port or 443), timeout=8) as raw:
            with context.wrap_socket(raw, server_hostname=parsed.hostname) as secured:
                live_certificate = secured.getpeercert(binary_form=True)
    except (OSError, ssl.SSLError) as exc:
        raise AdmissionError("gateway_tls_untrusted", "Gateway TLS is unreachable or not CA-trusted") from exc
    configured = command("openssl", "x509", "-in", str(cert), "-outform", "DER")
    require_success(configured, "tls_certificate_invalid", "configured TLS certificate is invalid")
    if configured.stdout != live_certificate:
        raise AdmissionError(
            "gateway_tls_certificate_mismatch",
            "live Gateway certificate differs from configured TLS certificate",
        )


def find_apksigner() -> str:
    explicit = os.environ.get("GARIVE_APKSIGNER")
    if explicit:
        return explicit
    found = shutil.which("apksigner")
    if found:
        return found
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    candidates = list(Path(sdk).glob("build-tools/*/apksigner")) if sdk else []
    if candidates:
        def version(path: Path) -> tuple[int, ...]:
            return tuple(int(value) for value in re.findall(r"\d+", path.parent.name))

        return str(max(candidates, key=version))
    raise AdmissionError("apksigner_unavailable", "Android apksigner is unavailable")


def find_apkanalyzer() -> str:
    explicit = os.environ.get("GARIVE_APKANALYZER")
    if explicit:
        return explicit
    found = shutil.which("apkanalyzer")
    if found:
        return found
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    candidate = Path(sdk) / "cmdline-tools/latest/bin/apkanalyzer" if sdk else None
    if candidate and candidate.is_file():
        return str(candidate)
    raise AdmissionError("apkanalyzer_unavailable", "Android apkanalyzer is unavailable")


def android_manifest_revision(payload: bytes) -> str | None:
    try:
        root = ET.fromstring(payload)
    except ET.ParseError as exc:
        raise AdmissionError("android_manifest_invalid", "Android release manifest is invalid") from exc
    namespace = "{http://schemas.android.com/apk/res/android}"
    for item in root.findall("./application/meta-data"):
        if item.get(namespace + "name") == "com.garive.build.REVISION":
            return item.get(namespace + "value")
    return None


def verify_android(revision: str) -> None:
    adb = os.environ.get("GARIVE_ADB") or shutil.which("adb")
    if not adb:
        raise AdmissionError("adb_unavailable", "adb is unavailable")
    serial = env("GARIVE_ANDROID_SERIAL")
    require_success(
        command(adb, "-s", serial, "get-state"),
        "android_device_unavailable",
        "Android physical device is unavailable",
    )
    qemu = command(adb, "-s", serial, "shell", "getprop", "ro.kernel.qemu")
    require_success(qemu, "android_device_unavailable", "Android device properties are unavailable")
    if qemu.stdout.decode(errors="replace").strip() == "1" or serial.startswith("emulator-"):
        raise AdmissionError("android_device_not_physical", "Android admission rejects emulators")
    apk = release_path("GARIVE_ANDROID_RELEASE_APK")
    signature = command(find_apksigner(), "verify", "--verbose", "--print-certs", str(apk))
    require_success(
        signature,
        "android_release_signature_invalid",
        "Android release APK signature verification failed",
    )
    if b"Android Debug" in signature.stdout + signature.stderr:
        raise AdmissionError(
            "android_debug_signature_rejected",
            "Android physical admission rejects the debug signing key",
        )
    manifest = command(find_apkanalyzer(), "manifest", "print", str(apk))
    require_success(manifest, "android_manifest_invalid", "Android release manifest cannot be inspected")
    if android_manifest_revision(manifest.stdout) != revision:
        raise AdmissionError(
            "android_build_revision_mismatch",
            "Android release APK differs from the current Git revision",
        )


def verify_ios(revision: str) -> None:
    device_id = env("GARIVE_IOS_DEVICE_ID")
    require_success(
        command("xcrun", "devicectl", "device", "info", "details", "--device", device_id),
        "ios_device_unavailable",
        "iOS physical device is unavailable",
    )
    identities = command("security", "find-identity", "-v", "-p", "codesigning")
    require_success(identities, "ios_signing_identity_unavailable", "cannot inspect iOS signing identities")
    identity_text = identities.stdout.decode(errors="replace")
    if "0 valid identities found" in identity_text or not re.search(
        r"Apple (Development|Distribution)|iPhone (Developer|Distribution)", identity_text
    ):
        raise AdmissionError(
            "ios_signing_identity_missing",
            "no valid Apple application signing identity is available",
        )
    env("GARIVE_IOS_DEVELOPMENT_TEAM")
    app = release_path("GARIVE_IOS_RELEASE_APP", directory=True)
    require_success(
        command("codesign", "--verify", "--deep", "--strict", str(app)),
        "ios_release_signature_invalid",
        "iOS release app signature verification failed",
    )
    entitlements = command("codesign", "-d", "--entitlements", ":-", str(app))
    require_success(entitlements, "ios_entitlements_unavailable", "iOS release entitlements are unavailable")
    if b"aps-environment" not in entitlements.stdout + entitlements.stderr:
        raise AdmissionError("ios_push_entitlement_missing", "iOS release app lacks the APNs entitlement")
    try:
        info = plistlib.loads((app / "Info.plist").read_bytes())
    except (OSError, plistlib.InvalidFileException) as exc:
        raise AdmissionError("ios_bundle_invalid", "iOS release app Info.plist is invalid") from exc
    if info.get("CFBundleIdentifier") != env("GARIVE_APNS_TOPIC"):
        raise AdmissionError("ios_push_topic_mismatch", "iOS bundle identifier differs from the APNs topic")
    if info.get("GariveBuildRevision") != revision:
        raise AdmissionError("ios_build_revision_mismatch", "iOS release app differs from the current Git revision")


def verify_provider_material() -> None:
    apns_key = private_file("GARIVE_APNS_KEY_FILE")
    require_success(
        command("openssl", "pkey", "-in", str(apns_key), "-noout"),
        "apns_private_key_invalid",
        "APNs private key is invalid",
    )
    if env("GARIVE_IOS_DEVELOPMENT_TEAM") != env("GARIVE_APNS_TEAM_ID"):
        raise AdmissionError("apns_team_mismatch", "iOS signing team differs from the APNs team")
    credentials = private_file("GARIVE_FCM_CREDENTIALS")
    try:
        service_account = json.loads(credentials.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise AdmissionError("fcm_credentials_invalid", "FCM service-account credentials are invalid") from exc
    required = ("project_id", "client_email", "private_key", "token_uri")
    if service_account.get("type") != "service_account" or any(not service_account.get(key) for key in required):
        raise AdmissionError("fcm_credentials_invalid", "FCM service-account credentials are incomplete")
    if service_account["project_id"] != env("GARIVE_FIREBASE_PROJECT_ID"):
        raise AdmissionError("fcm_project_mismatch", "FCM credentials and Android Firebase project differ")


def preflight() -> str:
    revision = git_revision()
    env("GARIVE_PAIRING_CODE")
    env("GARIVE_ADMIN_TOKEN", minimum=20)
    certificate = public_file("GARIVE_TLS_CERT")
    tls_key = private_file("GARIVE_TLS_KEY")
    verify_tls_material(certificate, tls_key)
    public_tls_origin(certificate)
    for name in ("GARIVE_APNS_TEAM_ID", "GARIVE_APNS_KEY_ID", "GARIVE_APNS_TOPIC"):
        env(name)
    for name in (
        "GARIVE_FIREBASE_APP_ID",
        "GARIVE_FIREBASE_API_KEY",
        "GARIVE_FIREBASE_PROJECT_ID",
        "GARIVE_FIREBASE_SENDER_ID",
    ):
        env(name)
    verify_provider_material()
    verify_ios(revision)
    verify_android(revision)
    return revision


def new_evidence(revision: str) -> dict[str, object]:
    return {
        "schema_version": SCHEMA_VERSION,
        "revision": revision,
        "started_at": now(),
        "completed_at": None,
        "steps": [
            {"platform": platform, "step": step, "result": "pending", "code": "pending", "timestamp": None}
            for platform, step in REQUIRED_STEPS
        ],
    }


def load_evidence(path: Path) -> dict[str, object]:
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise AdmissionError("evidence_unreadable", "physical admission evidence is unreadable") from exc
    if data.get("schema_version") != SCHEMA_VERSION:
        raise AdmissionError("evidence_schema_invalid", "physical admission evidence schema is unsupported")
    return data


def validate_shape(data: dict[str, object]) -> list[dict[str, object]]:
    if set(data) != EVIDENCE_KEYS or not re.fullmatch(r"[0-9a-f]{40}", str(data.get("revision", ""))):
        raise AdmissionError("evidence_shape_invalid", "physical admission top-level fields are malformed")
    steps = data.get("steps")
    if (
        not isinstance(steps, list)
        or any(not isinstance(item, dict) or set(item) != STEP_KEYS for item in steps)
    ):
        raise AdmissionError("evidence_shape_invalid", "physical admission steps are malformed")
    keys = [(item.get("platform"), item.get("step")) for item in steps]
    if keys != list(REQUIRED_STEPS):
        raise AdmissionError("evidence_steps_invalid", "physical admission requires the exact ordered step set")
    for item in steps:
        if item["result"] not in ("pending", "pass", "fail") or not CODE_PATTERN.fullmatch(str(item["code"])):
            raise AdmissionError("evidence_shape_invalid", "physical admission result fields are malformed")
        if item["result"] == "pending" and (item["code"] != "pending" or item["timestamp"] is not None):
            raise AdmissionError("evidence_shape_invalid", "pending physical admission steps cannot carry results")
        if item["result"] != "pending":
            timestamp(item["timestamp"], "evidence_timestamp_invalid")
    return steps


def write_evidence(path: Path, data: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=False) + "\n")


def record(path: Path, platform: str, step: str, result: str, code: str) -> None:
    data = load_evidence(path)
    steps = validate_shape(data)
    if (platform, step) not in REQUIRED_STEPS:
        raise AdmissionError("step_unknown", "physical admission step is unknown")
    if result not in ("pass", "fail") or not CODE_PATTERN.fullmatch(code):
        raise AdmissionError("step_result_invalid", "result or stable code is invalid")
    item = next(item for item in steps if item["platform"] == platform and item["step"] == step)
    if item["result"] != "pending":
        raise AdmissionError("step_already_recorded", "physical admission steps are single-write")
    item.update(result=result, code=code, timestamp=now())
    data["completed_at"] = now() if all(entry["result"] == "pass" for entry in steps) else None
    write_evidence(path, data)


def verify_evidence(path: Path) -> None:
    data = load_evidence(path)
    steps = validate_shape(data)
    if data.get("revision") != git_revision():
        raise AdmissionError(
            "evidence_revision_mismatch",
            "physical evidence does not bind the current clean revision",
        )
    started = timestamp(data.get("started_at"), "evidence_timestamp_invalid")
    if not data.get("completed_at"):
        raise AdmissionError("evidence_incomplete", "physical admission evidence is incomplete")
    completed = timestamp(data.get("completed_at"), "evidence_timestamp_invalid")
    for item in steps:
        if (
            item.get("result") != "pass"
            or not item.get("timestamp")
            or not CODE_PATTERN.fullmatch(str(item.get("code", "")))
        ):
            raise AdmissionError("evidence_incomplete", "every physical admission step must pass with a stable code")
        recorded = timestamp(item["timestamp"], "evidence_timestamp_invalid")
        if recorded < started or recorded > completed:
            raise AdmissionError("evidence_timestamp_invalid", "physical admission step timestamp is out of bounds")
    print(f"mobile physical admission verified: {len(steps)} pass steps at {data['revision']}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)
    subparsers.add_parser("list-steps")
    subparsers.add_parser("preflight")
    begin = subparsers.add_parser("begin")
    begin.add_argument("--output", type=Path, required=True)
    update = subparsers.add_parser("record")
    update.add_argument("--evidence", type=Path, required=True)
    update.add_argument("--platform", choices=("shared", "ios", "android"), required=True)
    update.add_argument("--step", required=True)
    update.add_argument("--result", choices=("pass", "fail"), required=True)
    update.add_argument("--code", required=True)
    verify = subparsers.add_parser("verify")
    verify.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()

    if args.action == "list-steps":
        for platform, step in REQUIRED_STEPS:
            print(f"{platform} {step}")
    elif args.action == "preflight":
        revision = preflight()
        print(f"mobile physical admission preflight passed at {revision}")
    elif args.action == "begin":
        write_evidence(args.output, new_evidence(preflight()))
        print("mobile physical admission evidence initialized")
    elif args.action == "record":
        record(
            args.evidence,
            args.platform,
            args.step,
            args.result,
            args.code,
        )
        print("mobile physical admission step recorded")
    else:
        verify_evidence(args.evidence)


if __name__ == "__main__":
    try:
        main()
    except AdmissionError as exc:
        print(f"{exc.code}: {exc}", file=sys.stderr)
        raise SystemExit(1)
