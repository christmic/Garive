#!/usr/bin/env python3
"""Build candidate-bound macOS release materials without claiming update support."""

from __future__ import annotations

import argparse
import base64
import hashlib
import ipaddress
import json
import os
import shutil
import subprocess
import tempfile
import uuid
from pathlib import Path
from urllib.parse import quote
from urllib.parse import urlsplit


SCRIPT = Path(__file__).resolve()
REPO = SCRIPT.parents[2]
TARGET = REPO / "target"
DESKTOP_PACKAGE = "garive-desktop"
PLATFORMS = ("aarch64-apple-darwin", "x86_64-apple-darwin")
UPDATE_PLATFORMS = ("darwin-aarch64", "darwin-x86_64")


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


def within(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def admitted_artifact(argument: str, label: str) -> Path:
    raw = Path(argument)
    if raw.is_symlink():
        fail(f"{label} must not be a symlink")
    path = (REPO / raw).resolve() if not raw.is_absolute() else raw.resolve()
    if not within(path, TARGET.resolve()) or not path.is_file() or path.stat().st_size == 0:
        fail(f"{label} must be a nonempty file inside this checkout's target directory")
    return path


def update_assets(arguments: argparse.Namespace, revision: str, version: str) -> dict | None:
    values = (
        arguments.updater_archive,
        arguments.updater_signature,
        arguments.update_manifest,
        arguments.updater_config,
    )
    if not any(values):
        if arguments.mode == "release":
            fail("release mode requires updater archive, signature, manifest, and config")
        return None
    if not all(values):
        fail("updater archive, signature, manifest, and config must be supplied together")
    archive = admitted_artifact(arguments.updater_archive, "updater archive")
    signature = admitted_artifact(arguments.updater_signature, "updater signature")
    manifest_path = admitted_artifact(arguments.update_manifest, "update manifest")
    config_path = admitted_artifact(arguments.updater_config, "updater config")
    if not archive.name.endswith(".app.tar.gz") or signature != Path(f"{archive}.sig"):
        fail("updater archive/signature names do not form an adjacent .app.tar.gz pair")
    try:
        manifest = json.loads(manifest_path.read_text())
        updater_config = json.loads(config_path.read_text())
    except (UnicodeDecodeError, json.JSONDecodeError):
        fail("updater manifest and config must be UTF-8 JSON")
    if not isinstance(manifest, dict) or not isinstance(updater_config, dict):
        fail("updater manifest and config must be JSON objects")
    garive_binding = manifest.get("garive")
    if not isinstance(garive_binding, dict) or manifest.get("version") != version \
            or garive_binding.get("git_revision") != revision:
        fail("update manifest is not bound to this version and Git revision")
    platforms = manifest.get("platforms")
    if not isinstance(platforms, dict) or set(platforms) != set(UPDATE_PLATFORMS):
        fail("update manifest must contain exactly both macOS architectures")
    encoded_signature = base64.b64encode(signature.read_bytes()).decode("ascii")
    urls = set()
    for platform in platforms.values():
        if not isinstance(platform, dict) or platform.get("signature") != encoded_signature:
            fail("update manifest signature does not match the exact signature file")
        url = platform.get("url")
        if not isinstance(url, str) or not url:
            fail("update manifest contains an invalid archive URL")
        try:
            parsed = urlsplit(url)
        except ValueError:
            fail("update manifest contains an invalid archive URL")
        hostname = parsed.hostname or ""
        if parsed.scheme != "https" or parsed.username is not None or parsed.password is not None \
                or parsed.query or parsed.fragment or Path(parsed.path).name != archive.name:
            fail("update manifest archive URL is not an exact public HTTPS artifact")
        lowered = hostname.rstrip(".").lower()
        try:
            ipaddress.ip_address(lowered)
        except ValueError:
            if not lowered or lowered == "localhost" or lowered.endswith(".localhost"):
                fail("update manifest archive URL must use a public DNS name")
        else:
            fail("update manifest archive URL must not use an IP literal")
        urls.add(url)
    if len(urls) != 1:
        fail("both macOS targets must use the same Universal updater archive")
    plugins = updater_config.get("plugins")
    updater = plugins.get("updater") if isinstance(plugins, dict) else None
    if updater_config.get("bundle", {}).get("createUpdaterArtifacts") is not True:
        fail("updater config does not enable signed updater artifacts")
    if not isinstance(updater, dict) or not isinstance(updater.get("endpoints"), list) \
            or not updater.get("endpoints") or not isinstance(updater.get("pubkey"), str) \
            or not updater.get("pubkey") or any(
        updater.get(flag) is not False for flag in (
            "dangerousInsecureTransportProtocol",
            "dangerousAcceptInvalidCerts",
            "dangerousAcceptInvalidHostnames",
        )
    ):
        fail("updater config is incomplete or enables a dangerous transport flag")
    return {
        "archive_path": archive.relative_to(REPO).as_posix(),
        "archive_sha256": sha256(archive),
        "signature_path": signature.relative_to(REPO).as_posix(),
        "signature_sha256": sha256(signature),
        "manifest_path": manifest_path.relative_to(REPO).as_posix(),
        "manifest_sha256": sha256(manifest_path),
        "config_path": config_path.relative_to(REPO).as_posix(),
        "config_sha256": sha256(config_path),
        "archive_url": next(iter(urls)),
    }


def clean_candidate(package_arg: str) -> tuple[Path, str]:
    if run("git", "status", "--porcelain").strip():
        fail("release materials require a clean Git worktree")
    revision = run("git", "rev-parse", "HEAD").strip()
    package_input = Path(package_arg)
    if package_input.is_symlink():
        fail("candidate package must not be a symlink")
    package = (REPO / package_input).resolve() if not package_input.is_absolute() else package_input.resolve()
    if not within(package, TARGET.resolve()):
        fail("candidate package must be inside this checkout's target directory")
    if package.suffix.lower() != ".dmg" or not package.is_file():
        fail("candidate package must be an existing DMG")
    return package, revision


def verify_package(package: Path, mode: str) -> tuple[list[str], str]:
    output = run(str(REPO / "desktop/release/verify-macos-bundle.sh"), str(package), mode)
    print(output, end="")
    fields = dict(
        line.split("=", 1) for line in output.splitlines() if "=" in line
    )
    architectures = fields.get("architectures", "").split()
    if set(architectures) != {"arm64", "x86_64"}:
        fail("release materials require exactly the arm64 and x86_64 slices")
    verified_hash = fields.get("sha256", "")
    if not verified_hash or verified_hash != sha256(package):
        fail("candidate digest differs from the bundle verifier")
    return sorted(architectures), verified_hash


def reachable_rust_packages() -> list[dict]:
    selected: dict[str, dict] = {}
    for platform in PLATFORMS:
        metadata = json.loads(
            run(
                "cargo", "metadata", "--format-version", "1", "--locked",
                "--filter-platform", platform,
            )
        )
        packages = {package["id"]: package for package in metadata["packages"]}
        root = next(
            (package["id"] for package in metadata["packages"] if package["name"] == DESKTOP_PACKAGE),
            None,
        )
        if root is None:
            fail(f"Cargo metadata does not contain {DESKTOP_PACKAGE}")
        nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
        pending = [root]
        seen: set[str] = set()
        while pending:
            package_id = pending.pop()
            if package_id in seen:
                continue
            seen.add(package_id)
            selected[package_id] = packages[package_id]
            for dependency in nodes[package_id]["deps"]:
                kinds = dependency.get("dep_kinds", [])
                if any(kind.get("kind") is None for kind in kinds):
                    pending.append(dependency["pkg"])
    return sorted(selected.values(), key=lambda item: (item["name"], item["version"], item["id"]))


def npm_packages() -> list[dict]:
    report = json.loads(
        run("pnpm", "--dir", "desktop/frontend", "licenses", "list", "--prod", "--json")
    )
    packages: dict[tuple[str, str], dict] = {}
    for license_name, entries in report.items():
        for entry in entries:
            versions = entry.get("versions", [])
            if not versions:
                fail(f"npm license entry has no version: {entry.get('name', '<unknown>')}")
            for version in versions:
                key = (entry["name"], version)
                candidate = {
                    "ecosystem": "npm",
                    "name": entry["name"],
                    "version": version,
                    "license": entry.get("license") or license_name,
                    "homepage": entry.get("homepage") or "",
                    "description": entry.get("description") or "",
                }
                previous = packages.get(key)
                if previous and previous != candidate:
                    fail(f"conflicting npm license metadata: {entry['name']}@{version}")
                packages[key] = candidate
    return [packages[key] for key in sorted(packages)]


def components(rust_packages: list[dict], npm: list[dict]) -> tuple[list[dict], list[dict]]:
    bom: list[dict] = []
    licenses: list[dict] = []
    for package in rust_packages:
        source = package.get("source") or ""
        license_name = package.get("license") or ""
        if source and not license_name:
            fail(f"Rust dependency has no declared license: {package['name']}@{package['version']}")
        purl = f"pkg:cargo/{quote(package['name'], safe='')}@{package['version']}"
        component = {
            "type": "library",
            "bom-ref": purl,
            "name": package["name"],
            "version": package["version"],
            "purl": purl,
            "licenses": [{"expression": license_name}] if license_name else [],
        }
        checksum = package.get("checksum")
        if checksum:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        bom.append(component)
        if source:
            licenses.append({
                "ecosystem": "cargo", "name": package["name"],
                "version": package["version"], "license": license_name,
                "homepage": package.get("homepage") or package.get("repository") or "",
                "description": package.get("description") or "",
            })
    for package in npm:
        if not package["license"]:
            fail(f"npm dependency has no declared license: {package['name']}@{package['version']}")
        purl = f"pkg:npm/{quote(package['name'], safe='/')}@{package['version']}"
        bom.append({
            "type": "library", "bom-ref": purl, "name": package["name"],
            "version": package["version"], "purl": purl,
            "licenses": [{"expression": package["license"]}],
        })
        licenses.append(package)
    bom.sort(key=lambda item: item["bom-ref"])
    licenses.sort(key=lambda item: (item["ecosystem"], item["name"], item["version"]))
    return bom, licenses


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def build_materials(package: Path, revision: str, arguments: argparse.Namespace) -> Path:
    config = json.loads((REPO / "desktop/backend/tauri.conf.json").read_text())
    mode = arguments.mode
    updater = update_assets(arguments, revision, config["version"])
    architectures, digest = verify_package(package, mode)
    commit_time = run("git", "show", "-s", "--format=%cI", revision).strip()
    rust = reachable_rust_packages()
    npm = npm_packages()
    bom_components, license_inventory = components(rust, npm)
    output = Path(arguments.output) if arguments.output else TARGET / "desktop-release" / digest[:16]
    output = (REPO / output).resolve() if not output.is_absolute() else output.resolve()
    if not within(output, TARGET.resolve()):
        fail("materials output must be inside this checkout's target directory")
    if output.exists() or output.is_symlink():
        fail("materials output already exists; refusing to overwrite it")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=f".{output.name}.", dir=output.parent))
    package_relative = package.relative_to(REPO).as_posix()
    try:
        bom = {
            "bomFormat": "CycloneDX", "specVersion": "1.6", "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, revision + digest)}",
            "version": 1,
            "metadata": {
                "timestamp": commit_time,
                "component": {
                    "type": "application", "name": "Garive", "version": config["version"],
                    "bom-ref": f"pkg:generic/garive@{config['version']}?git_revision={revision}",
                    "hashes": [{"alg": "SHA-256", "content": digest}],
                    "properties": [{"name": "garive:candidate-path", "value": package_relative}],
                },
            },
            "components": bom_components,
        }
        provenance = {
            "schema_version": 1, "mode": mode, "release_eligible": mode == "release",
            "git_revision": revision, "git_commit_time": commit_time,
            "package_path": package_relative, "package_sha256": digest,
            "version": config["version"], "bundle_identifier": config["identifier"],
            "minimum_macos": config["bundle"]["macOS"]["minimumSystemVersion"],
            "architectures": architectures,
            "updater_implemented": True,
            "update_assets": updater,
        }
        write_json(temporary / "garive-macos.cdx.json", bom)
        write_json(temporary / "THIRD_PARTY_LICENSES.json", {"schema_version": 1, "packages": license_inventory})
        write_json(temporary / "release-provenance.json", provenance)
        checksums = [(digest, package.name)]
        if updater:
            checksums.extend([
                (updater["archive_sha256"], Path(updater["archive_path"]).name),
                (updater["signature_sha256"], Path(updater["signature_path"]).name),
                (updater["manifest_sha256"], Path(updater["manifest_path"]).name),
                (updater["config_sha256"], Path(updater["config_path"]).name),
            ])
        names = [name for _, name in checksums]
        if len(names) != len(set(names)):
            fail("release artifact basenames must be unique")
        (temporary / "SHA256SUMS").write_text(
            "".join(f"{checksum}  {name}\n" for checksum, name in checksums)
        )
        (temporary / "ROLLBACK.md").write_text(
            "# Garive macOS rollback boundary\n\n"
            f"Candidate `{revision}` / `{digest}` was audited in `{mode}` mode.\n\n"
            "These materials do not make this candidate update-eligible or public. A pre-install validation failure must leave the installed "
            "app and user data untouched. An outcome-unknown install must retain its reconciliation record and never retry automatically. "
            "Keep the current installed app and data until the signed candidate passes clean-Mac install and migration validation. Downgrade "
            "is never automatic: first prove storage "
            "and configuration schema compatibility, then restore a separately retained, verified installer. Local-mode candidates must not be published.\n"
        )
        os.replace(temporary, output)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    print(f"release_materials={output.relative_to(REPO)}")
    print(f"rust_components={len(rust)}")
    print(f"npm_components={len(npm)}")
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", help="exact Universal Garive DMG under target/")
    parser.add_argument("--mode", choices=("local", "release"), default="local")
    parser.add_argument("--output", help="new output directory under target/")
    parser.add_argument("--updater-archive")
    parser.add_argument("--updater-signature")
    parser.add_argument("--update-manifest")
    parser.add_argument("--updater-config")
    arguments = parser.parse_args()
    package, revision = clean_candidate(arguments.package)
    build_materials(package, revision, arguments)


if __name__ == "__main__":
    main()
