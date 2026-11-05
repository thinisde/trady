"""Retired runtime configs cannot produce deployment artifacts."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PREPARE = ROOT / "services/runtime/prepare.py"


class RuntimePreparationTests(unittest.TestCase):
    def prepare(self, source, destination):
        return subprocess.run(
            [sys.executable, str(PREPARE), "--source", str(source),
             "--destination", str(destination)], capture_output=True, text=True,
            env={}, check=False)

    def test_legacy_config_is_rejected_without_creating_output(self):
        with tempfile.TemporaryDirectory() as name:
            destination = Path(name) / "deployment"
            result = self.prepare(ROOT / "config/autonomous.iex30.demo.json", destination)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("legacy runtime retired", result.stderr)
            self.assertFalse(destination.exists())

    def test_eur_preparation_forces_shadow_disabled_and_preserves_existing_file(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            source = directory / "reviewed.json"
            config = json.loads((ROOT / "config/eur-demo.example.json").read_text())
            config.update(enabled=True, execution_stage="DEMO")
            source.write_text(json.dumps(config))
            destination = directory / "deployment"
            result = self.prepare(source, destination)
            self.assertEqual(result.returncode, 0, result.stderr)
            target = destination / "eur-shadow.json"
            prepared = target.read_bytes()
            self.assertEqual(json.loads(prepared)["execution_stage"], "SHADOW")
            self.assertFalse(json.loads(prepared)["enabled"])
            self.assertFalse((destination / "analyst.json").exists())
            self.assertFalse((destination / "executor.json").exists())
            self.assertNotEqual(self.prepare(source, destination).returncode, 0)
            self.assertEqual(target.read_bytes(), prepared)


if __name__ == "__main__":
    unittest.main()
