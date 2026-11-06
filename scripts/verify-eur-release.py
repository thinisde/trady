#!/usr/bin/env python3
"""Assemble and verify an immutable EUR demo release; activation requires exact-hash approval.

Missing or failed evidence is recorded in a candidate bundle, never promoted to
passed. All commands are local. No credentials, orders or services are touched.
"""
import argparse
from datetime import datetime, timezone
from decimal import Decimal
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import subprocess
import tempfile
import zipfile
import grp

ROLES = {"runtime", "binary", "activation_helper", "crosswalk", "ibkr_config", "broker_config", "source_policy", "instrument_rules",
         "quote_compatibility", "quote_observations", "broker_history", "costs", "forward_cost_model", "protocol_acceptance", "paper_day_acceptance", "independent_review"}
TICKERS = {"AIRp_EQ": "NL0000235190", "ASMLa_EQ": "NL0010273215"}
CASES = {"partial_entry_fill", "stop_rejection", "cancellation_race", "stop_fill_cancellation_race", "unknown_submission", "restart_recovery"}
OPTIONAL_ROLES = {"broker_observations", "protocol_trace", "validation_logs", "paper_day_inputs", "probe_sources",
                  "instrument_observations", "yfinance_config", "research_config", "opencode_config", "demo_operation", "shadow_runtime", "quote_stream_archive", "universe_config", "current_bars_config", "current_bars_bridge", "capability_attribution", "capability_evidence"}
RECEIPT_ROOT = Path("/etc/trady/eur-approvals")
RECEIPT_GROUP = "trady-executor"


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def instant(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(result.utcoffset() is not None, "TIMEZONE_REQUIRED")
    return result


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def regular(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 512 * 1024 * 1024,
            "MISSING_SYMLINK_OR_OVERSIZED_ARTIFACT")


def load(path):
    regular(path)
    require(path.stat().st_size <= 24 * 1024 * 1024, "JSON_SIZE_BOUND")
    return json.loads(path.read_text())


def write_new(path, value):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as f:
        json.dump(value, f, sort_keys=True, indent=2)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())


def verify_quote_observation(sample, observations, crosswalk, ibkr):
    """Tie reviewed numbers to a retained callback and exact SMART contract."""
    identity = next(r for r in crosswalk["members"] if r["broker_ticker"] == sample["ticker"])
    binding = ibkr["bindings"][identity["internal_instrument_id"]]
    capture = next(r for r in observations["captures"] if r["name"] == sample["capture"])
    contract = next(r for r in capture["contracts"]["contracts"] if r["con_id"] == binding["con_id"])
    require(observations["provider"] == "IBKR_SMART_LIVE"
            and sample["isin"] == identity["isin"] == contract["identifiers"]["ISIN"]
            and sample["con_id"] == contract["con_id"]
            and sample["primary_exchange"] == contract["primary_exchange"] == binding["primary_exchange"]
            and sample["source_venue"] == contract["exchange"] == "SMART"
            and sample["currency"] == contract["currency"] == "EUR", "RAW_QUOTE_IDENTITY_CONFLICT")
    book = sample["raw_book"]
    contract_index = capture["contracts"]["contracts"].index(contract)
    require(book["kind"] == "bid_ask" and book["request_id"] == 2000 + contract_index
            and book in capture["books"][str(book["request_id"])], "RAW_QUOTE_CALLBACK_MISSING")
    received, filled = instant(book["received_at"]), instant(sample["filled_at"])
    types = [r for r in capture["market_data_types"] if r["request_id"] == 1000 + contract_index
             and instant(r["received_at"]) <= received]
    require(bool(types) and types[-1]["data_type"] == 1
            and instant(sample["quote_at"]).timestamp() == book["observed_at_seconds"]
            and sample["quote_received_at"] == book["received_at"]
            and instant(sample["quote_at"]) <= received <= filled
            and all(Decimal(sample[k]) == Decimal(book[k]) for k in ["bid", "ask", "bid_size", "ask_size"])
            and Decimal(book["bid_size"]) > 0 and Decimal(book["ask_size"]) > 0,
            "RAW_QUOTE_TIME_TYPE_OR_BOOK_CONFLICT")


def verify_cost_observations(value, history, observations):
    initial, final = observations["initial_account"], observations["final_snapshot"]
    account = next(r["response"] for r in final["responses"] if r["path"] == "account/summary")
    require(initial["id"] == account["id"] == 52211463 and initial["currency"] == account["currency"] == "EUR"
            and all(next(r["response"] for r in final["responses"] if r["path"] == path) == [] for path in ["positions", "orders"]),
            "RAW_COST_ACCOUNT_OR_FLAT_STATE_CONFLICT")
    fills = {str(r["fill"]["id"]): r for r in history["items"] if r.get("fill")}
    reported = {str(r["broker_fill_id"]): r for r in value["fills"]}
    require(len(reported) == len(value["fills"]) and reported.keys() == fills.keys(), "RAW_COST_FILL_COVERAGE")
    delta, quantities = Decimal(0), {}
    for identifier, row in fills.items():
        fill, cost = row["fill"], reported[identifier]
        wallet = fill["walletImpact"]
        gross = abs(Decimal(str(fill["quantity"]))) * Decimal(str(fill["price"]))
        net = Decimal(str(wallet["netValue"]))
        buy = row["order"]["side"] == "BUY"
        signed, charge = (-net, net - gross) if buy else (net, gross - net)
        require(row["order"]["side"] in ["BUY", "SELL"] and wallet["currency"] == "EUR"
                and Decimal(str(wallet["fxRate"])) == 1 and isinstance(wallet["taxes"], list)
                and cost["reconciled"] is True and cost["taxes"] == wallet["taxes"]
                and Decimal(cost["gross_eur"]) == gross and Decimal(cost["net_value_eur"]) == net
                and Decimal(cost["signed_cash_eur"]) == signed
                and Decimal(cost["observed_wallet_charge_eur"]) == charge, "RAW_FILL_WALLET_COST_CONFLICT")
        delta += signed
        ticker = row["order"].get("ticker", row["order"].get("instrument", {}).get("ticker"))
        quantity = abs(Decimal(str(fill["quantity"])))
        quantities[ticker] = quantities.get(ticker, Decimal(0)) + (quantity if buy else -quantity)
    require(bool(fills) and all(q == 0 for q in quantities.values()), "RAW_FILL_POSITIONS_NOT_FLAT")
    require(Decimal(str(initial["cash"]["availableToTrade"])) + delta == Decimal(str(account["cash"]["availableToTrade"]))
            and Decimal(value["initial_cash_eur"]) == Decimal(str(initial["cash"]["availableToTrade"]))
            and Decimal(value["observed_ending_cash_eur"]) == Decimal(str(account["cash"]["availableToTrade"]))
            and Decimal(value["observed_signed_cash_delta_eur"]) == delta, "RAW_CASH_RECONCILIATION_CONFLICT")


