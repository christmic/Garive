#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("mobile_physical_admission.py")
SPEC = importlib.util.spec_from_file_location("mobile_physical_admission", SCRIPT)
assert SPEC and SPEC.loader
admission = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(admission)


class EvidenceTests(unittest.TestCase):
    def test_complete_exact_evidence_verifies(self) -> None:
        revision = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            admission.write_evidence(path, admission.new_evidence(revision))
            for platform, step in admission.REQUIRED_STEPS:
                admission.record(path, platform, step, "pass", f"ok.{platform}.{step}")

            with mock.patch.object(admission, "git_revision", return_value=revision):
                admission.verify_evidence(path)

            data = json.loads(path.read_text())
            self.assertIsNotNone(data["completed_at"])
            self.assertEqual(len(data["steps"]), 29)

    def test_failed_or_pending_step_cannot_complete(self) -> None:
        revision = "b" * 40
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            admission.write_evidence(path, admission.new_evidence(revision))
            platform, step = admission.REQUIRED_STEPS[0]
            admission.record(path, platform, step, "fail", "gateway.unreachable")
            with self.assertRaisesRegex(admission.AdmissionError, "single-write"):
                admission.record(path, platform, step, "pass", "verified")
            with mock.patch.object(admission, "git_revision", return_value=revision):
                with self.assertRaisesRegex(admission.AdmissionError, "incomplete"):
                    admission.verify_evidence(path)

    def test_shape_and_stable_codes_fail_closed(self) -> None:
        data = admission.new_evidence("c" * 40)
        data["steps"] = list(reversed(data["steps"]))
        with self.assertRaisesRegex(admission.AdmissionError, "ordered step set"):
            admission.validate_shape(data)

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            admission.write_evidence(path, admission.new_evidence("c" * 40))
            with self.assertRaisesRegex(admission.AdmissionError, "stable code"):
                admission.record(path, "shared", "gateway_runtime_ready", "pass", "unsafe value")

    def test_preflight_reports_only_missing_variable_name(self) -> None:
        with mock.patch.object(admission, "git_revision", return_value="d" * 40), mock.patch.dict(
            os.environ, {}, clear=True
        ):
            with self.assertRaises(admission.AdmissionError) as raised:
                admission.preflight()
        self.assertEqual(raised.exception.code, "configuration_missing")
        self.assertEqual(str(raised.exception), "required environment value is missing: GARIVE_PAIRING_CODE")

    def test_android_manifest_revision_is_exact(self) -> None:
        revision = "e" * 40
        manifest = f'''<manifest xmlns:android="http://schemas.android.com/apk/res/android">
          <application><meta-data android:name="com.garive.build.REVISION" android:value="{revision}" /></application>
        </manifest>'''.encode()
        self.assertEqual(admission.android_manifest_revision(manifest), revision)
        self.assertIsNone(admission.android_manifest_revision(b'<manifest><application /></manifest>'))


if __name__ == "__main__":
    unittest.main()
