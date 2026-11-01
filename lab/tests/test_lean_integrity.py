"""Executable mutation regressions for the actual formula adapter call path."""
import fcntl
import hashlib
import importlib.util
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

LAB = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("trady_lean_integrity", LAB / "lean/bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
EVIDENCE = pathlib.Path("/tmp/trady-remediation-20261031/evaluator")


class FormulaExecutableIntegrity(unittest.TestCase):
    def setUp(self):
        EVIDENCE.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=EVIDENCE)
        self.addCleanup(self.temporary.cleanup)
        self.binary = pathlib.Path(self.temporary.name) / "calculator"
        self.trusted = (f"#!{pathlib.Path(sys.executable).resolve()}\n"
                        "import json,sys\njson.load(sys.stdin)\n"
                        "print('{\"status\":\"ACCEPTED\",\"limits\":{\"quantity\":\"1.25\"}}')\n").encode()
        self.binary.write_bytes(self.trusted); self.binary.chmod(0o700)
        self.adapter = bridge.RustFormulaBridge(self.binary, hashlib.sha256(self.trusted).hexdigest())

    def test_every_calculation_and_fixture_use_verifies_binary(self):
        self.assertEqual(self.adapter.calculate({}), {"quantity": "1.25"})
        self.assertEqual(self.adapter.fixture({})["status"], "ACCEPTED")
        self.binary.write_bytes(self.trusted + b"# mutation\n")
        for method in (self.adapter.calculate, self.adapter.fixture):
            with self.subTest(method=method.__name__), patch.object(bridge.subprocess, "run") as execute:
                with self.assertRaisesRegex(ValueError, "binary changed"):
                    method({})
                execute.assert_not_called()

    def test_replacement_symlink_nonregular_and_missing_executable_fail_closed(self):
        self.binary.unlink()
        replacement = self.binary.with_name("replacement")
        replacement.write_bytes(self.trusted); replacement.chmod(0o700)
        self.binary.symlink_to(replacement)
        with patch.object(bridge.subprocess, "run") as execute:
            with self.assertRaises(OSError):
                self.adapter.fixture({})
            self.binary.unlink(); self.binary.mkdir()
            with self.assertRaisesRegex(ValueError, "regular executable"):
                self.adapter.fixture({})
            self.binary.rmdir()
            with self.assertRaises(OSError):
                self.adapter.fixture({})
            execute.assert_not_called()

    def test_source_mutation_at_exec_cannot_execute_unverified_bytes(self):
        marker = self.binary.with_name("malicious-executed")
        real_run = subprocess.run
        observed = []

        def mutate_and_execute(*args, **kwargs):
            descriptor, = kwargs["pass_fds"]
            seals = fcntl.fcntl(descriptor, fcntl.F_GET_SEALS)
            self.assertEqual(seals & fcntl.F_SEAL_WRITE, fcntl.F_SEAL_WRITE)
            with self.assertRaises(PermissionError):
                os.write(descriptor, b"untrusted")
            self.binary.write_text(f"#!{pathlib.Path(sys.executable).resolve()}\nfrom pathlib import Path\nPath({str(marker)!r}).touch()\n")
            result = real_run(*args, **kwargs)
            observed.append(result)
            return result

        with patch.object(bridge.subprocess, "run", side_effect=mutate_and_execute):
            with self.assertRaisesRegex(ValueError, "binary changed"):
                self.adapter.fixture({})
        self.assertFalse(marker.exists())
        self.assertEqual(observed[0].returncode, 0, observed[0].stderr)
        self.assertIn('"ACCEPTED"', observed[0].stdout)

    def test_sealing_failure_rejects_before_execution_and_closes_snapshot(self):
        descriptors = []
        real_memfd = os.memfd_create

        def create(*args, **kwargs):
            descriptor = real_memfd(*args, **kwargs)
            descriptors.append(descriptor)
            return descriptor

        with patch.object(bridge.os, "memfd_create", side_effect=create), \
                patch.object(bridge.fcntl, "fcntl", side_effect=OSError("sealing unavailable")), \
                patch.object(bridge.subprocess, "run") as execute:
            with self.assertRaises(OSError):
                self.adapter.fixture({})
            execute.assert_not_called()
        with self.assertRaises(OSError):
            os.fstat(descriptors[0])


if __name__ == "__main__":
    unittest.main()
