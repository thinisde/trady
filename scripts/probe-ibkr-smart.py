#!/usr/bin/env python3
"""Bounded, read-only IB Gateway quote probe for the reviewed EUR crosswalk.

Use the official IB Python SDK in an isolated environment. This probe cannot send
order, account mutation, or paid regulatory snapshot requests. It does not accept
credentials: authentication belongs to the already-running IB Gateway.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import logging
import math
import os
from pathlib import Path
import threading
import time

from ibapi.client import EClient
from ibapi.common import PROTOBUF_MSG_ID
from ibapi.contract import Contract
from ibapi.message import OUT
from ibapi.ticktype import TickTypeEnum
from ibapi.wrapper import EWrapper
from ibapi import get_version_string


def utc_now():
    return datetime.now(timezone.utc).isoformat()


READ_ONLY_MESSAGES = {
    OUT.START_API, OUT.REQ_CONTRACT_DATA, OUT.REQ_MKT_DATA,
    OUT.CANCEL_MKT_DATA, OUT.REQ_MARKET_DATA_TYPE, OUT.REQ_TICK_BY_TICK_DATA,
    OUT.CANCEL_TICK_BY_TICK_DATA, OUT.REQ_CURRENT_TIME,
}


class QuoteProbe(EWrapper, EClient):
    def __init__(self):
        EWrapper.__init__(self)
        EClient.__init__(self, self)
        self.ready = threading.Event()
        self.contract_ends = {}
        self.contracts = {}
        self.events = []
        self.lock = threading.Lock()
        self.truncated = False

    def sendMsg(self, msgId, msg):
        if msgId not in READ_ONLY_MESSAGES:
            raise RuntimeError("IBKR_READ_ONLY_MESSAGE_REJECTED")
        return super().sendMsg(msgId, msg)

    def sendMsgProtoBuf(self, msgId, msg):
        if msgId - PROTOBUF_MSG_ID not in READ_ONLY_MESSAGES:
            raise RuntimeError("IBKR_READ_ONLY_PROTOBUF_MESSAGE_REJECTED")
        return super().sendMsgProtoBuf(msgId, msg)

    def record(self, kind, **fields):
        with self.lock:
            if len(self.events) < 4096:
                self.events.append({"kind": kind, "received_at": utc_now(), **fields})
            else:
                self.truncated = True

    def nextValidId(self, orderId):
        # The handshake supplies an ID. Never store, print, or use it for orders.
        self.ready.set()

    def error(self, reqId, errorTime, errorCode, errorString, advancedOrderRejectJson=""):
        # Messages can contain account identifiers. Keep only typed numeric codes.
        self.record("error", request_id=reqId, code=errorCode)

    def currentTime(self, time):
        self.record("server_time", observed_at_seconds=time)

    def connectionClosed(self):
        self.record("connection_closed")

    def contractDetails(self, reqId, contractDetails):
        self.contracts.setdefault(reqId, []).append(contractDetails)

    def contractDetailsEnd(self, reqId):
        self.contract_ends[reqId].set()

    def marketDataType(self, reqId, marketDataType):
        self.record("market_data_type", request_id=reqId, data_type=marketDataType)

    def tickPrice(self, reqId, tickType, price, attrib):
        if math.isfinite(price):
            self.record("price", request_id=reqId, tick_type=TickTypeEnum.toStr(tickType),
                        price=str(price), can_auto_execute=attrib.canAutoExecute,
                        pre_open=attrib.preOpen, past_limit=attrib.pastLimit)

    def tickSize(self, reqId, tickType, size):
        self.record("size", request_id=reqId, tick_type=TickTypeEnum.toStr(tickType), size=str(size))

    def tickString(self, reqId, tickType, value):
        # Exchange codes and last-trade time are data, not independently timed books.
        if tickType in (TickTypeEnum.LAST_TIMESTAMP, TickTypeEnum.BID_EXCH, TickTypeEnum.ASK_EXCH):
            self.record("string", request_id=reqId, tick_type=TickTypeEnum.toStr(tickType), value=value[:128])

    def tickReqParams(self, tickerId, minTick, bboExchange, snapshotPermissions):
        self.record("quote_parameters", request_id=tickerId, min_tick=str(minTick),
                    bbo_exchange=bboExchange, snapshot_permissions=snapshotPermissions)

    def tickByTickBidAsk(self, reqId, time, bidPrice, askPrice, bidSize, askSize, tickAttribBidAsk):
        if math.isfinite(bidPrice) and math.isfinite(askPrice):
            self.record("bid_ask", request_id=reqId, observed_at_seconds=time,
                        bid=str(bidPrice), ask=str(askPrice), bid_size=str(bidSize),
                        ask_size=str(askSize), bid_past_low=tickAttribBidAsk.bidPastLow,
                        ask_past_high=tickAttribBidAsk.askPastHigh)


def public_contract(detail):
    c = detail.contract
    return {"con_id": c.conId, "symbol": c.symbol, "local_symbol": c.localSymbol,
            "security_type": c.secType, "currency": c.currency,
            "exchange": c.exchange, "primary_exchange": c.primaryExchange,
            "identifiers": {p.tag: p.value for p in (detail.secIdList or [])},
            "legal_name": detail.longName, "timezone": detail.timeZoneId,
            "trading_hours": detail.tradingHours, "liquid_hours": detail.liquidHours,
            "valid_exchanges": detail.validExchanges}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--crosswalk", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--port", type=int, default=4002)
    parser.add_argument("--client-id", type=int, default=910049)
    parser.add_argument("--seconds", type=int, default=20)
    args = parser.parse_args()
    if not (1 <= args.port <= 65535 and 1 <= args.client_id <= 2147483647 and 5 <= args.seconds <= 60):
        parser.error("invalid bounded probe arguments")
    logging.disable(logging.CRITICAL)
    raw_crosswalk = args.crosswalk.read_bytes()
    crosswalk = json.loads(raw_crosswalk)
    if (crosswalk.get("review_scope") != "SHADOW_IDENTITY_ONLY"
        or crosswalk.get("broker_environment") != "demo"
        or crosswalk.get("base_currency") != "EUR"
        or crosswalk.get("live_enabled") is not False
        or crosswalk.get("broker_mutations_authorized") is not False
        or crosswalk.get("broker_account_id") != 52211463
        or not 1 <= len(crosswalk.get("members", [])) <= 8):
        parser.error("reviewed EUR shadow crosswalk required")
    now = datetime.now(timezone.utc)
    if not (datetime.fromisoformat(crosswalk["effective_at"].replace("Z", "+00:00")) <= now
            < datetime.fromisoformat(crosswalk["expires_at"].replace("Z", "+00:00"))):
        parser.error("shadow identity review expired or future-dated")
    started = utc_now()
    app = QuoteProbe()
    requested = []
    active = []
    status = "UNAVAILABLE"
    failure = None
    try:
        app.connect("127.0.0.1", args.port, args.client_id)
        worker = threading.Thread(target=app.run, daemon=True)
        worker.start()
        if not app.ready.wait(12):
            raise RuntimeError("IBKR_HANDSHAKE_TIMEOUT")
        app.reqCurrentTime()
        for i, identity in enumerate(crosswalk["members"]):
            request_id = 100 + i
            app.contract_ends[request_id] = threading.Event()
            c = Contract()
            c.secType = "STK"
            c.exchange = "SMART"
            c.currency = "EUR"
            c.secIdType = "ISIN"
            c.secId = identity["isin"]
            app.reqContractDetails(request_id, c)
            complete = app.contract_ends[request_id].wait(10)
            details = app.contracts.get(request_id, []) if complete else []
            matches = [d for d in details if d.contract.currency == "EUR"
                       and d.contract.secType == "STK"
                       and public_contract(d)["identifiers"].get("ISIN") == identity["isin"]]
            record = {"instrument_id": identity["internal_instrument_id"],
                      "broker_ticker": identity["broker_ticker"], "isin": identity["isin"],
                      "contract_resolution_complete": complete,
                      "contracts": [public_contract(d) for d in details],
                      "unique_exact_isin_eur_contract": len(matches) == 1}
            requested.append(record)
            if len(matches) != 1:
                continue
            c = matches[0].contract
            c.exchange = "SMART"
            quote_id, book_id = 1000+i, 2000+i
            record.update({"quote_request_id": quote_id, "book_request_id": book_id})
            app.reqMarketDataType(1)  # LIVE only. Never fall back to delayed/frozen.
            app.reqMktData(quote_id, c, "", False, False, [])
            app.reqTickByTickData(book_id, c, "BidAsk", 0, False)
            active.append((quote_id, book_id))
        time.sleep(args.seconds)
        status = "COMPLETED" if active else "EXACT_CONTRACT_UNAVAILABLE"
    except Exception as error:
        failure = type(error).__name__  # Never print arbitrary SDK/account messages.
        status = "PROBE_ERROR"
    finally:
        server_version = app.serverVersion()
        for quote_id, book_id in active:
            if app.isConnected():
                app.cancelMktData(quote_id)
                app.cancelTickByTickData(book_id)
        app.disconnect()
    with app.lock:
        events = list(app.events)
    result = {"schema_version":1, "provider":"IBKR_TWS_API_SMART", "status":status,
              "sdk_version":get_version_string(), "server_version":server_version,
              "probe_source_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "started_at":started, "completed_at":utc_now(), "failure_type":failure,
              "crosswalk_file_sha256":hashlib.sha256(raw_crosswalk).hexdigest(),
              "requested_market_data_type":1, "regulatory_snapshots":False,
              "read_only_outgoing_message_allowlist":True, "broker_mutations":0,
              "entry_authorized":False, "events_truncated":app.truncated,
              "instruments":requested, "events":events}
    args.out.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(args.out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd,"w") as f:
        json.dump(result, f, indent=2, allow_nan=False)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())
    print(json.dumps({"status":status,"artifact":str(args.out),"events":len(events),"broker_mutations":0}))
    return 0 if status == "COMPLETED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
