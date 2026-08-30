import base64
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[3]
SCRIPT = REPO / "desktop/release/build-updater-config.py"
TARGET = REPO / "target"


class BuildUpdaterConfigTest(unittest.TestCase):
    def setUp(self):
        TARGET.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="updater-config-test-", dir=TARGET)
        self.root = Path(self.temporary.name)
        encoded = base64.b64encode(b"Ed" + bytes(range(40))).decode()
        self.public_key = self.root / "garive.pub"
        self.public_key.write_text(f"untrusted comment: minisign public key\n{encoded}\n")

    def tearDown(self):
        self.temporary.cleanup()

    def run_script(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(SCRIPT), *arguments], cwd=REPO,
            text=True, capture_output=True, check=False,
        )

    def test_builds_bounded_release_overlay_without_private_material(self):
        output = self.root / "release-updater.json"
        result = self.run_script(
            "--endpoint", "https://releases.example.com/garive/{{target}}/{{arch}}/{{current_version}}",
            "--public-key", str(self.public_key), "--output", str(output),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        config = json.loads(output.read_text())
        self.assertEqual(config["bundle"]["createUpdaterArtifacts"], True)
        updater = config["plugins"]["updater"]
        self.assertEqual(updater["pubkey"], self.public_key.read_text().strip())
        self.assertEqual(len(updater["endpoints"]), 1)
        self.assertEqual(updater["dangerousInsecureTransportProtocol"], False)
        self.assertNotIn(str(self.public_key), output.read_text())
        self.assertIn("updater_config_sha256=", result.stdout)

    def test_rejects_insecure_keys_paths_channels_and_overwrites(self):
        cases = [
            ["--endpoint", "http://releases.example.com/latest.json"],
            ["--endpoint", "https://user:secret@releases.example.com/latest.json"],
            ["--endpoint", "https://127.0.0.1/latest.json"],
            ["--endpoint", "https://localhost/latest.json"],
            ["--endpoint", "https://releases.example.com/latest.json#fragment"],
        ]
        for index, endpoint in enumerate(cases):
            result = self.run_script(*endpoint, "--public-key", str(self.public_key),
                                     "--output", str(self.root / f"rejected-{index}.json"))
            self.assertNotEqual(result.returncode, 0, endpoint)

        invalid_key = self.root / "invalid.pub"
        invalid_key.write_text("not a minisign key\n")
        result = self.run_script("--endpoint", "https://releases.example.com/latest.json",
                                 "--public-key", str(invalid_key),
                                 "--output", str(self.root / "invalid-key.json"))
        self.assertNotEqual(result.returncode, 0)

        link = self.root / "linked.pub"
        link.symlink_to(self.public_key)
        result = self.run_script("--endpoint", "https://releases.example.com/latest.json",
                                 "--public-key", str(link),
                                 "--output", str(self.root / "linked-key.json"))
        self.assertNotEqual(result.returncode, 0)

        outside = Path(tempfile.gettempdir()) / "garive-updater-config-outside.json"
        result = self.run_script("--endpoint", "https://releases.example.com/latest.json",
                                 "--public-key", str(self.public_key), "--output", str(outside))
        self.assertNotEqual(result.returncode, 0)

        occupied = self.root / "occupied.json"
        occupied.write_text("preserve")
        result = self.run_script("--endpoint", "https://releases.example.com/latest.json",
                                 "--public-key", str(self.public_key), "--output", str(occupied))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(occupied.read_text(), "preserve")


if __name__ == "__main__":
    unittest.main()
