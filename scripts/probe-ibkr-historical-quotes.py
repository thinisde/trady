#!/usr/bin/env python3
"""Read-only historical SMART bid/ask controls for a captured T212 demo history.

Run with the official IBKR Python SDK. This does not enable a quote policy or
submit orders. Input pages must match the demo capture manifest's file hashes.
"""
import argparse
from datetime import datetime, timedelta, timezone
from decimal import Decimal, InvalidOperation
import hashlib
import importlib.util
import json
import logging
import os
from pathlib import Path
import threading
import time

from ibapi.client import EClient
from ibapi.common import PROTOBUF_MSG_ID
from ibapi.contract import Contract
from ibapi.message import OUT
from ibapi import get_version_string

spec = importlib.util.spec_from_file_location("live_quote_probe", Path(__file__).with_name("probe-ibkr-smart.py"))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
READ_ONLY = base.READ_ONLY_MESSAGES | {OUT.REQ_HISTORICAL_TICKS}


class HistoricalProbe(base.QuoteProbe):
    def __init__(self):
        super().__init__()
        self.history_ends = {}
        self.history = {}
        self.history_done = set()

    def sendMsg(self, msgId, msg):
        if msgId not in READ_ONLY:
            raise RuntimeError("IBKR_READ_ONLY_MESSAGE_REJECTED")
        return EClient.sendMsg(self, msgId, msg)

    def sendMsgProtoBuf(self, msgId, msg):
        if msgId - PROTOBUF_MSG_ID not in READ_ONLY:
            raise RuntimeError("IBKR_READ_ONLY_PROTOBUF_MESSAGE_REJECTED")
        return EClient.sendMsgProtoBuf(self, msgId, msg)

    def error(self, reqId, errorTime, errorCode, errorString, advancedOrderRejectJson=""):
        super().error(reqId, errorTime, errorCode, errorString, advancedOrderRejectJson)
        if reqId in self.history_ends and errorCode == 10187:
            self.history_ends[reqId].set()

    def historicalTicksBidAsk(self, reqId, ticks, done):
        self.history.setdefault(reqId, []).extend({
            "observed_at_seconds": t.time, "bid": str(t.priceBid), "ask": str(t.priceAsk),
            "bid_size": str(t.sizeBid), "ask_size": str(t.sizeAsk),
            "bid_past_low": t.tickAttribBidAsk.bidPastLow,
            "ask_past_high": t.tickAttribBidAsk.askPastHigh,
        } for t in ticks)
        if done:
            self.history_done.add(reqId)
            self.history_ends[reqId].set()


def read_history(path):
    manifest = json.loads(path.read_text())
    if not (manifest["broker_environment"] == "demo" and manifest["broker_account_id"] == 52211463
            and manifest["base_currency"] == "EUR" and manifest["broker_mutations"] == 0):
        raise ValueError("DEMO_HISTORY_SCOPE_REQUIRED")
    root = path.parent.resolve()
    rows = []
    for page in manifest["pages"]:
        file = (root / page["path"]).resolve()
        if not file.is_relative_to(root) or file.stat().st_size > 12_000_000:
            raise ValueError("INVALID_HISTORY_PAGE_PATH")
        data = file.read_bytes()
        if hashlib.sha256(data).hexdigest() != page["sha256"]:
            raise ValueError("HISTORY_PAGE_HASH_CHANGED")
        record = json.loads(data)
        if record["status"] != "OK":
            raise ValueError("HISTORY_PAGE_UNAVAILABLE")
        rows.extend(record["response"]["items"])
    unique = {}
    for row in rows:
        fill = row.get("fill")
        if fill and fill.get("type") == "TRADE":
            if row["order"].get("side") not in ("BUY", "SELL"):
                raise ValueError("FILL_SIDE_UNKNOWN")
            at = datetime.fromisoformat(fill["filledAt"].replace("Z", "+00:00"))
            if at.tzinfo is None or not Decimal(str(fill["price"])).is_finite() or Decimal(str(fill["price"])) <= 0:
                raise ValueError("INVALID_FILL_PRICE_OR_TIME")
            if fill.get("id") is None:
                raise ValueError("EXACT_FILL_ID_REQUIRED")
            key = str(fill["id"])
            if key in unique and unique[key] != row:
                raise ValueError("CONFLICTING_FILL_HISTORY")
            unique[key] = row
    return manifest, list(unique.values())


