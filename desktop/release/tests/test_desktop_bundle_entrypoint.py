import json
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


class DesktopBundleEntrypointTest(unittest.TestCase):
    def test_tauri_bundle_uses_gui_binary_as_main_entrypoint(self) -> None:
        config = json.loads((ROOT / "desktop/backend/tauri.conf.json").read_text())

        self.assertEqual(config["mainBinaryName"], "garive-desktop")
        self.assertEqual(config["plugins"]["updater"], {"endpoints": [], "pubkey": ""})
        self.assertTrue((ROOT / "desktop/backend/src/main.rs").is_file())
        self.assertTrue((ROOT / "desktop/backend/src/bin/garive-host.rs").is_file())
        metadata = json.loads(subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout)
        package = next(item for item in metadata["packages"] if item["name"] == "garive-desktop")
        self.assertEqual(package["default_run"], "garive-desktop")


if __name__ == "__main__":
    unittest.main()
