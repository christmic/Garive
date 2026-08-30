import argparse
import base64
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[3]
SCRIPT = REPO / "desktop/release/build-release-materials.py"
TARGET = REPO / "target"
SPEC = importlib.util.spec_from_file_location("build_release_materials", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BuildReleaseMaterialsTest(unittest.TestCase):
    def setUp(self):
        TARGET.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="release-materials-test-", dir=TARGET)
        self.root = Path(self.temporary.name)
        self.revision = "b" * 40
        self.archive = self.root / "Garive.app.tar.gz"
        self.archive.write_bytes(b"archive")
        self.signature = Path(f"{self.archive}.sig")
        self.signature.write_bytes(b"signature")
        encoded_signature = base64.b64encode(self.signature.read_bytes()).decode()
        self.manifest = self.root / "latest.json"
        self.manifest.write_text(json.dumps({
            "version": "0.1.0",
            "platforms": {
                platform: {
                    "url": "https://releases.example.com/garive/Garive.app.tar.gz",
                    "signature": encoded_signature,
                }
                for platform in MODULE.UPDATE_PLATFORMS
            },
            "garive": {"git_revision": self.revision},
        }))
        self.config = self.root / "updater.json"
        self.config.write_text(json.dumps({
            "bundle": {"createUpdaterArtifacts": True},
            "plugins": {"updater": {
                "endpoints": ["https://releases.example.com/garive/latest.json"],
                "pubkey": "public key",
                "dangerousInsecureTransportProtocol": False,
                "dangerousAcceptInvalidCerts": False,
                "dangerousAcceptInvalidHostnames": False,
            }},
        }))

    def tearDown(self):
        self.temporary.cleanup()

    def arguments(self, **changes):
        values = {
            "mode": "release",
            "updater_archive": str(self.archive),
            "updater_signature": str(self.signature),
            "update_manifest": str(self.manifest),
            "updater_config": str(self.config),
        }
        values.update(changes)
        return argparse.Namespace(**values)

    def test_binds_every_update_artifact_digest_to_release_provenance(self):
        assets = MODULE.update_assets(self.arguments(), self.revision, "0.1.0")
        self.assertEqual(assets["archive_sha256"], MODULE.sha256(self.archive))
        self.assertEqual(assets["signature_sha256"], MODULE.sha256(self.signature))
        self.assertEqual(assets["manifest_sha256"], MODULE.sha256(self.manifest))
        self.assertEqual(assets["config_sha256"], MODULE.sha256(self.config))
        self.assertEqual(
            assets["archive_url"],
            "https://releases.example.com/garive/Garive.app.tar.gz",
        )

    def test_release_requires_a_complete_exact_update_set(self):
        with self.assertRaises(SystemExit):
            MODULE.update_assets(self.arguments(updater_config=None), self.revision, "0.1.0")
        with self.assertRaises(SystemExit):
            MODULE.update_assets(
                self.arguments(updater_archive=None, updater_signature=None,
                               update_manifest=None, updater_config=None),
                self.revision, "0.1.0",
            )
        manifest = json.loads(self.manifest.read_text())
        manifest["garive"]["git_revision"] = "c" * 40
        self.manifest.write_text(json.dumps(manifest))
        with self.assertRaises(SystemExit):
            MODULE.update_assets(self.arguments(), self.revision, "0.1.0")


if __name__ == "__main__":
    unittest.main()