def native_partial_entry(value):
    """A case flag or reducer fixture cannot supply a native partial order."""
    if isinstance(value, dict):
        if {"id", "quantity", "filledQuantity", "status", "side"} <= value.keys():
            ticker = value.get("ticker", value.get("instrument", {}).get("ticker"))
            if ticker in TICKERS and value["side"] == "BUY" and value["status"] in ["PARTIALLY_FILLED", "CANCELLED"]:
                if 0 < abs(Decimal(str(value["filledQuantity"]))) < abs(Decimal(str(value["quantity"]))):
                    return True
        return any(native_partial_entry(child) for child in value.values())
    if isinstance(value, list):
        return any(native_partial_entry(child) for child in value)
    return False


def scoped_protocol(value, experiment):
    scopes = [s for s in value["scopes"] if s["name"] == experiment]
    require(len(scopes) == 1, "EXACT_PROTOCOL_EXPERIMENT_MISSING")
    scope = scopes[0]
    context = scope["authority"]["context"]
    require(context["broker_account_id"] == 52211463 and context["runtime_mode"] == "DEMO"
            and context["broker_ticker"] in TICKERS and context["isin"] == TICKERS[context["broker_ticker"]]
            and scope["authority"]["initial_broker_snapshot"]["positions"] == []
            and scope["authority"]["initial_broker_snapshot"]["orders"] == [], "PROTOCOL_SCOPE_OR_INITIAL_HOLDING")
    artifacts = scope["artifacts"]
    results = [artifacts[name]["value"] for name in ["result_protect.json", "result_exit.json"] if name in artifacts]
    require(bool(results), "PROTOCOL_RESULT_MISSING")
    responses, confirmations = [], []
    for result in results:
        require(result["classification"] == "REAL_TRADING212_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE"
                and result["state"]["context"] == context, "PROTOCOL_CONTEXT_REBOUND")
        for doc in result["broker_observations"]:
            observation = doc["document"]["observation"]
            require(doc["document"]["intent_id"] == context["intent_id"], "PROTOCOL_OBSERVATION_REBOUND")
            require(observation["evidence"]["provider"] == "TRADING212_DEMO", "NATIVE_BROKER_PROVIDER_REQUIRED")
            responses.extend(observation["evidence"]["responses"])
        for doc in result["protection_confirmations"]:
            confirmation = doc["document"]["confirmation"]
            require(doc["document"]["intent_id"] == context["intent_id"], "PROTECTION_INTENT_REBOUND")
            confirmations.append(confirmation)
            responses.extend(confirmation["evidence"]["responses"])
    final = results[-1]["state"]
    require(final["phase"] == "FLAT" and Decimal(final["holding_quantity"]) == 0, "PROTOCOL_END_NOT_FLAT")
    for suffix in ["/positions", "/orders"]:
        matching = [r for r in responses if r["path"] in [suffix.lstrip("/"), "/api/v0/equity" + suffix] and "response" in r]
        require(bool(matching) and max(matching, key=lambda r: instant(r["received_at"]))["response"] == [],
                "PROTOCOL_RAW_FINAL_NOT_FLAT")
    rows = []
    for response in responses:
        body = response.get("response")
        if isinstance(body, dict) and isinstance(body.get("items"), list):
            rows.extend(body["items"])
    return context, final, responses, confirmations, rows, artifacts


def exact_fill_rows(rows, order_id, context, side, kind):
    exact = [r for r in rows if str(r["order"]["id"]) == str(order_id)]
    require(bool(exact), "EXACT_NATIVE_ORDER_HISTORY_MISSING")
    for row in exact:
        order = row["order"]
        require(order["instrument"]["isin"] == context["isin"]
                and order["instrument"]["ticker"] == context["broker_ticker"]
                and order["side"] == side and order["type"] == kind
                and order["currency"] == "EUR", "EXACT_NATIVE_ORDER_REBOUND")
    fills = {}
    for row in exact:
        if row.get("fill"):
            key = str(row["fill"]["id"])
            require(key not in fills or fills[key] == row["fill"], "CONFLICTING_NATIVE_FILL")
            fills[key] = row["fill"]
    return exact, list(fills.values())