def compare(fill, ticks):
    at = datetime.fromisoformat(fill["filled_at"].replace("Z", "+00:00"))
    # Strictly earlier seconds avoid using a quote from after the fill within
    # its timestamp's one-second bin. This remains a retrospective control.
    candidates = []
    for tick in ticks:
        try:
            bid, ask = Decimal(tick["bid"]), Decimal(tick["ask"])
            age = Decimal(str(at.timestamp())) - Decimal(tick["observed_at_seconds"])
            if (bid.is_finite() and ask.is_finite() and 0 < bid <= ask
                    and Decimal(tick["bid_size"]) > 0 and Decimal(tick["ask_size"]) > 0
                    and 0 < age <= 10):
                candidates.append((tick["observed_at_seconds"], tick))
        except (InvalidOperation, TypeError, KeyError):
            continue
    if not candidates:
        return {"status": "NO_PRECEDING_VALID_BOOK_WITHIN_10_SECONDS"}
    tick = max(enumerate(candidates), key=lambda item: (item[1][0], item[0]))[1][1]
    reference = Decimal(tick["ask"] if fill["side"] == "BUY" else tick["bid"])
    price = Decimal(fill["fill_price"])
    adverse = price-reference if fill["side"] == "BUY" else reference-price
    return {"status": "HISTORICAL_BOOK_MATCHED", "reference": "ASK" if fill["side"] == "BUY" else "BID",
            "quote": tick, "quote_age_seconds": str(Decimal(str(at.timestamp()))-Decimal(tick["observed_at_seconds"])),
            "adverse_price_difference_bps": str(adverse/reference*10000),
            "fill_inside_recorded_book": Decimal(tick["bid"]) <= price <= Decimal(tick["ask"]),
            "live_receipt_at_fill_time_proven": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history-manifest", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--ticker", help="Optional exact broker ticker from the captured history")
    parser.add_argument("--eur-exchange", choices=["SMART", "IBIS"], default="SMART",
                        help="Explicit EUR control source; IBIS is Xetra, not SMART")
    args = parser.parse_args()
    logging.disable(logging.CRITICAL)
    manifest, rows = read_history(args.history_manifest)
    fill_count_in_history = len(rows)
    if args.ticker:
        rows = [r for r in rows if r["order"]["ticker"] == args.ticker]
    if len(rows) > 8:
        raise ValueError("REVIEW_EXCEEDS_EIGHT_FILL_QUERY_BUDGET")
    app = HistoricalProbe()
    started = base.utc_now()
    results, contracts = [], {}
    failure = None
    try:
        app.connect("127.0.0.1", 4002, 910051)
        threading.Thread(target=app.run, daemon=True).start()
        if not app.ready.wait(12):
            raise RuntimeError("IBKR_HANDSHAKE_TIMEOUT")
        for n, row in enumerate(rows):
            order, fill = row["order"], row["fill"]
            instrument = order["instrument"]
            isin, currency = instrument["isin"], instrument["currency"]
            key = isin, currency
            if key not in contracts:
                request_id = 100+n
                app.contract_ends[request_id] = threading.Event()
                c = Contract()
                c.secType, c.exchange, c.currency, c.secIdType, c.secId = "STK", "SMART", currency, "ISIN", isin
                app.reqContractDetails(request_id, c)
                ended = app.contract_ends[request_id].wait(10)
                details = app.contracts.get(request_id, []) if ended else []
                matches = [d for d in details if d.contract.currency == currency and d.contract.secType == "STK"
                           and base.public_contract(d)["identifiers"].get("ISIN") == isin]
                contracts[key] = matches
            matches = contracts[key]
            result = {"order_id":str(order["id"]),"fill_id":str(fill["id"]),"broker_ticker":order["ticker"],
                      "isin":isin,"currency":currency,"side":order["side"],"fill_price":str(fill["price"]),
                      "fill_quantity":str(fill["quantity"]),"filled_at":fill["filledAt"],"trading_method":fill["tradingMethod"],
                      "ibkr_contracts":[base.public_contract(d) for d in matches]}
            results.append(result)
            if len(matches) != 1:
                result["comparison"] = {"status":"EXACT_SMART_CONTRACT_UNAVAILABLE_OR_AMBIGUOUS"}
                continue
            c = matches[0].contract
            exchange = args.eur_exchange if currency == "EUR" else "SMART"
            if exchange not in matches[0].validExchanges.split(","):
                result["comparison"] = {"status":"REQUESTED_CONTROL_VENUE_UNAVAILABLE"}
                continue
            c.exchange = exchange
            result["requested_quote_exchange"] = exchange
            request_id = 3000+n
            app.history_ends[request_id] = threading.Event()
            end = datetime.fromisoformat(fill["filledAt"].replace("Z", "+00:00"))+timedelta(seconds=1)
            app.reqHistoricalTicks(request_id,c,"",end.astimezone(timezone.utc).strftime("%Y%m%d %H:%M:%S UTC"),1000,"BID_ASK",0,False,[])
            ended = app.history_ends[request_id].wait(20)
            ticks = app.history.get(request_id, [])
            complete = request_id in app.history_done
            result.update({"history_request_id":request_id,"history_request_completed":ended,"historical_ticks":ticks,
                           "history_response_done":complete,
                           "comparison":compare(result,ticks) if complete else {"status":"HISTORICAL_DATA_UNAVAILABLE_OR_INCOMPLETE"}})
            time.sleep(2.1)
    except Exception as error:
        failure = type(error).__name__
    finally:
        server_version = app.serverVersion()
        app.disconnect()
    value = {"schema_version":1,"classification":"RETROSPECTIVE_HISTORICAL_CONTROL",
             "started_at":started,"completed_at":base.utc_now(),"sdk_version":get_version_string(),
             "source_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"server_version":server_version,
             "history_manifest_file_sha256":hashlib.sha256(args.history_manifest.read_bytes()).hexdigest(),
             "history_complete":manifest["history_complete"],"failure_type":failure,"fills":results,"events":app.events,
             "fill_count_in_history":fill_count_in_history,"ticker_filter":args.ticker,"eur_exchange":args.eur_exchange,
             "broker_mutations":0,"read_only_outgoing_message_allowlist":True,"entry_authorized":False}
    fd = os.open(args.out,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,"w") as f:
        json.dump(value,f,indent=2,allow_nan=False);f.write("\n");f.flush();os.fsync(f.fileno())
    print(json.dumps({"status":"HISTORY_CONTROL_COMPLETE" if failure is None else "HISTORY_CONTROL_FAILED",
                      "fills":len(results),"matched":sum(v["comparison"]["status"]=="HISTORICAL_BOOK_MATCHED" for v in results),
                      "artifact":str(args.out),"broker_mutations":0}))
    return 0 if failure is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
