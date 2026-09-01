import plistlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).parents[1] / "build_process_xpc.py"
BUNDLE_IDENTIFIER = "com.garive.desktop.process-isolation-service"


class ProcessXPCBundleTests(unittest.TestCase):
    def test_builds_and_validates_exact_signed_layout(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            root = Path(value)
            executable = self.executable(root)
            codesign = self.codesign_stub(root)
            output = root / "GariveProcessIsolationService.xpc"
            command = self.command(executable, codesign, output)
            subprocess.run(command, check=True, capture_output=True, text=True)

            metadata = plistlib.loads((output / "Contents/Info.plist").read_bytes())
            self.assertEqual(metadata["CFBundleIdentifier"], BUNDLE_IDENTIFIER)
            self.assertEqual(metadata["CFBundlePackageType"], "XPC!")
            self.assertEqual(metadata["XPCService"], {"ServiceType": "Application"})
            self.assertEqual(
                metadata["GariveBackendCodeSigningRequirement"],
                'identifier "com.garive.desktop" and anchor apple generic',
            )
            files = {item.relative_to(output) for item in output.rglob("*") if item.is_file()}
            self.assertEqual(
                files,
                {
                    Path("Contents/Info.plist"),
                    Path("Contents/MacOS/GariveProcessIsolationService"),
                    Path("Contents/_CodeSignature/CodeResources"),
                },
            )
            rejected = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(rejected.returncode, 0)

    def test_rejects_inexact_identity_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            root = Path(value)
            command = self.command(
                self.executable(root), self.codesign_stub(root), root / "service.xpc"
            )
            command[command.index(BUNDLE_IDENTIFIER)] = "com.garive.wrong"
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "service.xpc").exists())

    @staticmethod
    def executable(root: Path) -> Path:
        value = root / "service-bin"
        value.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        value.chmod(0o755)
        return value

    @staticmethod
    def codesign_stub(root: Path) -> Path:
        value = root / "codesign-stub"
        value.write_text(
            "#!/bin/sh\n"
            "set -eu\n"
            "bundle=\"\"\n"
            "for argument in \"$@\"; do bundle=\"$argument\"; done\n"
            "case \" $* \" in\n"
            "  *' --sign '*) mkdir -p \"$bundle/Contents/_CodeSignature\"; "
            ": > \"$bundle/Contents/_CodeSignature/CodeResources\" ;;\n"
            "  *' --display '*) echo 'Identifier=com.garive.desktop.process-isolation-service' >&2 ;;\n"
            "esac\n",
            encoding="utf-8",
        )
        value.chmod(0o755)
        return value

    @staticmethod
    def command(executable: Path, codesign: Path, output: Path) -> list[str]:
        return [
            sys.executable,
            str(SCRIPT),
            "--executable", str(executable),
            "--output", str(output),
            "--bundle-identifier", BUNDLE_IDENTIFIER,
            "--bundle-version", "1",
            "--short-version", "0.1.0",
            "--backend-requirement", 'identifier "com.garive.desktop" and anchor apple generic',
            "--signing-identity", "Developer ID Application: Garive",
            "--codesign-tool", str(codesign),
        ]


if __name__ == "__main__":
    unittest.main()
