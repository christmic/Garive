import base64
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[3]
SCRIPT = REPO / "desktop/release/build-update-manifest.py"
TARGET = REPO / "target"
SPEC = importlib.util.spec_from_file_location("build_update_manifest", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BuildUpdateManifestTest(unittest.TestCase):
    def setUp(self):
        TARGET.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="update-manifest-test-", dir=TARGET)
        self.root = Path(self.temporary.name)
        self.archive = self.root / "Garive_0.1.0_universal.app.tar.gz"
        self.archive.write_bytes(b"signed updater archive")
        self.signature = Path(f"{self.archive}.sig")
        primary = base64.b64encode(b"ED" + bytes(range(72))).decode()
        global_signature = base64.b64encode(bytes(range(64))).decode()
        self.signature.write_text(
            "untrusted comment: signature from minisign secret key\n"
            f"{primary}\ntrusted comment: timestamp:1\n{global_signature}\n"
        )

    def tearDown(self):
        self.temporary.cleanup()

    def build(self, **changes):
        values = {
            "archive_argument": str(self.archive),
            "signature_argument": str(self.signature),
            "archive_url": f"https://releases.example.com/garive/{self.archive.name}",
            "notes": "Verified desktop update",
            "output_argument": str(self.root / "latest.json"),
            "revision": "a" * 40,
            "commit_time": "2026-08-30T23:00:00+08:00",
        }
        values.update(changes)
        return MODULE.build(**values)

    def test_binds_both_macos_targets_to_one_exact_signed_archive(self):
        output = self.build()
        manifest = json.loads(output.read_text())
        self.assertEqual(manifest["version"], "0.1.0")
        self.assertEqual(set(manifest["platforms"]), {"darwin-aarch64", "darwin-x86_64"})
        for platform in manifest["platforms"].values():
            self.assertEqual(platform["url"], f"https://releases.example.com/garive/{self.archive.name}")
            self.assertEqual(base64.b64decode(platform["signature"]), self.signature.read_bytes())
        self.assertEqual(manifest["garive"]["git_revision"], "a" * 40)

    def test_rejects_ambiguous_inputs_and_never_overwrites(self):
        invalid_signature = self.root / "invalid.app.tar.gz.sig"
        invalid_signature.write_text("not a signature")
        cases = [
            {"archive_url": f"http://releases.example.com/{self.archive.name}"},
            {"archive_url": f"https://user:secret@releases.example.com/{self.archive.name}"},
            {"archive_url": f"https://127.0.0.1/{self.archive.name}"},
            {"archive_url": f"https://localhost/{self.archive.name}"},
            {"archive_url": "https://releases.example.com/other.app.tar.gz"},
            {"signature_argument": str(invalid_signature)},
        ]
        for index, changes in enumerate(cases):
            changes["output_argument"] = str(self.root / f"rejected-{index}.json")
            with self.assertRaises(SystemExit, msg=changes):
                self.build(**changes)

        occupied = self.root / "occupied.json"
        occupied.write_text("preserve")
        with self.assertRaises(SystemExit):
            self.build(output_argument=str(occupied))
        self.assertEqual(occupied.read_text(), "preserve")


if __name__ == "__main__":
    unittest.main()
