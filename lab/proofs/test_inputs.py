"""Quick offline checks for transport and parser-rejection evidence semantics."""
import json
import subprocess
import unittest
from unittest.mock import patch

from bindings import strict_json
from fixtures import UNSIGNED_FIELDS, baseline, changed, fixtures
from parity import execute_parse_rejection, malformed_wire_fixtures, timestamp, transport


class TimestampTransportTests(unittest.TestCase):
    def test_offset_and_fraction_conversion_preserves_exact_instants(self):
        utc = timestamp("2026-10-30T14:00:00.123456789Z")
        self.assertEqual(utc, timestamp("2026-10-30T16:00:00.123456789+02:00"))
        self.assertEqual(utc, timestamp("2026-10-30T14:00:00.123456789987Z"))
        self.assertEqual(utc, timestamp("2026-10-30t14:00:00.123456789z"))
        self.assertEqual(utc, timestamp("2026-10-30 14:00:00.123456789Z"))

    def test_represented_leap_second_does_not_alias_next_ordinary_second(self):
        leap = timestamp("2026-10-30T13:59:60.5Z")
        ordinary = timestamp("2026-10-30T14:00:00.5Z")
        self.assertNotEqual(leap, ordinary)
        self.assertEqual(leap % (2 * 10**9), 1500000000)

    def test_gregorian_leap_year_rules_include_century_boundaries(self):
        timestamp("2000-02-29T00:00:00Z")
        timestamp("2024-02-29T00:00:00Z")
        for value in ["1900-02-29T00:00:00Z", "2100-02-29T00:00:00Z"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                timestamp(value)

    def test_impossible_dates_times_offsets_and_types_are_rejected(self):
        for value in ["2026-02-30T00:00:00Z", "2026-00-01T00:00:00Z", "2026-13-01T00:00:00Z",
                      "2026-04-31T00:00:00Z", "2026-10-30T24:00:00Z", "2026-10-30T00:60:00Z",
                      "2026-10-30T00:00:61Z", "2026-10-30T00:00:00+24:00", "2026-10-30T00:00:00-00:60",
                      "２０２６-10-30T00:00:00Z", "+262143-01-01T00:00:00Z", None, 3586737600000000000, True]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                timestamp(value)

    def test_utc_range_is_checked_after_offset_conversion(self):
        timestamp("-262143-01-01T00:00:00Z")
        timestamp("+262142-12-31T23:59:59.999999999Z")
        for value in ["-262143-01-01T00:00:00+00:01", "+262142-12-31T23:59:59-00:01",
                      "+262142-12-31T23:59:60-00:01", "+262142-12-31T23:59:60.999999999-00:01"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                timestamp(value)

    def test_complete_final_represented_leap_second_is_transportable(self):
        ordinary = timestamp("+262142-12-31T23:59:59.999999999Z")
        leap = timestamp("+262142-12-31T23:59:60Z")
        maximum = timestamp("+262142-12-31T23:59:60.999999999Z")
        self.assertEqual(ordinary, 16420533753598999999999)
        self.assertEqual(leap, ordinary + 1)
        self.assertEqual(maximum, 16420533753599999999999)
        self.assertEqual(maximum // (2 * 10**9), ordinary // (2 * 10**9))
        self.assertEqual(maximum % (2 * 10**9), 1999999999)

    def test_negative_fixture_reaches_the_decoder_at_the_malformed_field(self):
        invalid_time = "2026-02-30T14:00:00Z"
        request = changed(baseline(), {"input.quote.observed_at": invalid_time})
        with self.assertRaises(ValueError):
            transport(request)
        encoded = transport(request, malformed_timestamp=True)
        self.assertEqual(encoded["input"]["quote"]["observed_at"], invalid_time)
        self.assertIsInstance(encoded["as_of"], int)
        self.assertIsInstance(encoded["input"]["quote"]["received_at"], int)
        with self.assertRaises(ValueError):
            transport(changed(request, {"as_of": 1}), malformed_timestamp=True)


class ParseRejectionEvidenceTests(unittest.TestCase):
    def test_only_a_clean_parser_failure_counts_as_rejection(self):
        result = subprocess.CompletedProcess([], 2, "", "invalid identifier\n")
        with patch("parity.invoke", return_value=result):
            execute_parse_rejection(["fixture"], {}, diagnostic="invalid identifier", exit_code=2)

    def test_model_rejections_crashes_and_unrelated_failures_do_not_count(self):
        for result in [
            subprocess.CompletedProcess([], 0, '{"status":"REJECTED"}', ""),
            subprocess.CompletedProcess([], 2, '{"status":"REJECTED"}', "invalid identifier"),
            subprocess.CompletedProcess([], -11, "", "invalid identifier"),
            subprocess.CompletedProcess([], 2, "", "missing library"),
            subprocess.CompletedProcess([], 1, "", "invalid identifier"),
        ]:
            with self.subTest(result=result), patch("parity.invoke", return_value=result), self.assertRaises(ValueError):
                execute_parse_rejection(["fixture"], {}, diagnostic="invalid identifier", exit_code=2)

    def test_adapter_rejection_requires_its_exact_typed_parser_signal(self):
        diagnostic = "LEAN formula fixture PARSE_REJECTED: input schema mismatch"
        with patch("parity.invoke", return_value=subprocess.CompletedProcess([], 2, "", diagnostic + "\n")):
            execute_parse_rejection(["adapter"], {}, diagnostic=diagnostic, exit_code=2, exact_diagnostic=True)
        for result in [
            subprocess.CompletedProcess([], 1, "", "LEAN formula fixture BLOCKED: FileNotFoundError\n"),
            subprocess.CompletedProcess([], 1, "", "LEAN formula fixture BLOCKED: TimeoutExpired\n"),
            subprocess.CompletedProcess([], 1, "", "LEAN formula fixture BLOCKED: RuntimeError\n"),
            subprocess.CompletedProcess([], -11, "", diagnostic + "\n"),
            subprocess.CompletedProcess([], 2, "", diagnostic + "\nRuntimeError\n"),
            subprocess.CompletedProcess([], 1, "", diagnostic + "\n"),
            subprocess.CompletedProcess([], 2, '{"status":"REJECTED"}', diagnostic + "\n"),
            subprocess.CompletedProcess([], 2, "\n", diagnostic + "\n"),
        ]:
            with self.subTest(result=result), patch("parity.invoke", return_value=result), self.assertRaises(ValueError):
                execute_parse_rejection(["adapter"], {}, diagnostic=diagnostic, exit_code=2, exact_diagnostic=True)

    def test_transport_exceptions_cannot_supply_rejection_evidence(self):
        for error in [FileNotFoundError("adapter missing"), subprocess.TimeoutExpired("adapter", 30),
                      RuntimeError("wrapped inner signal")]:
            with self.subTest(error=type(error).__name__), patch("parity.invoke", side_effect=error), self.assertRaises(type(error)):
                execute_parse_rejection(["adapter"], {}, diagnostic="PARSE_REJECTED", exit_code=2, exact_diagnostic=True)

    def test_raw_unsigned_cases_retain_exponents_and_signed_zero_at_every_field(self):
        cases = {case[0]: case for case in malformed_wire_fixtures()}
        for path, _ in UNSIGNED_FIELDS:
            for token in ["1e0", "1E+0", "10e-1", "1.0e1", "0e0", "-0", "-0.0", "-0e0"]:
                name = "parse/lexeme/" + path + "/" + token
                original, reference = cases[name][1:3]
                field = path.split(".")[-1]
                self.assertIn('"' + field + '":' + token, original)
                self.assertIn('"' + field + '":' + token, reference)
                # These are legal JSON lexemes whose unsigned typing differs.
                json.loads(original)
                json.loads(reference)

    def test_fixture_inventory_names_are_unique(self):
        exact = fixtures()
        malformed = list(malformed_wire_fixtures())
        self.assertEqual(len(exact), len({case.name for case in exact}))
        self.assertEqual(len(malformed), len({case[0] for case in malformed}))

    def test_surrogate_wire_cases_are_never_normalized_through_python(self):
        cases = [case for case in malformed_wire_fixtures() if case[0].startswith("parse/surrogate/")]
        self.assertEqual(len(cases), 16)
        for name, original, reference, _, _ in cases:
            with self.subTest(name=name):
                self.assertIsInstance(original, str)
                self.assertIsInstance(reference, str)
                self.assertIn(r"\u", original)
                self.assertIn(r"\u", reference)

    def test_accepted_unicode_cases_include_escaped_and_direct_utf8_wire(self):
        cases = [case for case in fixtures() if case.name.startswith("formula/accepted/unicode/")]
        self.assertEqual(len(cases), 2)
        for case in cases:
            wire = json.dumps(case.request, separators=(",", ":"))
            for source, replacement in case.wire_replacements:
                wire = wire.replace(source, replacement)
            self.assertEqual(json.loads(wire), case.request)
            if case.name.endswith("escaped"):
                self.assertIn(r"\uD83D\uDE00", wire)
            else:
                self.assertIn("😀𐀀\U0010ffff", wire)

    def test_wire_fixtures_preserve_duplicates_and_trailing_documents(self):
        wire = {case[0]: case for case in malformed_wire_fixtures() if case[0].startswith("parse/wire/")}
        self.assertEqual(len(wire), 9)
        for name in ["duplicate-root", "duplicate-nested", "duplicate-escaped", "trailing-line", "broken-json"]:
            case = wire["parse/wire/" + name]
            for encoded in case[1:3]:
                with self.subTest(name=name), self.assertRaises(ValueError):
                    strict_json(encoded)


if __name__ == "__main__":
    unittest.main()
