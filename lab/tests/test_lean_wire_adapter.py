"""The real LEAN translation adapter must preserve production bridge wire data."""
import hashlib
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import unittest

LAB = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get("TRADY_TEST_RUST_BINARY", str(LAB.parent / "target/debug/trady")))
spec = importlib.util.spec_from_file_location("trady_lean_formula_wire", LAB / "lean/bridge.py")
bridge = importlib.util.module_from_spec(spec); spec.loader.exec_module(bridge)


@unittest.skipUnless(BINARY.is_file(), "build trady or provide TRADY_TEST_RUST_BINARY for executable wire tests")
class LeanWireAdapter(unittest.TestCase):
    def test_adapter_fixture_cli_and_actual_runtime_method_share_exact_wire(self):
        digest = hashlib.sha256(BINARY.read_bytes()).hexdigest()
        adapter = bridge.RustFormulaBridge(str(BINARY.resolve()), digest)
        for request in ({"kind": "DECIMAL", "operation": "ADD", "a": "0.10", "b": "0.20"},
                        {"kind": "DECIMAL", "operation": "DIV", "a": "1", "b": "0"},
                        {"kind": "DECIMAL", "operation": "MUL", "a": "79228162514264337593543950335", "b": "2"},
                        {"kind": "DECIMAL", "operation": "FLOOR", "a": "1.001", "b": "0.01"}):
            data = json.dumps(request).encode()
            direct = subprocess.run([str(BINARY), "weekly", "formula-bridge", "--file", "-"], input=data, capture_output=True, timeout=10)
            self.assertEqual(direct.returncode, 0, direct.stderr.decode())
            self.assertEqual(adapter.fixture(request), json.loads(direct.stdout))
            translated = subprocess.run([sys.executable, "-B", str(LAB / "lean/bridge.py"), "--fixture", "--binary", str(BINARY), "--sha256", digest],
                                        input=data, capture_output=True, timeout=10)
            self.assertEqual(translated.returncode, 0, translated.stderr.decode())
            self.assertEqual(json.loads(translated.stdout), json.loads(direct.stdout))
        with self.assertRaises(ValueError):
            bridge.RustFormulaBridge(str(BINARY), "0" * 64)


if __name__ == "__main__":
    unittest.main()
