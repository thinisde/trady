"""Offline LEAN replay of frozen point-in-time formula inputs.

This bridge exercises Rust decisions inside the actual engine. LEAN fees,
slippage, fill models and approved data must be configured and pinned by the
deployment-owned engine configuration. It makes no claim of broker equivalence.
"""
from AlgorithmImports import *
from bridge import RustFormulaBridge
import json
import pathlib
from datetime import datetime, timezone


class TradyFormulaReplay(QCAlgorithm):
    def initialize(self):
        settings = json.loads(pathlib.Path("/lab-input/replay.json").read_text())
        if settings["mode"] != "OFFLINE" or settings["data_kind"] != "REAL":
            raise ValueError("approved real offline data required")
        self.settings = settings
        start = datetime.fromisoformat(settings["opens_at"].replace("Z", "+00:00"))
        end = datetime.fromisoformat(settings["closes_at"].replace("Z", "+00:00"))
        self.set_start_date(start.year, start.month, start.day)
        self.set_end_date(end.year, end.month, end.day)
        self.set_cash(float(settings["opening_cash"]))
        self.symbol = self.add_equity(settings["symbol"], Resolution.MINUTE).symbol
        self.bridge = RustFormulaBridge(settings["rust_binary"], settings["rust_sha256"])
        self.inputs = settings["inputs"]
        self.seen = set()
        self.equity = []
        self.closed_trades = 0
        self.entry = None
        self.stop = None
        self.target = None
        self.limits = None
        self.schedule.on(self.date_rules.every_day(self.symbol), self.time_rules.after_market_open(self.symbol, 0), self.capture_opening)
        self.schedule.on(self.date_rules.every_day(self.symbol), self.time_rules.after_market_close(self.symbol, 0), self.capture_equity)

    def on_data(self, data):
        stamp = self.utc_time.replace(tzinfo=timezone.utc).isoformat().replace("+00:00", "Z")
        if not self.equity:
            if stamp != self.settings["opens_at"]:
                raise ValueError("first engine valuation differs from the frozen opening boundary")
            self.equity.append({"at": stamp, "equity": str(self.portfolio.total_portfolio_value)})
        if self.portfolio[self.symbol].quantity < 0:
            raise ValueError("offline sibling-order race created an invalid short holding")
        if self.is_warming_up or self.entry is not None or self.portfolio.invested:
            return
        # Frozen inputs must exist at the actual engine event time. Missing
        # support means cash, never a fabricated signal or future feature.
        request = self.inputs.get(stamp)
        if request is None or stamp in self.seen:
            return
        self.seen.add(stamp)
        if request["as_of"] != stamp or request["input"]["account"]["mode"] != "paper":
            raise ValueError("replay input is not point-in-time/paper scoped")
        if abs(float(request["input"]["account"]["spendable_cash"]["amount"]) - float(self.portfolio.cash)) > 0.000001:
            raise ValueError("frozen account cash differs from actual engine cash")
        self.limits = self.bridge.calculate(request)
        self.entry = self.limit_order(self.symbol, float(self.limits["quantity"]), float(self.limits["buy_limit"]))

    def on_order_event(self, event):
        if event.status != OrderStatus.FILLED:
            return
        if self.entry is not None and event.order_id == self.entry.order_id:
            quantity = float(self.portfolio[self.symbol].quantity)
            if quantity <= 0:
                raise ValueError("entry fill did not create a holding")
            self.stop = self.stop_market_order(self.symbol, -quantity, float(self.limits["stop_trigger"]))
            self.target = self.limit_order(self.symbol, -quantity, float(self.limits["target_sell_limit"]))
        elif self.stop is not None and event.order_id == self.stop.order_id:
            if self.target is not None:
                self.target.cancel("stop filled; offline sibling cancellation")
            self.closed_trades += 1
            self.entry = self.stop = self.target = None
        elif self.target is not None and event.order_id == self.target.order_id:
            if self.stop is not None:
                self.stop.cancel("target filled; offline sibling cancellation")
            self.closed_trades += 1
            self.entry = self.stop = self.target = None

    def capture_equity(self):
        stamp = self.utc_time.replace(tzinfo=timezone.utc).isoformat().replace("+00:00", "Z")
        self.equity.append({"at": stamp, "equity": str(self.portfolio.total_portfolio_value)})

    def capture_opening(self):
        if not self.equity:
            stamp = self.utc_time.replace(tzinfo=timezone.utc).isoformat().replace("+00:00", "Z")
            if stamp != self.settings["opens_at"]:
                raise ValueError("scheduled exchange open differs from the frozen fold")
            self.equity.append({"at": stamp, "equity": str(self.portfolio.total_portfolio_value)})

    def on_end_of_algorithm(self):
        report = {"schema_version": 1, "data_kind": "REAL", "equity": self.equity,
                  "closed_trades": self.closed_trades, "open_holdings": int(self.portfolio.invested),
                  "unfilled_entry": self.entry is not None and not self.portfolio.invested,
                  "broker_equivalence_verified": False}
        pathlib.Path("/lab-output/trady-replay.json").write_text(json.dumps(report))