def verify_native_protocol_cases(value, observations):
    cases = {c["case"]: c for c in value["cases"]}
    case = cases["partial_entry_fill"]
    context, state, responses, confirmations, rows, artifacts = scoped_protocol(observations, case["experiment"])
    entry_id = state["entry"]["broker_id"]
    exact, fills = exact_fill_rows(rows, entry_id, context, "BUY", "LIMIT")
    acquired = Decimal(state["entry"]["filled"])
    require(0 < acquired < Decimal(context["quantity"])
            and state["entry"]["status"] == "CANCELLED" and state["entry"]["reconciled_terminal"] is True
            and sum((abs(Decimal(str(f["quantity"]))) for f in fills), Decimal(0)) == acquired,
            "NATIVE_PARTIAL_QUANTITY_NOT_PROVEN")
    stop_id = state["protection"]["broker_id"]
    transport = artifacts["transport.jsonl"]["value"]
    cancels = [r for r in transport if str(r.get("cancel_exact_id")) == str(entry_id) and r.get("accepted") is True]
    posts = [r for r in transport if str((r.get("response") or {}).get("broker_order_id")) == str(stop_id)]
    terminal = [r for r in responses if any(str(x["order"]["id"]) == str(entry_id)
                and x["order"]["status"] == "CANCELLED"
                and abs(Decimal(str(x["order"]["filledQuantity"]))) == acquired
                for x in (r.get("response", {}).get("items", []) if isinstance(r.get("response"), dict) else []))]
    require(bool(cancels) and bool(posts) and bool(terminal)
            and min(instant(r["received_at"]) for r in terminal) <= instant(posts[0]["at"]),
            "PARTIAL_REMAINDER_TERMINAL_BEFORE_PROTECTION_REQUIRED")
    first_fill = min(instant(f["filledAt"]) for f in fills)
    valid = []
    for confirmation in confirmations:
        if str(confirmation["order"]["broker_order_id"]) != str(stop_id):
            continue
        bodies = confirmation["evidence"]["responses"]
        native = [r["response"] for r in bodies if r["path"].endswith("/orders/" + str(stop_id)) and "response" in r]
        holdings = [r["response"] for r in bodies if r["path"].endswith("/positions")]
        if any(o["type"] == "STOP" and o["side"] == "SELL" and o["status"] in ["NEW", "CONFIRMED", "OPEN"]
               and o["instrument"]["isin"] == context["isin"] and o["timeInForce"] == "DAY"
               and abs(Decimal(str(o["quantity"]))) == acquired
               and Decimal(str(o["stopPrice"])) == Decimal(context["initial_stop"]) for o in native) and any(
                   len(h) == 1 and h[0]["instrument"]["isin"] == context["isin"] and Decimal(str(h[0]["quantity"])) == acquired for h in holdings):
            valid.append(confirmation)
    require(any(0 <= (instant(c["observed_at"]) - first_fill).total_seconds() <= 10 for c in valid),
            "ACQUIRED_PARTIAL_NATIVE_PROTECTION_NOT_PROVEN")
    verify_stop_fill_race(cases["stop_fill_cancellation_race"], observations)


def verify_stop_fill_race(case, observations):
    context, state, responses, confirmations, rows, artifacts = scoped_protocol(observations, case["experiment"])
    stop_id = state["protection"]["broker_id"]
    _, entry_fills = exact_fill_rows(rows, state["entry"]["broker_id"], context, "BUY", "LIMIT")
    exact, fills = exact_fill_rows(rows, stop_id, context, "SELL", "STOP")
    require(bool(fills) and any(r["order"]["status"] == "FILLED" for r in exact), "NATIVE_STOP_FILL_NOT_PROVEN")
    faults = artifacts["faults.jsonl"]["value"]
    path = "orders/" + str(stop_id)
    before = [r for r in faults if r.get("phase") == "BEFORE_SEND" and r.get("method") == "DELETE" and r.get("path") == path]
    after = [r for r in faults if r.get("phase") == "RESPONSE" and r.get("method") == "DELETE" and r.get("path") == path]
    require(len(before) == len(after) == 1 and instant(before[0]["at"]) <= instant(after[0]["at"])
            and after[0].get("http_status") in [200, 404]
            and max(instant(f["filledAt"]) for f in fills) <= instant(after[0]["at"]), "STOP_FILL_CANCELLATION_CHAIN_MISSING")
    acquired = sum((abs(Decimal(str(f["quantity"]))) for f in entry_fills), Decimal(0))
    sold = sum((abs(Decimal(str(f["quantity"]))) for f in fills), Decimal(0))
    for leg in state["exits"]:
        _, exit_fills = exact_fill_rows(rows, leg["broker_id"], context, "SELL", "MARKET")
        sold += sum((abs(Decimal(str(f["quantity"]))) for f in exit_fills), Decimal(0))
    require(acquired == sold and acquired > 0 and state["protection"]["reconciled_terminal"] is True,
            "STOP_RACE_REMAINDER_OR_OVERSELL_CONFLICT")


def verify_forward_cost_model(value, model):
    require(value["forward_cost_model"] == model and value["currency"] == "EUR", "FORWARD_COST_MODEL_REBOUND")
    ceiling = Decimal(value["maximum_round_trip_cost_bps"])
    require(0 <= ceiling <= 50, "FORWARD_COST_CEILING_INVALID")
    scopes = value["forward_cost_scopes"]
    require({s["ticker"] for s in scopes} == set(TICKERS), "FORWARD_COST_INSTRUMENT_SCOPE_MISSING")
    for scope in scopes:
        # Costs include a cent of conservative rounding on EACH wallet leg.
        notional = Decimal(scope["minimum_entry_notional_eur"])
        exit_ratio = Decimal(scope["maximum_exit_to_entry_notional_ratio"])
        require(notional > 0 and exit_ratio >= 1, "FORWARD_COST_NOTIONAL_SCOPE_INVALID")
        cost = Decimal("0.02")
        for leg, gross in [("entry", notional), ("exit", notional * exit_ratio)]:
            c = model[leg]
            require(c["fx"] is None and c["fixed"]["currency"] == c["uncertainty"]["currency"] == "EUR",
                    "FORWARD_COST_FX_OR_CURRENCY_UNREVIEWED")
            fixed, rate, reserve = Decimal(c["fixed"]["amount"]), Decimal(c["rate"]), Decimal(c["uncertainty"]["amount"])
            require(min(fixed, rate, reserve) >= 0, "NEGATIVE_FORWARD_COST")
            cost += fixed + gross * rate + reserve
        require(cost / notional * 10000 <= ceiling, "FORWARD_COST_CEILING_EXCEEDED")


def verify_paper_sources(context, files):
    runtime=load(files["runtime"])
    required=runtime.get("shadow",{}).get("current_bars_config")
    frozen=context["runtime"].get("shadow",{}).get("current_bars_config")
    require(required==frozen, "PAPER_CURRENT_CANDLE_SOURCE_REBOUND")
    mapping={"ibkr":"ibkr_config","risk":"forward_cost_model","universe":"universe_config","yfinance":"yfinance_config","research":"research_config","broker":"broker_config","opencode":"opencode_config"}
    if required:
        mapping["current_bars"]="current_bars_config"
        require("current_bars_bridge" in files and digest(files["current_bars_bridge"])==context["provider_configurations"]["current_bars"]["script_sha256"], "PAPER_CURRENT_CANDLE_BRIDGE_REBOUND")
        for key,role in mapping.items():
            require(role in files and context["provider_configurations"][key]==load(files[role]), "PAPER_CURRENT_CANDLE_CONFIGURATION_REBOUND")


