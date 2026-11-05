#!/usr/bin/env python3
"""Bounded, read-only native SMART TRADES candles; never executable quotes."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import logging
import os
from pathlib import Path
import threading
from zoneinfo import ZoneInfo
from ibapi.client import EClient
from ibapi.common import PROTOBUF_MSG_ID
from ibapi.message import OUT
from ibapi import get_version_string


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()


def provider_cutoff(text):
    local, zone = text.rsplit(" ", 1)
    mapping = {"UTC": "UTC", "GMT": "UTC", "MET": "Europe/Berlin", "Europe/Berlin": "Europe/Berlin",
               "Europe/Paris": "Europe/Paris", "Europe/Amsterdam": "Europe/Amsterdam"}
    if zone not in mapping:
        raise ValueError("UNREVIEWED_PROVIDER_COVERAGE_TIMEZONE")
    at = datetime.strptime(local, "%Y%m%d %H:%M:%S").replace(tzinfo=ZoneInfo(mapping[zone]))
    return at.astimezone(timezone.utc).isoformat(), int(at.utcoffset().total_seconds())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quote-config", type=Path, required=True)
    parser.add_argument("--crosswalk", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--client-id", type=int, required=True)
    args = parser.parse_args()
    quote = json.loads(args.quote_config.read_text())
    identities = json.loads(args.crosswalk.read_text())
    if not (quote["enabled"] and quote["authority"] == "SHADOW_QUOTES_ONLY" and quote["base_currency"] == "EUR"
            and quote["sdk_version"] == get_version_string() == "10.50.2" and 1 <= args.client_id <= 2147483647 and args.client_id != quote["client_id"]
            and 1 <= quote["port"] <= 65535 and 1 <= len(identities["members"]) <= 2
            and identities["broker_account_id"] == 52211463 and identities["broker_environment"] == "demo"
            and identities["live_enabled"] is False and identities["broker_mutations_authorized"] is False):
        raise ValueError("INVALID_READ_ONLY_BAR_SCOPE")
    at = datetime.now(timezone.utc)
    if not datetime.fromisoformat(identities["effective_at"].replace("Z", "+00:00")) <= at < datetime.fromisoformat(identities["expires_at"].replace("Z", "+00:00")):
        raise ValueError("EXPECTED_MAPPING_EXPIRED")
    source = Path(quote["script_path"])
    if hashlib.sha256(source.read_bytes()).hexdigest() != quote["script_sha256"]:
        raise ValueError("READ_ONLY_QUOTE_BRIDGE_CHANGED")
    spec = importlib.util.spec_from_file_location("readonly_contracts", source)
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    allowed = {OUT.START_API, OUT.REQ_CONTRACT_DATA, OUT.REQ_CURRENT_TIME, OUT.REQ_HISTORICAL_DATA, OUT.CANCEL_HISTORICAL_DATA}

    class Bars(base.QuoteProbe):
        def __init__(self):
            super().__init__()
            self.ends, self.bars, self.coverage = {}, {}, {}

        def sendMsg(self, msgId, msg):
            if msgId not in allowed:
                raise RuntimeError("IBKR_BAR_READ_ONLY_MESSAGE_REJECTED")
            return EClient.sendMsg(self, msgId, msg)

        def sendMsgProtoBuf(self, msgId, msg):
            if msgId - PROTOBUF_MSG_ID not in allowed:
                raise RuntimeError("IBKR_BAR_READ_ONLY_PROTOBUF_REJECTED")
            return EClient.sendMsgProtoBuf(self, msgId, msg)

        def error(self, reqId, errorTime, errorCode, errorString, advancedOrderRejectJson=""):
            super().error(reqId, errorTime, errorCode, errorString, advancedOrderRejectJson)
            if reqId in self.ends and errorCode < 1000:
                self.ends[reqId].set()

        def historicalData(self, reqId, bar):
            rows = self.bars.setdefault(reqId, [])
            if len(rows) >= 512:
                self.truncated = True
                return
            rows.append({"date": bar.date, "open": str(bar.open), "high": str(bar.high), "low": str(bar.low),
                         "close": str(bar.close), "volume": str(bar.volume), "wap": str(bar.wap), "trade_count": bar.barCount})

        def historicalDataEnd(self, reqId, start, end):
            start_utc, start_offset = provider_cutoff(start)
            end_utc, end_offset = provider_cutoff(end)
            self.coverage[reqId] = {"start": start, "end": end, "start_utc": start_utc, "end_utc": end_utc,
                                   "start_offset_seconds": start_offset, "end_offset_seconds": end_offset}
            self.ends[reqId].set()

    logging.disable(logging.CRITICAL)
    app = Bars()
    started = base.utc_now()
    instruments, failure = {}, None
    try:
        app.connect("127.0.0.1", quote["port"], args.client_id)
        threading.Thread(target=app.run, daemon=True).start()
        if not app.ready.wait(12):
            raise RuntimeError("IBKR_HANDSHAKE_TIMEOUT")
        app.reqCurrentTime()
        for i, identity in enumerate(identities["members"]):
            key = identity["internal_instrument_id"]
            binding = quote["bindings"][key]
            contract = base.Contract()
            contract.conId, contract.secType, contract.currency = binding["con_id"], "STK", "EUR"
            contract.exchange, contract.primaryExchange = "SMART", binding["primary_exchange"]
            rid = 3000 + i
            app.contract_ends[rid] = threading.Event()
            app.reqContractDetails(rid, contract)
            if not app.contract_ends[rid].wait(12) or len(app.contracts.get(rid, [])) != 1:
                raise RuntimeError("EXACT_CONTRACT_UNAVAILABLE")
            public = base.public_contract(app.contracts[rid][0])
            if not (public["con_id"] == binding["con_id"] and public["currency"] == "EUR"
                    and public["primary_exchange"] == binding["primary_exchange"] and public["identifiers"].get("ISIN") == identity["isin"]):
                raise RuntimeError("EXACT_CONTRACT_CHANGED")
            hid = 5000 + i
            app.ends[hid] = threading.Event()
            requested_at = datetime.now(timezone.utc)
            app.reqHistoricalData(hid, contract, requested_at.strftime("%Y%m%d-%H:%M:%S"), "2 D", "5 mins", "TRADES", 1, 2, False, [])
            if not app.ends[hid].wait(20):
                app.cancelHistoricalData(hid)
            instruments[key] = {"contract": public, "requested_at": requested_at.isoformat(), "received_at": base.utc_now(),
                                "historical_end_received": hid in app.coverage, "coverage": app.coverage.get(hid), "raw_bars": app.bars.get(hid, [])}
    except Exception as error:
        failure = type(error).__name__
    finally:
        app.disconnect()
    complete = failure is None and not app.truncated and len(instruments) == len(identities["members"]) and all(v["historical_end_received"] for v in instruments.values())
    result = {"schema_version": 1, "status": "COMPLETED" if complete else "UNAVAILABLE", "provider": "IBKR_SMART_TRADES",
              "requested_at": started, "received_at": base.utc_now(), "sdk_version": get_version_string(),
              "probe_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "quote_config_canonical_sha256": digest(quote),
              "crosswalk_canonical_sha256": digest(identities), "client_id": args.client_id, "interval_seconds": 300, "duration_days": 2,
              "what_to_show": "TRADES", "rth_only": True, "read_only_outgoing_message_allowlist": True, "regulatory_snapshots": False,
              "broker_mutations": 0, "instruments": instruments, "events": app.events, "failure": failure}
    with os.fdopen(os.open(args.out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as output:
        json.dump(result, output, allow_nan=False)
        output.flush()
        os.fsync(output.fileno())
    return 0 if complete else 1


if __name__ == "__main__":
    raise SystemExit(main())
