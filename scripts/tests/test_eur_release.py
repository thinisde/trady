import hashlib
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import zipfile
import grp
import pwd
import subprocess
from datetime import datetime, timedelta, timezone

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("eur_release", ROOT / "scripts/verify-eur-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_positive_paper_must_use_the_actual_release_current_candle_source_and_bridge(self):
        runtime=json.loads((self.path/"runtime.json").read_text())
        runtime["shadow"]["current_bars_config"]="/opt/trady/config/current-bars.json"
        (self.path/"runtime.json").write_text(json.dumps(runtime))
        context={"runtime":copy.deepcopy(runtime),"provider_configurations":{}}
        files={"runtime":self.path/"runtime.json"}
        bridge=self.path/"bridge.py";bridge.write_text('# SYNTHETIC_SCOPE_TEST_NEVER_RELEASE_EVIDENCE\n');files["current_bars_bridge"]=bridge
        mapping={"ibkr":"ibkr_config","risk":"forward_cost_model","universe":"universe_config","yfinance":"yfinance_config","research":"research_config","broker":"broker_config","opencode":"opencode_config","current_bars":"current_bars_config"}
        for key,role in mapping.items():
            data={"fixture":key}
            if key=="current_bars":data["script_sha256"]=release.digest(bridge)
            file=self.path/(role+'.json');file.write_text(json.dumps(data));files[role]=file;context["provider_configurations"][key]=data
        release.verify_paper_sources(context,files)
        downgraded=copy.deepcopy(context);downgraded["runtime"]["shadow"].pop("current_bars_config")
        with self.assertRaisesRegex(ValueError,"PAPER_CURRENT_CANDLE_SOURCE_REBOUND"):release.verify_paper_sources(downgraded,files)
        changed=copy.deepcopy(context);changed["provider_configurations"]["current_bars"]["script_sha256"]="a"*64
        with self.assertRaisesRegex(ValueError,"PAPER_CURRENT_CANDLE_BRIDGE_REBOUND"):release.verify_paper_sources(changed,files)
        changed=copy.deepcopy(context);changed["provider_configurations"]["ibkr"]={"different":"contract"}
        with self.assertRaisesRegex(ValueError,"PAPER_CURRENT_CANDLE_CONFIGURATION_REBOUND"):release.verify_paper_sources(changed,files)

    @unittest.skipUnless(os.geteuid() == 0, "requires root and the isolated executor identity")
    def test_executor_can_read_a_root_readonly_receipt_but_cannot_edit_authority(self):
        try:
            identity=pwd.getpwnam("trady-executor")
        except KeyError:
            self.skipTest("executor identity unavailable")
        with tempfile.TemporaryDirectory(prefix="eur-receipt-permission-fixture-",dir="/var/lib") as directory:
            root=Path(directory);root.chmod(0o750);os.chown(root,0,identity.pw_gid)
            receipt=root/"synthetic-authority.json";original=root/"synthetic-private-original.json"
            receipt.write_text('{"fixture":"SYNTHETIC_PERMISSION_TEST_NEVER_ACTIVATION_AUTHORITY"}')
            original.write_text(receipt.read_text());original.chmod(0o600)
            os.chown(receipt,0,identity.pw_gid);receipt.chmod(0o440)
            check="""import os,sys
from pathlib import Path
receipt,original=map(Path,sys.argv[1:])
assert 'SYNTHETIC_PERMISSION_TEST' in receipt.read_text()
assert not os.access(receipt,os.W_OK)
assert not os.access(receipt.parent,os.W_OK)
try: original.read_bytes()
except PermissionError: pass
else: raise AssertionError('executor read private owner authority')
try: receipt.write_text('changed')
except PermissionError: pass
else: raise AssertionError('executor edited authority')
"""
            subprocess.run(["runuser","-u","trady-executor","--","python3","-c",check,str(receipt),str(original)],check=True,capture_output=True)

    @unittest.skipUnless(os.geteuid() == 0, "trusted-root receipt tests")
    def test_receipt_is_fixed_path_readonly_and_cannot_override_missing_evidence(self):
        bundle, _ = self.build()
        subject=release.digest(bundle / "manifest.json")
        with tempfile.TemporaryDirectory(prefix="eur-authority-fixture-", dir="/var/lib") as directory:
            root=Path(directory);original_root,original_group=release.RECEIPT_ROOT,release.RECEIPT_GROUP
            self.addCleanup(setattr,release,"RECEIPT_ROOT",original_root);self.addCleanup(setattr,release,"RECEIPT_GROUP",original_group)
            release.RECEIPT_ROOT=root;release.RECEIPT_GROUP=grp.getgrgid(os.getgid()).gr_name
            parent=root/subject;parent.mkdir();receipt=parent/"authority.json"
            authority={"kind":"EUR_DEMO_OWNER_APPROVAL_RECEIPT","release_sha256":subject,"decision":"APPROVE","owner_id":self.spec["owner_id"],
                "broker_account_id":52211463,"environment":"demo","live_enabled":False,"approved_at":self.now.isoformat(),"issued_at":self.now.isoformat(),
                "valid_until":self.spec["valid_until"],"original_owner_approval_sha256":"a"*64,"fixture":"SYNTHETIC_PERMISSION_TEST_NEVER_REAL_APPROVAL"}
            receipt.write_text(json.dumps(authority));receipt.chmod(0o440)
            result=release.verify(bundle,receipt,self.now)
            self.assertFalse(result["activation_authorized"])
            self.assertNotIn("EXACT_HASH_OWNER_APPROVAL_MISSING",result["blockers"])
            receipt.chmod(0o640)
            with self.assertRaisesRegex(ValueError,"OWNER_RECEIPT_OWNERSHIP_OR_MODE"):release.verify(bundle,receipt,self.now)
            receipt.chmod(0o440);parent.chmod(0o770)
            with self.assertRaisesRegex(ValueError,"OWNER_RECEIPT_ANCESTOR_NOT_TRUSTED"):release.verify(bundle,receipt,self.now)
            parent.chmod(0o700)
            with self.assertRaisesRegex(ValueError,"OWNER_RECEIPT_PATH_NOT_TRUSTED"):release.verify_receipt_file(root/"wrong.json",subject)

    @unittest.skipUnless(os.geteuid() == 0, "trusted-root receipt tests")
    def test_receipt_issuer_refuses_a_blocked_candidate_without_writing(self):
        bundle,_=self.build();owner=self.path/"synthetic-owner.json"
        owner.write_text(json.dumps({"release_sha256":release.digest(bundle/"manifest.json"),"decision":"APPROVE","owner_id":self.spec["owner_id"],
            "broker_account_id":52211463,"environment":"demo","live_enabled":False,"approved_at":self.now.isoformat(),"valid_until":self.spec["valid_until"]}))
        owner.chmod(0o600);out=self.path/"must-not-exist.json"
        with self.assertRaisesRegex(ValueError,"ACTIVATION_BLOCKED_BY_EVIDENCE"):release.issue_receipt(bundle,owner,out,self.now)
        self.assertFalse(out.exists())

    def test_forward_reserves_need_a_bound_notional_scope_and_nonzero_ceiling(self):
        money = lambda v: {"amount": v, "currency": "EUR"}
        leg = {"fixed": money("0"), "rate": "0.0005", "uncertainty": money("0.01"), "fx": None}
        model = {"entry": leg, "exit": leg}
        value = {"currency": "EUR", "forward_cost_model": model, "maximum_round_trip_cost_bps": "15",
                 "forward_cost_scopes": [{"ticker": t, "minimum_entry_notional_eur": "500",
                                          "maximum_exit_to_entry_notional_ratio": "1.05"} for t in release.TICKERS]}
        release.verify_forward_cost_model(value, model)
        value["maximum_round_trip_cost_bps"] = "0"
        with self.assertRaisesRegex(ValueError, "FORWARD_COST_CEILING_EXCEEDED"):
            release.verify_forward_cost_model(value, model)
        value["maximum_round_trip_cost_bps"] = "15"
        value["forward_cost_scopes"][0]["minimum_entry_notional_eur"] = "1"
        with self.assertRaisesRegex(ValueError, "FORWARD_COST_CEILING_EXCEEDED"):
            release.verify_forward_cost_model(value, model)

    def test_paper_stage_flags_cannot_substitute_for_a_positive_capsule(self):
        archive = self.path / "paper.zip"
        with zipfile.ZipFile(archive, "w") as z:
            z.writestr("capture/manifest.json", json.dumps({"inputs": {}}))
        with self.assertRaises(KeyError):
            release.verify_paper_capsule({"capture_manifest": "capture/manifest.json", "stages": {"selection": True}},
                                         {"paper_day_inputs": archive})

    def test_arbitrary_partial_dict_cannot_supply_a_scoped_native_chain(self):
        with self.assertRaisesRegex(ValueError, "EXACT_PROTOCOL_EXPERIMENT_MISSING"):
            release.verify_native_protocol_cases({"cases": [{"case": "partial_entry_fill", "experiment": "made-up"}]},
                {"scopes": [], "unrelated": {"id": 1, "ticker": "AIRp_EQ", "quantity": 2, "filledQuantity": 1,
                                            "side": "BUY", "status": "PARTIALLY_FILLED"}})

    def test_partial_case_needs_native_order_fields_and_real_acquired_remainder(self):
        self.assertFalse(release.native_partial_entry({"case": "partial_entry_fill", "passed": True, "broker_evidence": True}))
        self.assertFalse(release.native_partial_entry({"leg": "ENTRY", "quantity": "2", "filled": "1", "status": "PARTIALLY_FILLED"}))
        order = {"id": 1, "ticker": "AIRp_EQ", "quantity": 2, "filledQuantity": 1, "side": "BUY", "status": "PARTIALLY_FILLED"}
        self.assertTrue(release.native_partial_entry({"response": [order]}))
        order["filledQuantity"] = 2
        self.assertFalse(release.native_partial_entry(order))

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)
        self.now = datetime.now(timezone.utc)
        runtime = json.loads((ROOT / "config/eur-shadow.example.json").read_text())
        runtime["execution_stage"] = "DEMO"
        runtime["enabled"] = True
        (self.path / "runtime.json").write_text(json.dumps(runtime))
        (self.path / "history.json").write_text('{"items":[],"nextPagePath":null}')
        self.spec = {"owner_id": runtime["owner_id"], "account_id": runtime["account_id"], "builder": "implementation_agent",
                     "created_at": self.now.isoformat(), "valid_until": (self.now + timedelta(hours=1)).isoformat(),
                     "artifacts": {"runtime": "runtime.json", "binary": "history.json", "broker_history": "history.json",
                                   "activation_helper": str(ROOT / "scripts/verify-eur-release.py")}}

    def build(self):
        source = self.path / "spec.json"
        source.write_text(json.dumps(self.spec))
        bundle = self.path / "bundle"
        result = release.assemble(source, bundle)
        return bundle, result

    def test_missing_evidence_is_a_blocked_candidate(self):
        _, result = self.build()
        self.assertFalse(result["activation_authorized"])
        self.assertIn("MISSING_PROTOCOL_ACCEPTANCE", result["blockers"])
        self.assertIn("EXACT_HASH_OWNER_APPROVAL_MISSING", result["blockers"])
        self.assertEqual(len(result["release_sha256"]), 64)

    def test_tampering_and_unlisted_files_fail_closed(self):
        bundle, _ = self.build()
        path = bundle / "artifacts/broker_history.json"
        path.chmod(0o600)
        path.write_text("{}")
        with self.assertRaisesRegex(ValueError, "ARTIFACT_HASH_CHANGED"):
            release.verify(bundle)

    def test_unlisted_file_is_not_in_the_approval_subject(self):
        bundle, _ = self.build()
        (bundle / "unlisted.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "UNLISTED_RELEASE_FILES"):
            release.verify(bundle)

    def test_synthetic_protocol_is_not_broker_acceptance(self):
        evidence = {"broker_account_id": 52211463, "environment": "demo", "passed": True,
                    "reviewer": "different_reviewer", "reviewed_at": self.now.isoformat(),
                    "valid_until": (self.now + timedelta(hours=1)).isoformat(),
                    "observations": [{"role": "broker_history", "sha256": hashlib.sha256((self.path / "history.json").read_bytes()).hexdigest()}],
                    "source": "SYNTHETIC_PROTOCOL_FIXTURE", "protection_mode": "SEQUENTIAL_NATIVE_STOP",
                    "cases": [{"case": c, "passed": True, "broker_evidence": True} for c in release.CASES]}
        (self.path / "protocol.json").write_text(json.dumps(evidence))
        self.spec["artifacts"]["protocol_acceptance"] = "protocol.json"
        _, result = self.build()
        self.assertIn("PROTOCOL_ACCEPTANCE_NOT_ACCEPTED", result["blockers"])

    def test_quote_must_match_retained_live_callback_received_before_fill(self):
        module_spec = importlib.util.spec_from_file_location("eur_analysis", ROOT / "scripts/analyze-eur-demo-evidence.py")
        analysis = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(analysis)
        contract = {"con_id": 29612256, "identifiers": {"ISIN": "NL0000235190"}, "primary_exchange": "SBF", "currency": "EUR", "exchange": "SMART"}
        event = {"kind": "bid_ask", "request_id": 2000, "observed_at_seconds": 1793869201,
                 "received_at": "2026-11-05T09:00:01.1Z", "bid": "199.95", "ask": "200", "bid_size": "1", "ask_size": "1"}
        capture = {"name": "test", "contracts": {"contracts": [contract]}, "books": {2000: [event]},
                   "market_data_types": [{"request_id": 1000, "data_type": 1, "received_at": "2026-11-05T09:00:00Z"}]}
        row = {"order": {"id": 1, "ticker": "AIRp_EQ", "instrument": {"ticker": "AIRp_EQ"}, "side": "BUY"},
               "fill": {"id": 2, "filledAt": "2026-11-05T09:00:02Z", "quantity": 1, "price": 200}}
        sample = analysis.quote_match(row, [capture])
        observations = json.loads(json.dumps({"provider": "IBKR_SMART_LIVE", "captures": [capture]}))
        crosswalk = {"members": [{"broker_ticker": "AIRp_EQ", "isin": "NL0000235190", "internal_instrument_id": "AIR_PA_EQ"}]}
        ibkr = {"bindings": {"AIR_PA_EQ": {"con_id": 29612256, "primary_exchange": "SBF"}}}
        release.verify_quote_observation(sample, observations, crosswalk, ibkr)
        sample["ask"] = "199"
        with self.assertRaisesRegex(ValueError, "RAW_QUOTE_TIME_TYPE_OR_BOOK_CONFLICT"):
            release.verify_quote_observation(sample, observations, crosswalk, ibkr)
        sample["ask"] = "200"
        sample["filled_at"] = "2026-11-05T09:00:01Z"
        with self.assertRaisesRegex(ValueError, "RAW_QUOTE_TIME_TYPE_OR_BOOK_CONFLICT"):
            release.verify_quote_observation(sample, observations, crosswalk, ibkr)

    def test_cost_review_cannot_replace_raw_wallet_and_cash_arithmetic(self):
        def row(identifier, side, net):
            return {"order": {"id": identifier, "ticker": "AIRp_EQ", "side": side},
                    "fill": {"id": identifier + 100, "quantity": 1, "price": 200,
                             "walletImpact": {"currency": "EUR", "fxRate": 1, "netValue": net, "taxes": []}}}
        history = {"items": [row(1, "BUY", 200.1), row(2, "SELL", 199.9)]}
        account = {"id": 52211463, "currency": "EUR", "cash": {"availableToTrade": 1000}}
        final = copy.deepcopy(account)
        final["cash"]["availableToTrade"] = 999.8
        observations = {"initial_account": account, "final_snapshot": {"responses": [
            {"path": "account/summary", "response": final}, {"path": "positions", "response": []}, {"path": "orders", "response": []}]}}
        value = {"initial_cash_eur": "1000", "observed_ending_cash_eur": "999.8", "observed_signed_cash_delta_eur": "-0.2",
                 "fills": [{"broker_fill_id": "101", "reconciled": True, "taxes": [], "gross_eur": "200", "net_value_eur": "200.1", "signed_cash_eur": "-200.1", "observed_wallet_charge_eur": "0.1"},
                           {"broker_fill_id": "102", "reconciled": True, "taxes": [], "gross_eur": "200", "net_value_eur": "199.9", "signed_cash_eur": "199.9", "observed_wallet_charge_eur": "0.1"}]}
        release.verify_cost_observations(value, history, observations)
        tampered = copy.deepcopy(value)
        tampered["fills"][0]["observed_wallet_charge_eur"] = "0"
        with self.assertRaisesRegex(ValueError, "RAW_FILL_WALLET_COST_CONFLICT"):
            release.verify_cost_observations(tampered, history, observations)
        tampered = copy.deepcopy(observations)
        tampered["final_snapshot"]["responses"][0]["response"]["cash"]["availableToTrade"] = 1000
        with self.assertRaisesRegex(ValueError, "RAW_CASH_RECONCILIATION_CONFLICT"):
            release.verify_cost_observations(value, history, tampered)

    @unittest.skipUnless(os.geteuid() == 0, "owner approval validation requires root-owned files")
    def test_approval_cannot_be_rebound_to_a_different_bundle(self):
        bundle, _ = self.build()
        approval = self.path / "approval.json"
        approval.write_text(json.dumps({"release_sha256": "0" * 64, "decision": "APPROVE", "owner_id": self.spec["owner_id"],
                                       "broker_account_id": 52211463, "environment": "demo", "live_enabled": False,
                                       "approved_at": self.now.isoformat(), "valid_until": self.spec["valid_until"]}))
        approval.chmod(0o600)
        with self.assertRaisesRegex(ValueError, "OWNER_APPROVAL_HASH_SCOPE_OR_EXPIRY_MISMATCH"):
            release.verify(bundle, approval)


if __name__ == "__main__":
    unittest.main()
