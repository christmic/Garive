#!/usr/bin/env python3
"""Build candidate-bound macOS release materials without claiming update support."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import uuid
from pathlib import Path
from urllib.parse import quote


SCRIPT = Path(__file__).resolve()
REPO = SCRIPT.parents[2]
TARGET = REPO / "target"
DESKTOP_PACKAGE = "garive-desktop"
PLATFORMS = ("aarch64-apple-darwin", "x86_64-apple-darwin")


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


def build_materials(package: Path, revision: str, mode: str, output_arg: str | None) -> Path:
    config = json.loads((REPO / "desktop/backend/tauri.conf.json").read_text())
    architectures, digest = verify_package(package, mode)
    commit_time = run("git", "show", "-s", "--format=%cI", revision).strip()
    rust = reachable_rust_packages()
    npm = npm_packages()
    bom_components, license_inventory = components(rust, npm)
    output = Path(output_arg) if output_arg else TARGET / "desktop-release" / digest[:16]
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
            "updater_implemented": False,
        }
        write_json(temporary / "garive-macos.cdx.json", bom)
        write_json(temporary / "THIRD_PARTY_LICENSES.json", {"schema_version": 1, "packages": license_inventory})
        write_json(temporary / "release-provenance.json", provenance)
        (temporary / "SHA256SUMS").write_text(f"{digest}  {package.name}\n")
        (temporary / "ROLLBACK.md").write_text(
            "# Garive macOS rollback boundary\n\n"
            f"Candidate `{revision}` / `{digest}` was audited in `{mode}` mode.\n\n"
            "Garive Desktop does not currently implement an updater. These materials do not make this candidate update-eligible or public. "
            "A pre-install validation failure must leave the installed app and user data untouched. Keep the current installed app and data "
            "until the signed candidate passes clean-Mac install and migration validation. Downgrade is never automatic: first prove storage "
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
    arguments = parser.parse_args()
    package, revision = clean_candidate(arguments.package)
    build_materials(package, revision, arguments.mode, arguments.output)


if __name__ == "__main__":
    main()
