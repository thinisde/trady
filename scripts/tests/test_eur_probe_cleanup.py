import importlib.util
from datetime import datetime, timezone
import json
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("cleanup", ROOT / "scripts/cleanup-eur-demo-probe.py")
cleanup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleanup)
run_spec = importlib.util.spec_from_file_location("supervisor", ROOT / "scripts/run-eur-demo-capability.py")
supervisor = importlib.util.module_from_spec(run_spec)
run_spec.loader.exec_module(supervisor)


class CleanupTests(unittest.TestCase):
    def test_completed_cleanup_or_exit_is_rejected_before_credentials_and_child_transport(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            (root/'cleanup.jsonl').write_text(json.dumps({'status':'EXACT_ID_DEMO_PROBE_CLEANUP_FLAT'})+'\n')
            with patch.object(cleanup.subprocess,'run') as child:
                with self.assertRaisesRegex(ValueError,'PROBE_SCOPE_ALREADY_CLEANED'):cleanup.cleanup_scope(root)
                child.assert_not_called()
            (root/'cleanup.jsonl').unlink()
            (root/'result_exit.json').write_text(json.dumps({'state':{'phase':'FLAT'}}))
            with self.assertRaisesRegex(ValueError,'PROBE_SCOPE_ALREADY_TERMINAL'):cleanup.cleanup_scope(root)

    def test_cleanup_is_only_an_original_rust_exit_route_and_prior_attempt_cannot_restart(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            context={'broker_account_id':52211463,'runtime_mode':'DEMO','broker_ticker':'AIRp_EQ','instrument_id':'AIR_PA_EQ',
                     'account_id':'paper_cap_nov6_new_air_depth_partial','intent_id':'cap_nov6_new_air_depth_partial'}
            (root/'authority.json').write_text(json.dumps({'experiment_id':'nov6_new','context':context}))
            (root/'transport.jsonl').write_text(json.dumps({'phase':'RESPONSE','method':'POST','path':'orders/limit','http_status':200,'response':{'id':123}})+'\n')
            self.assertEqual(cleanup.cleanup_scope(root),(context,'nov6_new','AIR_PA_EQ','depth_partial'))
            (root/'cleanup-start.json').write_text('{}')
            with self.assertRaisesRegex(ValueError,'PRIOR_CLEANUP_REQUIRES_ORIGINAL_CONTEXT_REVIEW'):cleanup.cleanup_scope(root)

    def test_account_summary_id_never_becomes_an_order_id(self):
        rows = [{"phase": "RESPONSE", "method": "GET", "path": "account/summary", "http_status": 200, "response": {"id": 52211463}},
                {"phase": "RESPONSE", "method": "POST", "path": "orders/limit", "http_status": 200, "response": {"id": 123}},
                {"request": {"side": "SELL"}, "response": {"broker_order_id": "456"}}]
        self.assertEqual(cleanup.acknowledged_ids("transport.jsonl", rows), {"123", "456"})

    def test_unknown_and_server_error_mutations_require_review(self):
        before = {"phase": "BEFORE_SEND", "method": "POST", "path": "orders/market"}
        for status, unresolved in [(200, False), (400, False), (500, True), (408, True), (429, True)]:
            rows = [before, {"phase": "RESPONSE", "method": "POST", "path": "orders/market", "http_status": status}]
            self.assertEqual(cleanup.unresolved_mutation(rows) is not None, unresolved)
        self.assertIsNotNone(cleanup.unresolved_mutation([before, {"phase": "UNKNOWN", "method": "POST", "path": "orders/market"}]))

    def test_unknown_read_does_not_make_a_broker_mutation_ambiguous(self):
        self.assertIsNone(cleanup.unresolved_mutation([
            {"phase": "BEFORE_SEND", "method": "GET", "path": "positions"},
            {"phase": "UNKNOWN", "method": "GET", "path": "positions"},
        ]))

    def test_legacy_ambiguous_mutations_require_review_even_with_known_entry(self):
        known = {"request": {"side": "BUY"}, "response": {"broker_order_id": "123"}}
        for unresolved in [
            {"request": {"side": "SELL"}, "failure": {"outcome_unknown": True}},
            {"cancel_exact_id": "123", "accepted": False, "failure_http_status": None},
            {"cancel_exact_id": "123", "accepted": False, "failure_http_status": 500},
            {"phase": "UNKNOWN", "method": "POST", "path": "orders/stop"},
        ]:
            self.assertIsNotNone(cleanup.unresolved_mutation([known, unresolved]))
        self.assertIsNone(cleanup.unresolved_mutation([known, {"cancel_exact_id": "123", "accepted": False, "failure_http_status": 404}]))

    def test_one_continuation_cannot_claim_two_entries_or_restart_in_new_directory(self):
        now = datetime(2026, 11, 6, 8, 45, tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / "START.json").write_text(json.dumps({"self_disable_confirmed": True,
                "maximum_new_entry_attempts": 1, "maximum_entry_notional_eur": "2500",
                "activation_authorized": False, "live_enabled": False,
                "started_at": "2026-11-06T08:16:00+00:00", "new_experiment_cutoff": "2026-11-06T09:40:00Z"}))
            supervisor.claim_continuation(root, "nov6_first", root / "first", now)
            with self.assertRaises(FileExistsError):
                supervisor.claim_continuation(root, "nov6_second", root / "second", now)
            with self.assertRaises(ValueError):
                supervisor.claim_continuation(root, "nov6_first", root / "third", now.replace(hour=10))

    def test_local_pre_dispatch_failure_does_not_strand_an_exact_known_entry(self):
        rows = [{"phase": "BEFORE_SEND", "method": "POST", "path": "orders/stop"},
                {"phase": "RESPONSE", "method": "POST", "path": "orders/stop", "definitive_no_effect": True}]
        self.assertIsNone(cleanup.unresolved_mutation(rows))

    def test_new_manual_watch_cannot_reopen_morning_run_or_claim_two_attempts(self):
        now=datetime(2026,11,6,12,0,tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            start={'kind':'OWNER_AUTHORIZED_MANUAL_EVIDENCE_AFTER_30_ENTRY_POLICY_CHANGE', 'started_at':now.isoformat(),
                   'new_experiment_cutoff':'2026-11-06T12:15:00Z','maximum_new_entry_attempts':1,'maximum_entry_notional_eur':'2500',
                   'maximum_acquired_holdings':1,'activation_authorized':False,'live_enabled':False,'scheduled_task_used':False}
            (root/'START.json').write_text(json.dumps(start))
            supervisor.claim_manual(root,'nov6_manual',root/'probe',now)
            with self.assertRaises(FileExistsError):supervisor.claim_manual(root,'nov6_second',root/'second',now)
            start['kind']='SCHEDULED_CONTINUATION'
            (root/'START.json').write_text(json.dumps(start))
            with self.assertRaisesRegex(ValueError,'MANUAL_BOUNDED_AUTHORITY'):supervisor.claim_manual(root,'nov6_old',root/'third',now)

    def test_cancel_ack_or_definite_not_found_clears_its_marker(self):
        before = {"phase": "BEFORE_SEND", "method": "DELETE", "path": "orders/123"}
        self.assertIsNone(cleanup.unresolved_mutation([before, {"phase": "RESPONSE", "method": "DELETE", "path": "orders/123", "accepted": True}]))
        self.assertIsNone(cleanup.unresolved_mutation([before, {"phase": "RESPONSE", "method": "DELETE", "path": "orders/123", "http_status": 404}]))

    def test_98_percent_cash_probe_needs_its_own_authority_and_cannot_repeat(self):
        now = datetime(2026,11,6,15,50,tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            start={'kind':'OWNER_AUTHORIZED_MANUAL_EVIDENCE_AFTER_30_ENTRY_POLICY_CHANGE',
                   'started_at':now.isoformat(),'new_experiment_cutoff':'2026-11-06T16:00:00Z',
                   'maximum_new_entry_attempts':1,'maximum_entry_notional_eur':'2500',
                   'maximum_acquired_holdings':1,'activation_authorized':False,'live_enabled':False,'scheduled_task_used':False}
            (root/'START.json').write_text(json.dumps(start))
            with self.assertRaisesRegex(ValueError,'MANUAL_BOUNDED_AUTHORITY'):
                supervisor.claim_manual(root,'nov6_cash',root/'probe',now,True)
            start.update(kind='OWNER_AUTHORIZED_MANUAL_EVIDENCE_98_PERCENT_BALANCE',maximum_entry_cash_fraction='0.98')
            (root/'START.json').write_text(json.dumps(start))
            with self.assertRaisesRegex(ValueError,'MANUAL_BOUNDED_AUTHORITY'):
                supervisor.claim_manual(root,'nov6_cash',root/'probe',now)
            supervisor.claim_manual(root,'nov6_cash',root/'probe',now,True)
            with self.assertRaises(FileExistsError):
                supervisor.claim_manual(root,'nov6_other',root/'other',now,True)


if __name__ == "__main__":
    unittest.main()
