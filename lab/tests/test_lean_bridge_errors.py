"""Only genuine input failures may become adapter parser-rejection evidence."""
from contextlib import nullcontext, redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import pathlib
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

LAB = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("trady_bridge_errors", LAB / "lean/bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class FormulaBridgeErrors(unittest.TestCase):
    def setUp(self):
        self.adapter = object.__new__(bridge.RustFormulaBridge)
        self.adapter.binary = pathlib.Path("/unused/pinned-calculator")

    def call_with(self, result=None, error=None, wire='{"kind":"FORMULA"}'):
        with patch.object(self.adapter, "_executable_snapshot", return_value=nullcontext(3)), \
                patch.object(self.adapter, "_verified_binary"), \
                patch.object(bridge.subprocess, "run", return_value=result, side_effect=error) as run:
            reply = self.adapter.fixture_wire(wire)
            self.assertEqual(run.call_args.kwargs["input"], wire)
            return reply

    def test_only_exact_clean_rust_parser_errors_have_the_dedicated_type(self):
        for diagnostic in bridge.RUST_INPUT_DIAGNOSTICS:
            with self.subTest(diagnostic=diagnostic), self.assertRaises(bridge.FormulaInputRejected):
                self.call_with(subprocess.CompletedProcess([], 1, "", diagnostic))
        for result in [
            subprocess.CompletedProcess([], -11, "", "trady: input schema mismatch\n"),
            subprocess.CompletedProcess([], 1, "", "trady: input read failed\n"),
            subprocess.CompletedProcess([], 1, "", "trady: input schema mismatch\nRuntimeError\n"),
            subprocess.CompletedProcess([], 2, "", "trady: input schema mismatch\n"),
            subprocess.CompletedProcess([], 1, '{"status":"REJECTED"}', "trady: input schema mismatch\n"),
        ]:
            with self.subTest(result=result), self.assertRaises(RuntimeError):
                self.call_with(result)

    def test_file_timeout_runtime_and_wrapped_signal_failures_stay_blocked(self):
        for error in [FileNotFoundError("missing"), subprocess.TimeoutExpired("calculator", 10),
                      RuntimeError("input schema mismatch")]:
            stderr = io.StringIO()
            with self.subTest(error=type(error).__name__), patch.object(bridge, "main", side_effect=lambda: self.call_with(error=error)), redirect_stderr(stderr):
                self.assertEqual(bridge.cli(), 1)
            self.assertEqual(stderr.getvalue(), "LEAN formula fixture BLOCKED: " + type(error).__name__ + "\n")
        stderr = io.StringIO()
        with patch.object(bridge, "main", side_effect=lambda: self.call_with(subprocess.CompletedProcess([], -11, "", "trady: input schema mismatch\n"))), redirect_stderr(stderr):
            self.assertEqual(bridge.cli(), 1)
        self.assertEqual(stderr.getvalue(), "LEAN formula fixture BLOCKED: RuntimeError\n")

    def test_integrity_failure_precedes_parser_error_classification(self):
        with patch.object(self.adapter, "_executable_snapshot", return_value=nullcontext(3)), \
                patch.object(self.adapter, "_verified_binary", side_effect=ValueError("Rust bridge binary changed")), \
                patch.object(bridge.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "trady: input schema mismatch\n")):
            with self.assertRaisesRegex(ValueError, "binary changed") as caught:
                self.adapter.fixture_wire("{}")
        self.assertNotIsInstance(caught.exception, bridge.FormulaInputRejected)

    def test_cli_parser_signal_is_distinct_from_all_generic_exceptions(self):
        stderr = io.StringIO()
        with patch.object(bridge, "main", side_effect=bridge.FormulaInputRejected("input schema mismatch")), redirect_stderr(stderr):
            self.assertEqual(bridge.cli(), 2)
        self.assertEqual(stderr.getvalue(), bridge.PARSE_REJECTED_PREFIX + "input schema mismatch\n")

    def test_raw_wire_lexemes_reach_the_pinned_process_unchanged(self):
        for wire in [r'{"version":1e0}', r'{"version":-0}', r'{"snapshot_id":"\ud83d\ude00"}',
                     r'{"snapshot_id":"\ud800"}']:
            with self.subTest(wire=wire):
                self.assertEqual(self.call_with(subprocess.CompletedProcess([], 0, '{"status":"DECIMAL","value":"1"}', ""), wire=wire)["value"], "1")

    def test_actual_cli_forwards_the_original_document_without_reserializing(self):
        wire = r' {"version":1e0,"zero":-0,"snapshot_id":"\ud83d\ude00"}' + "\n"
        adapter = Mock()
        adapter.fixture_wire.return_value = {"status": "DECIMAL", "value": "1"}
        output = io.StringIO()
        with patch.object(sys, "argv", ["bridge.py", "--fixture", "--binary", "/unused/pinned", "--sha256", "0" * 64]), \
                patch.object(sys, "stdin", SimpleNamespace(buffer=io.BytesIO(wire.encode()))), \
                patch.object(bridge, "RustFormulaBridge", return_value=adapter), redirect_stdout(output):
            bridge.main()
        adapter.fixture_wire.assert_called_once_with(wire)
        self.assertEqual(json.loads(output.getvalue()), adapter.fixture_wire.return_value)

    def test_invalid_response_json_is_not_an_input_parser_failure(self):
        with self.assertRaises(json.JSONDecodeError) as caught:
            self.call_with(subprocess.CompletedProcess([], 0, "{", ""))
        self.assertNotIsInstance(caught.exception, bridge.FormulaInputRejected)


if __name__ == "__main__":
    unittest.main()