def verify_paper_capsule(value, files):
    relative = Path(value["capture_manifest"])
    require(not relative.is_absolute() and ".." not in relative.parts, "PAPER_CAPSULE_PATH_ESCAPE")
    with tempfile.TemporaryDirectory(prefix="eur-release-paper-") as directory:
        root = Path(directory)
        with zipfile.ZipFile(files["paper_day_inputs"]) as archive:
            total = 0
            names = set()
            for entry in archive.infolist():
                name = Path(entry.filename)
                require(not name.is_absolute() and ".." not in name.parts and entry.filename not in names
                        and not stat.S_ISLNK(entry.external_attr >> 16), "UNSAFE_PAPER_ARCHIVE")
                names.add(entry.filename)
                total += entry.file_size
                require(total <= 128 * 1024 * 1024 and entry.file_size <= 24 * 1024 * 1024, "PAPER_CAPSULE_SIZE_BOUND")
            archive.extractall(root)
        manifest_path = root / relative
        capsule = load(manifest_path)
        context_ref = capsule["inputs"]["capture_context"]
        context_path = manifest_path.parent / context_ref["path"]
        require(context_path.resolve().is_relative_to(root.resolve()) and digest(context_path) == context_ref["sha256"],
                "PAPER_CONTEXT_HASH_CONFLICT")
        context = load(context_path)
        verify_paper_sources(context,files)
        require(context["provider_configurations"]["risk"] == load(files["forward_cost_model"]), "PAPER_FORWARD_COST_MODEL_REBOUND")
        require(context["result"]["risk_cost_model"] == load(files["forward_cost_model"]), "PAPER_RISK_PLAN_COSTS_UNBOUND")
        ledger_ref = capsule["inputs"]["paper_order_ledger"]
        ledger_path = manifest_path.parent / ledger_ref["path"]
        require(ledger_path.resolve().is_relative_to(root.resolve()) and digest(ledger_path) == ledger_ref["sha256"],
                "PAPER_LEDGER_HASH_CONFLICT")
        ledger = load(ledger_path)
        scope = next(s for s in load(files["costs"])["forward_cost_scopes"]
                     if s["ticker"] == next(m["broker_ticker"] for m in context["crosswalk"]["members"]
                                           if m["internal_instrument_id"] == ledger["risk_plan"]["instrument_id"]))
        require(Decimal(ledger["risk_plan"]["quantity"]) * Decimal(ledger["entry_quote"]["ask"]) >= Decimal(scope["minimum_entry_notional_eur"])
                and Decimal(ledger["exit_quote"]["bid"]) / Decimal(ledger["entry_quote"]["ask"]) <= Decimal(scope["maximum_exit_to_entry_notional_ratio"]),
                "PAPER_COST_SCOPE_EXCEEDED")
        binary = root / "release-binary"
        shutil.copyfile(files["binary"], binary)
        binary.chmod(0o500)
        result = subprocess.run([str(binary), "replay", "paper-day-check", "--config", str(files["runtime"].resolve()),
                                 "--manifest", str(manifest_path), "--out", str(root / "verified")],
                                env={}, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        summary = load(root / "verified/paper-day-summary.json")
        require(result.returncode == 0 and summary["paper_day_passed"] is True
                and summary["forward_paper_ledger_verified"] is True and summary["provider_calls"] == 0
                and summary["broker_mutations"] == 0, "POSITIVE_REAL_INPUT_PAPER_REPLAY_REQUIRED")


def verify_capability_attribution(files, manifest, now, operation_start, operation_end):
    value=load(files["capability_attribution"])
    require(value["schema_version"]==1 and value["kind"]=="EUR_REVIEWED_CAPABILITY_ATTRIBUTION"
            and value["owner_id"]==manifest["owner_id"] and value["account_id"]==manifest["account_id"]
            and value["broker_account_id"]==52211463 and value["environment"]=="demo" and value["live_enabled"] is False
            and value["passed"] is True and isinstance(value["reviewer"],str) and value["reviewer"].strip()
            and value["reviewer"]!=manifest["builder"] and instant(value["reviewed_at"])<=now<instant(value["valid_until"])
            and operation_end<=instant(value["valid_until"]) and value["evidence_sha256"]==digest(files["capability_evidence"])
            and instant(value["period_start"])==operation_start and instant(value["period_end"])==operation_end,
            "CAPABILITY_REVIEW_OR_EVIDENCE_INVALID")
    # Delegate protocol hash-chain/flatness and exact native quantity checks to
    # the same bundled implementation, with neither credentials nor providers.
    with tempfile.TemporaryDirectory(prefix="eur-capability-proof-") as directory:
        binary=Path(directory)/"binary"
        shutil.copyfile(files["binary"],binary);binary.chmod(0o500)
        result=subprocess.run([str(binary),"eur","capability-attribution-check","--registry",str(files["capability_attribution"].resolve()),"--evidence",str(files["capability_evidence"].resolve())],
            env={},stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        proof=json.loads(result.stdout)
        require(result.returncode==0 and proof["status"]=="REVIEWED_CAPABILITY_ATTRIBUTION_VALID"
                and proof["broker_mutations"]==proof["provider_calls"]==0 and proof["release_accepted"] is False,
                "CAPABILITY_NATIVE_PROOF_INVALID")


def paths_and_manifest(bundle):
    require(not bundle.is_symlink(), "BUNDLE_SYMLINK")
    manifest = load(bundle / "manifest.json")
    require(manifest["schema_version"] == 1 and manifest["kind"] == "EUR_DEMO_RELEASE_CANDIDATE"
            and manifest["broker_account_id"] == 52211463 and manifest["environment"] == "demo"
            and manifest["live_enabled"] is False, "RELEASE_SCOPE_CHANGED")
    files = {}
    for role, item in manifest["artifacts"].items():
        relative = Path(item["path"])
        require(not relative.is_absolute() and ".." not in relative.parts and len(relative.parts) == 2
                and relative.parts[0] == "artifacts", "ARTIFACT_PATH_ESCAPE")
        path = bundle / relative
        regular(path)
        require(not path.parent.is_symlink() and digest(path) == item["sha256"], "ARTIFACT_HASH_CHANGED")
        files[role] = path
    expected = {"manifest.json", *[item["path"] for item in manifest["artifacts"].values()]}
    require({str(p.relative_to(bundle)) for p in bundle.rglob("*") if p.is_file() or p.is_symlink()} == expected,
            "UNLISTED_RELEASE_FILES")
    return manifest, files


def evidence_blockers(manifest, files, now):
    blockers = ["MISSING_" + role.upper() for role in sorted(ROLES - files.keys())]
    if not instant(manifest["created_at"]) <= now < instant(manifest["valid_until"]):
        blockers.append("RELEASE_EXPIRED_OR_FUTURE")
    runtime = load(files["runtime"])
    require(runtime["account_id"] == manifest["account_id"] and runtime["owner_id"] == manifest["owner_id"]
            and runtime["broker"]["environment"] == "demo" and runtime["live_enabled"] is False,
            "RUNTIME_ACCOUNT_OR_OWNER_MISMATCH")
    if runtime["execution_stage"] != "DEMO":
        blockers.append("RUNTIME_IS_NOT_DEMO")
    if runtime["enabled"] is not True:
        blockers.append("RUNTIME_DISABLED")
    policy = runtime["policy"]
    require(((policy["revision"]=="eur_intraday_v1" and 1<=policy["max_daily_entries"]<=2 and policy.get("max_daily_exits") in [1,2])
             or (policy["revision"]=="eur_intraday_30_entries_v2" and 1<=policy["max_daily_entries"]<=30 and policy.get("max_daily_exits")==None))
            and policy["max_open_positions"] == 1 and policy["rotation_enabled"] is False
            and policy["no_overnight"] is True and Decimal(policy["max_initial_loss_fraction"]) <= Decimal("0.02")
            and Decimal(policy["min_reward_to_risk"]) >= Decimal("1.5"), "RUNTIME_LIMITS_EXCEEDED")
    if policy["revision"]=="eur_intraday_30_entries_v2" and not runtime.get("operation_config"):
        blockers.append("BOUNDED_DEMO_OPERATION_REQUIRED")
    if not runtime.get("shadow", {}).get("ibkr_config"):
        blockers.append("IBKR_SMART_LIVE_CONFIGURATION_REQUIRED")
    require(runtime["eligibility"]["quote_max_age_seconds"] <= 10, "QUOTE_FRESHNESS_LIMIT_EXCEEDED")
    if "activation_helper" in files:
        require(digest(files["activation_helper"]) == digest(Path(__file__)), "ACTIVATION_HELPER_CHANGED")
    if "crosswalk" in files:
        crosswalk = load(files["crosswalk"])
        require(crosswalk["broker_account_id"] == 52211463 and crosswalk["broker_environment"] == "demo"
                and {r["broker_ticker"]: r["isin"] for r in crosswalk["members"]} == TICKERS,
                "CROSSWALK_IDENTITIES_CHANGED")
        if not instant(crosswalk["effective_at"]) <= now < instant(crosswalk["expires_at"]):
            blockers.append("IDENTITY_REVIEW_EXPIRED")
    if "ibkr_config" in files:
        ibkr = load(files["ibkr_config"])
        if ibkr.get("enabled") is not True:
            blockers.append("IBKR_SOURCE_DISABLED")
    if "source_policy" in files:
        sources = load(files["source_policy"])
        require(sources["real_time_bid_ask"] == "IBKR_SMART_LIVE" and sources["quote_fallback_allowed"] is False
                and sources["maximum_quote_age_seconds"] <= 10 and sources["execution_broker"] == "TRADING212"
                and set(sources["research_providers"]) == {"YFINANCE", "ALPHA_VANTAGE"}, "OWNER_SOURCE_POLICY_CHANGED")
    if runtime.get("operation_config"):
        require({"demo_operation","shadow_runtime"} <= files.keys(), "DEMO_MONTH_ARTIFACTS_MISSING")
        operation,source = load(files["demo_operation"]),load(files["shadow_runtime"])
        start,end=instant(operation["starts_at"]),instant(operation["ends_at"])
        require(operation["schema_version"]==1 and operation["account_id"]==runtime["account_id"] and operation["owner_id"]==runtime["owner_id"]
                and operation["broker_account_id"]==52211463 and operation["environment"]=="demo" and operation["live_enabled"] is False
                and 0<(end-start).total_seconds()<=31*86400 and end<=instant(manifest["valid_until"])
                and 1<=operation["max_daily_entries"]<=30 and operation["max_daily_entries"]==policy["max_daily_entries"]
                and operation["max_open_positions"]==1 and operation["no_overnight"] is True
                and operation["automatic_live_activation"] is False and operation["automatic_model_promotion"] is False
                and source["execution_stage"]=="SHADOW" and source["live_enabled"] is False and source["account_id"]==runtime["account_id"]
                and source["owner_id"]==runtime["owner_id"] and source["policy"]==runtime["policy"]
                and source["approved_universe_snapshot_id"]==runtime["approved_universe_snapshot_id"], "DEMO_MONTH_SCOPE_OR_SOURCE_INVALID")
        require(source.get("shadow",{}).get("current_bars_config")==runtime.get("shadow",{}).get("current_bars_config"), "DEMO_MONTH_CANDLE_SOURCE_CHANGED")
        registry_path,evidence_path=operation.get("capability_attribution_config"),operation.get("capability_evidence_config")
        require(bool(registry_path)==bool(evidence_path), "CAPABILITY_CONFIGURATION_INCOMPLETE")
        require(policy["revision"]!="eur_intraday_30_entries_v2" or bool(registry_path), "CAPABILITY_ATTRIBUTION_REQUIRED_FOR_THIRTY_ENTRY_MONTH")
        if registry_path:
            require({"capability_attribution","capability_evidence"}<=files.keys(),"CAPABILITY_ARTIFACTS_MISSING")
            try:
                verify_capability_attribution(files, manifest, now, start, end)
            except (ValueError, KeyError, TypeError, subprocess.TimeoutExpired):
                blockers.append("CAPABILITY_ATTRIBUTION_NOT_ACCEPTED")

        require("crosswalk" in files and end <= instant(load(files["crosswalk"])["expires_at"]), "DEMO_MONTH_IDENTITY_END_NOT_COVERED")
        for role in ["instrument_rules", "quote_compatibility", "costs", "forward_cost_model", "protocol_acceptance", "paper_day_acceptance", "independent_review"]:
            if role in files and end > instant(load(files[role])["valid_until"]):
                blockers.append("DEMO_MONTH_" + role.upper() + "_END_NOT_COVERED")
    if runtime.get("shadow",{}).get("current_bars_config"):
        require({"current_bars_config","current_bars_bridge","source_policy"}<=files.keys(), "CURRENT_CANDLE_ARTIFACT_MISSING")
        bars=load(files["current_bars_config"])
        require(bars["config_version"]==1 and bars["enabled"] is True and bars["provider"]=="IBKR_SMART_TRADES"
                and bars["interval_seconds"]==runtime["shadow"].get("bar_interval_seconds",300)==300 and bars["duration_days"]==2
                and load(files["source_policy"]).get("current_candles")=="IBKR_SMART_TRADES"
                and load(files["source_policy"]).get("current_candles_fallback_allowed") is False
                and digest(files["current_bars_bridge"])==bars["script_sha256"], "CURRENT_CANDLE_SCOPE_OR_POLICY_INVALID")
    elif "source_policy" in files:
        require(load(files["source_policy"]).get("current_candles")!="IBKR_SMART_TRADES", "CURRENT_CANDLE_POLICY_NOT_IMPLEMENTED_BY_RUNTIME")
    if "broker_observations" in files and not native_partial_entry(load(files["broker_observations"])):
        blockers.append("NATIVE_PARTIAL_ENTRY_NOT_OBSERVED")
    for role in ["instrument_rules", "quote_compatibility", "costs", "protocol_acceptance", "paper_day_acceptance", "independent_review"]:
        if role not in files:
            continue
        value = load(files[role])
        try:
            require(value["broker_account_id"] == 52211463 and value["environment"] == "demo"
                    and value["passed"] is True and isinstance(value["reviewer"], str)
                    and bool(value["reviewer"].strip()) and value["reviewer"] != manifest["builder"]
                    and instant(value["reviewed_at"]) <= now < instant(value["valid_until"]), "EVIDENCE_SCOPE_OR_REVIEW")
            # Every approval refers to separately hashed observations, not just
            # a top-level passed flag or the synthetic regression-suite result.
            refs = value["observations"]
            require(bool(refs) and all(ref["role"] in files and manifest["artifacts"][ref["role"]]["sha256"] == ref["sha256"]
                    and ref["role"] != role for ref in refs), "EVIDENCE_OBSERVATION_BINDING")
            if role == "instrument_rules":
                rows = value["instruments"]
                require({r["ticker"]: r["isin"] for r in rows} == TICKERS, "RULE_IDENTITIES")
                for row in rows:
                    require(all(Decimal(row[k]) > 0 for k in ["minimum_quantity", "quantity_step", "price_tick"])
                            and isinstance(row["fractional_orders"], bool) and row["price_scope"] == "RELEASE_ORDER_PRICE_RANGE"
                            and row["broker_validation_passed"] is True, "ORDER_RULES_UNPROVEN")
                    require(all(row["binding"][key] == row[key] for key in ["minimum_quantity", "quantity_step", "price_tick", "fractional_orders"])
                            and row["binding"]["broker_instrument_id"] == row["ticker"] and row["binding"]["isin"] == row["isin"], "ORDER_RULE_BINDING_CONFLICT")
                    rules = row["value_rules"]
                    require(row["binding"]["value_rules"] == rules
                            and Decimal(rules["minimum_trade_value"]) > 0
                            and Decimal(rules["minimum_remaining_position_value"]) > 0
                            and 0 < Decimal(rules["price_floor"]) <= Decimal(rules["price_ceiling"])
                            and row["price_tick_semantics"] in ["VALIDATED_APPLICATION_GRID", "BROKER_PUBLISHED_MINIMUM_TICK"],
                            "PRICE_DEPENDENT_VALUE_RULES_UNPROVEN")
            elif role == "quote_compatibility":
                history = load(files["broker_history"])
                rows = history.get("items", history.get("response", {}).get("items", []))
                fills = {str(row["fill"]["id"]): row for row in rows if row.get("fill") and row["fill"].get("id") is not None}
                require(value["source"] == "IBKR_SMART_LIVE" and 0 < Decimal(value["maximum_divergence_bps"]) <= 20,
                        "QUOTE_SOURCE_OR_DIVERGENCE_BOUND")
                require({r["role"] for r in refs} >= {"quote_observations", "broker_history"}, "QUOTE_OBSERVATION_BINDING_MISSING")
                quotes = load(files["quote_observations"])
                crosswalk, ibkr = load(files["crosswalk"]), load(files["ibkr_config"])
                limits = value["instrument_limits"]
                require(len(limits) == len(TICKERS) and {r["ticker"]: r["isin"] for r in limits} == TICKERS,
                        "INSTRUMENT_DIVERGENCE_LIMITS_REQUIRED")
                sample_ids = [str(s["broker_fill_id"]) for s in value["samples"]]
                require(len(sample_ids) == len(set(sample_ids)) and set(sample_ids) == set(fills), "QUOTE_FILL_COVERAGE_INCOMPLETE")
                for ticker, isin in TICKERS.items():
                    limit = Decimal(next(r["maximum_divergence_bps"] for r in limits if r["ticker"] == ticker))
                    require(0 < limit <= Decimal(value["maximum_divergence_bps"]), "INSTRUMENT_DIVERGENCE_LIMIT_INVALID")
                    samples = [s for s in value["samples"] if s["ticker"] == ticker and s["isin"] == isin]
                    require({s["side"] for s in samples} >= {"BUY", "SELL"}, "MISSING_INSTRUMENT_SIDE_SAMPLES")
                    for sample in samples:
                        verify_quote_observation(sample, quotes, crosswalk, ibkr)
                        raw = fills.get(str(sample["broker_fill_id"]))
                        require(raw is not None, "BROKER_FILL_NOT_IN_RAW_HISTORY")
                        order = raw["order"]
                        require(order.get("ticker", order.get("instrument", {}).get("ticker")) == ticker and order["side"] == sample["side"]
                                and Decimal(str(raw["fill"]["price"])) == Decimal(sample["fill_price"])
                                and instant(raw["fill"]["filledAt"]) == instant(sample["filled_at"]), "BROKER_FILL_SAMPLE_REBOUND")
                        age = (instant(sample["filled_at"]) - instant(sample["quote_at"])).total_seconds()
                        require(0 <= age <= 10 and sample["market_data_type"] == 1 and sample["broker_fill_id"], "QUOTE_TIME_OR_LIVE_TYPE")
                        bid, ask, fill = (Decimal(sample[k]) for k in ["bid", "ask", "fill_price"])
                        require(0 < bid <= ask and (ask - bid) / bid * 10000 <= 20, "QUOTE_SPREAD")
                        reference = ask if sample["side"] == "BUY" else bid
                        require(abs(fill - reference) / reference * 10000 <= limit, "QUOTE_DIVERGENCE")
            elif role == "protocol_acceptance":
                require(value["source"] == "TRADING212_DEMO" and value["protection_mode"] == "SEQUENTIAL_NATIVE_STOP"
                        and {c["case"] for c in value["cases"] if c["passed"] is True and c["broker_evidence"] is True} >= CASES,
                        "BROKER_PROTOCOL_COVERAGE_INCOMPLETE")
                require(0 < Decimal(value["maximum_verified_unprotected_seconds"]) <= 10
                        and Decimal(value["maximum_verified_unprotected_seconds"]) <= policy.get("max_unprotected_seconds", 10)
                        and value["exposure_clock"] == "BROKER_FIRST_FILL_TIMESTAMP"
                        and {r["role"] for r in refs} >= {"broker_history", "broker_observations", "protocol_trace"},
                        "BROKER_PROTECTION_LATENCY_NOT_PROVEN")
                require(native_partial_entry(load(files["broker_observations"])), "NATIVE_PARTIAL_ENTRY_NOT_IN_RAW_BROKER_EVIDENCE")
                verify_native_protocol_cases(value, load(files["broker_observations"]))
            elif role == "paper_day_acceptance":
                require(value["source"] == "REAL_INPUT_PAPER_DAY" and value["reconciled_flat"] is True
                        and all(value["stages"][k] is True for k in ["selection", "research", "risk", "protection", "exit", "cost_reconciliation"]),
                        "COMPLETE_PAPER_DAY_NOT_PROVEN")
                require({r["role"] for r in refs} >= {"paper_day_inputs", "forward_cost_model", "costs", "binary"}, "PAPER_CAPSULE_BINDINGS_REQUIRED")
                if runtime.get("shadow",{}).get("current_bars_config"):
                    require({r["role"] for r in refs}>={"current_bars_config","current_bars_bridge","runtime","ibkr_config"},"PAPER_CURRENT_CANDLE_RELEASE_BINDINGS_REQUIRED")
                verify_paper_capsule(value, files)
            elif role == "costs":
                require(value["currency"] == "EUR" and 0 <= Decimal(value["maximum_round_trip_cost_bps"]) <= 50
                        and value["broker_costs_reconciled"] is True and value["all_fill_wallet_fields_present"] is True
                        and value["expected_ending_cash_eur"] == value["observed_ending_cash_eur"]
                        and {r["role"] for r in refs} >= {"broker_history", "broker_observations"}, "EUR_COST_ACCEPTANCE_MISSING")
                verify_cost_observations(value, load(files["broker_history"]), load(files["broker_observations"]))
                require({r["role"] for r in refs} >= {"forward_cost_model"}, "FORWARD_COST_MODEL_BINDING_REQUIRED")
                model = load(files["forward_cost_model"])
                require(instant(model["reviewed_at"]) <= now < instant(model["valid_until"]), "FORWARD_COST_MODEL_EXPIRED")
                verify_forward_cost_model(value, model)
            elif role == "independent_review":
                require(value["decision"] == "ACCEPT" and value["unresolved_blockers"] == [] and value["independent"] is True,
                        "INDEPENDENT_ACCEPTANCE_MISSING")
                require({ref["role"] for ref in refs} >= {"runtime", "binary", "activation_helper", "instrument_rules",
                        "quote_compatibility", "costs", "protocol_acceptance", "paper_day_acceptance"}, "INDEPENDENT_REVIEW_SCOPE_INCOMPLETE")
        except (ValueError, KeyError, TypeError, StopIteration, ArithmeticError, OSError, zipfile.BadZipFile, subprocess.SubprocessError):
            blockers.append(role.upper() + "_NOT_ACCEPTED")
    return blockers


def verify(bundle, approval=None, now=None):
    now = now or datetime.now(timezone.utc)
    manifest, files = paths_and_manifest(bundle)
    blockers = evidence_blockers(manifest, files, now)
    subject = digest(bundle / "manifest.json")
    if approval is None:
        blockers.append("EXACT_HASH_OWNER_APPROVAL_MISSING")
    else:
        owner = load(approval)
        if owner.get("kind") == "EUR_DEMO_OWNER_APPROVAL_RECEIPT":
            verify_receipt_file(approval, subject)
            require(len(owner["original_owner_approval_sha256"]) == 64
                    and all(c in "0123456789abcdef" for c in owner["original_owner_approval_sha256"])
                    and instant(owner["approved_at"]) <= instant(owner["issued_at"]) <= now,
                    "INVALID_DERIVED_OWNER_AUTHORITY")
        else:
            require(approval.stat().st_uid == 0 and stat.S_IMODE(approval.stat().st_mode) & 0o077 == 0,
                    "OWNER_APPROVAL_MUST_BE_ROOT_OWNED_PRIVATE_FILE")
        require(owner["release_sha256"] == subject and owner["decision"] == "APPROVE"
                and owner["owner_id"] == manifest["owner_id"] and owner["broker_account_id"] == 52211463
                and owner["environment"] == "demo" and owner["live_enabled"] is False
                and instant(owner["approved_at"]) <= now < instant(owner["valid_until"]) <= instant(manifest["valid_until"]),
                "OWNER_APPROVAL_HASH_SCOPE_OR_EXPIRY_MISMATCH")
    return {"schema_version": 1, "release_sha256": subject, "activation_authorized": not blockers,
            "blockers": blockers, "broker_mutations": 0, "live_enabled": False}


def verify_receipt_file(path, subject):
    require(path.absolute() == RECEIPT_ROOT / subject / "authority.json", "OWNER_RECEIPT_PATH_NOT_TRUSTED")
    regular(path)
    metadata = path.stat()
    require(metadata.st_uid == 0 and metadata.st_gid == grp.getgrnam(RECEIPT_GROUP).gr_gid
            and stat.S_IMODE(metadata.st_mode) == 0o440, "OWNER_RECEIPT_OWNERSHIP_OR_MODE")
    for parent in path.parents:
        metadata = parent.lstat()
        require(stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == 0 and stat.S_IMODE(metadata.st_mode) & 0o022 == 0,
                "OWNER_RECEIPT_ANCESTOR_NOT_TRUSTED")


def issue_receipt(bundle, approval, out, now=None):
    require(os.geteuid() == 0, "OWNER_RECEIPT_WRITER_MUST_BE_ROOT")
    now = now or datetime.now(timezone.utc)
    original = load(approval)
    require(original.get("kind") != "EUR_DEMO_OWNER_APPROVAL_RECEIPT", "RECEIPT_CANNOT_REISSUE_AUTHORITY")
    result = verify(bundle, approval, now)
    require(result["activation_authorized"], "ACTIVATION_BLOCKED_BY_EVIDENCE")
    manifest, files = paths_and_manifest(bundle)
    subject = result["release_sha256"]
    require(out.absolute() == RECEIPT_ROOT / subject / "authority.json", "OWNER_RECEIPT_PATH_NOT_TRUSTED")
    expiry = min(instant(original["valid_until"]), instant(manifest["valid_until"]), instant(load(files["crosswalk"])["expires_at"]),
                 *[instant(load(files[r])["valid_until"]) for r in ["instrument_rules", "quote_compatibility", "costs", "forward_cost_model", "protocol_acceptance", "paper_day_acceptance", "independent_review"]])
    gid = grp.getgrnam(RECEIPT_GROUP).gr_gid
    for parent in [RECEIPT_ROOT.parent, *RECEIPT_ROOT.parent.parents]:
        metadata = parent.lstat()
        require(stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == 0 and stat.S_IMODE(metadata.st_mode) & 0o022 == 0,
                "OWNER_RECEIPT_ANCESTOR_NOT_TRUSTED")
    for directory in [RECEIPT_ROOT, RECEIPT_ROOT / subject]:
        if not directory.exists():
            directory.mkdir(mode=0o750)
            os.chown(directory, 0, gid)
            directory.chmod(0o750)
        metadata = directory.lstat()
        require(stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == 0 and metadata.st_gid == gid
                and stat.S_IMODE(metadata.st_mode) & 0o050 == 0o050 and stat.S_IMODE(metadata.st_mode) & 0o022 == 0,
                "OWNER_RECEIPT_ANCESTOR_NOT_TRUSTED")
    receipt = {**original, "kind": "EUR_DEMO_OWNER_APPROVAL_RECEIPT", "issued_at": now.isoformat(),
               "valid_until": expiry.isoformat(), "original_owner_approval_sha256": digest(approval),
               "broker_mutations": 0, "live_enabled": False}
    write_new(out, receipt)
    os.chown(out, 0, gid)
    out.chmod(0o440)
    verify_receipt_file(out, subject)
    return {**result, "receipt": str(out), "valid_until": expiry.isoformat()}


def assemble(spec_path, out):
    spec = load(spec_path)
    require(set(spec["artifacts"]) <= ROLES | OPTIONAL_ROLES, "UNKNOWN_ARTIFACT_ROLE")
    out.mkdir(mode=0o700)
    (out / "artifacts").mkdir(mode=0o700)
    artifacts = {}
    for role, source in spec["artifacts"].items():
        source = (spec_path.parent / source).resolve()
        regular(source)
        path = Path("artifacts") / (role + (".json" if source.suffix == ".json" else ".bin"))
        shutil.copyfile(source, out / path)
        (out / path).chmod(0o400)
        artifacts[role] = {"path": str(path), "sha256": digest(out / path)}
    manifest = {"schema_version": 1, "kind": "EUR_DEMO_RELEASE_CANDIDATE", "environment": "demo",
                "broker_account_id": 52211463, "live_enabled": False, "owner_id": spec["owner_id"],
                "account_id": spec["account_id"], "builder": spec["builder"], "created_at": spec["created_at"],
                "valid_until": spec["valid_until"], "artifacts": artifacts}
    write_new(out / "manifest.json", manifest)
    (out / "manifest.json").chmod(0o400)
    return verify(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("assemble")
    build.add_argument("--spec", type=Path, required=True)
    build.add_argument("--out", type=Path, required=True)
    for command in ["verify", "activate"]:
        p = sub.add_parser(command)
        p.add_argument("--bundle", type=Path, required=True)
        p.add_argument("--owner-approval", type=Path, required=command == "activate")
        if command == "activate":
            p.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "assemble":
        result = assemble(args.spec, args.out)
    else:
        result = verify(args.bundle, args.owner_approval)
        if args.command == "activate":
            result = issue_receipt(args.bundle, args.owner_approval, args.out)
    print(json.dumps(result))
    return 0 if args.command == "assemble" or result["activation_authorized"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, TypeError, OSError) as exc:
        print(json.dumps({"activation_authorized": False, "error": str(exc)[:200], "broker_mutations": 0}), file=sys.stderr)
        raise SystemExit(1) from None
