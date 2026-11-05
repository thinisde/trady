#!/usr/bin/env python3
"""Bounded read-only SMART stream for supervised EUR broker evidence.

Each provider-timed book is synced before the atomic latest snapshot changes.
No broker credentials or IBKR mutation messages are accepted.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import logging
import os
from pathlib import Path
import signal
import threading
import time

ROOT = Path(__file__).resolve().parents[1]


def write_new(path, value):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as f:
        json.dump(value, f, allow_nan=False)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--crosswalk", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--seconds", type=int, default=600)
    p.add_argument("--client-id", type=int, default=910055)
    args = p.parse_args()
    if not 10 <= args.seconds <= 900 or not 1 <= args.client_id <= 2147483647:
        raise ValueError("INVALID_BOUNDED_CAPTURE")
    config = json.loads(args.config.read_text())
    identities = json.loads(args.crosswalk.read_text())
    if config["enabled"] is not True or config["base_currency"] != "EUR" or identities["broker_account_id"] != 52211463 or identities["live_enabled"]:
        raise ValueError("SOURCE_OR_ACCOUNT_SCOPE_CHANGED")
    current = datetime.now(timezone.utc)
    if not datetime.fromisoformat(identities["effective_at"].replace("Z", "+00:00")) <= current < datetime.fromisoformat(identities["expires_at"].replace("Z", "+00:00")):
        raise ValueError("IDENTITY_REVIEW_EXPIRED")
    script = Path(config["script_path"])
    if hashlib.sha256(script.read_bytes()).hexdigest() != config["script_sha256"]:
        raise ValueError("READ_ONLY_PROBE_CHANGED")
    spec = importlib.util.spec_from_file_location("readonly_ibkr", script)
    probe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(probe)
    logging.disable(logging.CRITICAL)
    args.out.mkdir(mode=0o700)
    stopped = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stopped.set())
    signal.signal(signal.SIGINT, lambda *_: stopped.set())

    class Stream(probe.QuoteProbe):
        def __init__(self):
            super().__init__()
            self.books = {}
            self.types = {}
            self.minimum_ticks = {}
            self.bindings = {}
            self.count = 0

        def record(self, kind, **fields):
            with self.lock:
                self.count += 1
                if self.count > 100000:
                    stopped.set()
                    return
                event = {"kind": kind, "received_at": probe.utc_now(), **fields}
                fd = os.open(args.out / "events.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
                with os.fdopen(fd, "a") as f:
                    f.write(json.dumps(event, allow_nan=False) + "\n")
                    f.flush()
                    if kind in ["bid_ask", "market_data_type", "quote_parameters"]:
                        os.fsync(f.fileno())
                request = fields.get("request_id")
                if kind == "market_data_type":
                    self.types[request] = fields["data_type"]
                elif kind == "quote_parameters":
                    self.minimum_ticks[request] = fields["min_tick"]
                elif kind == "bid_ask":
                    self.books[request] = event
                if kind in ["bid_ask", "market_data_type", "quote_parameters"]:
                    members = {}
                    for book_id, binding in self.bindings.items():
                        if book_id in self.books:
                            members[binding["instrument_id"]] = {**binding, **self.books[book_id],
                                "market_data_type": self.types.get(binding["quote_request_id"]),
                                "reference_min_tick": self.minimum_ticks.get(binding["quote_request_id"])}
                    temporary = args.out / "latest.new"
                    if temporary.exists():
                        temporary.unlink()
                    write_new(temporary, {"schema_version": 1, "provider": "IBKR_SMART_LIVE", "broker_mutations": 0,
                        "written_at": probe.utc_now(), "instruments": members})
                    os.replace(temporary, args.out / "latest.json")

    app = Stream()
    active, contracts = [], []
    started = probe.utc_now()
    try:
        app.connect("127.0.0.1", config["port"], args.client_id)
        worker = threading.Thread(target=app.run, daemon=True)
        worker.start()
        if not app.ready.wait(12):
            raise ValueError("IBKR_HANDSHAKE_UNAVAILABLE")
        for i, identity in enumerate(identities["members"]):
            rid = 100 + i
            app.contract_ends[rid] = threading.Event()
            contract = probe.Contract()
            contract.secType, contract.exchange, contract.currency = "STK", "SMART", "EUR"
            contract.secIdType, contract.secId = "ISIN", identity["isin"]
            app.reqContractDetails(rid, contract)
            if not app.contract_ends[rid].wait(10):
                raise ValueError("CONTRACT_IDENTITY_UNAVAILABLE")
            matches = [d for d in app.contracts.get(rid, []) if probe.public_contract(d)["identifiers"].get("ISIN") == identity["isin"] and d.contract.currency == "EUR"]
            expected = config["bindings"][identity["internal_instrument_id"]]
            if len(matches) != 1 or matches[0].contract.conId != expected["con_id"] or matches[0].contract.primaryExchange != expected["primary_exchange"]:
                raise ValueError("EXACT_EUR_CONTRACT_CHANGED")
            public = probe.public_contract(matches[0])
            contracts.append(public)
            contract = matches[0].contract
            contract.exchange = "SMART"
            qid, bid = 1000 + i, 2000 + i
            with app.lock:
                app.bindings[bid] = {"instrument_id": identity["internal_instrument_id"], "broker_ticker": identity["broker_ticker"],
                    "isin": identity["isin"], "con_id": contract.conId, "primary_exchange": contract.primaryExchange,
                    "quote_request_id": qid, "book_request_id": bid, "currency": "EUR", "source_venue": "SMART"}
            app.reqMarketDataType(1)
            app.reqMktData(qid, contract, "", False, False, [])
            app.reqTickByTickData(bid, contract, "BidAsk", 0, False)
            active.append((qid, bid))
        write_new(args.out / "contracts.json", {"started_at": started, "contracts": contracts, "read_only_outgoing_message_allowlist": True})
        stopped.wait(args.seconds)
    finally:
        for qid, bid in active:
            if app.isConnected():
                app.cancelMktData(qid)
                app.cancelTickByTickData(bid)
        app.disconnect()
        write_new(args.out / "summary.json", {"started_at": started, "completed_at": probe.utc_now(), "events": app.count,
            "broker_mutations": 0, "read_only_outgoing_message_allowlist": True})


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, TypeError):
        raise SystemExit("Read-only EUR evidence stream stopped; inspect its scoped artifacts.") from None
