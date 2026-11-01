"""Deterministic formula and checked-Decimal boundaries, not empirical evidence.

Every case executes both binaries. Expected outcomes guard against accidental
coverage of a different rejection, and accepted cases compare the entire result.
"""
from copy import deepcopy
from dataclasses import dataclass
import random


# Production unsigned fields shared by width and raw-wire lexeme regressions.
UNSIGNED_FIELDS = [
    ("policy.version", 32), ("policy.confidence_entry_min", 8), ("policy.max_holding_sessions", 8),
    ("parameters.version", 32), ("parameters.max_holding_sessions", 8), ("parameters.atr_lookback", 16),
    ("parameters.bar_seconds", 32), ("input.evidence.atr_lookback", 16),
    ("input.evidence.bar_seconds", 32), ("input.evidence.completed_bars", 32),
    ("input.evidence.base_confidence_score", 8), ("input.evidence.confidence_score", 8),
]


@dataclass(frozen=True)
class Fixture:
    name: str
    request: dict
    status: str | None
    error: str | None = None
    wire_replacements: tuple[tuple[str, str], ...] = ()


@dataclass(frozen=True)
class MalformedFixture:
    name: str
    request: dict
    diagnostic: str


def baseline():
    now = "2026-10-30T14:00:00Z"
    money = lambda amount: {"amount": amount, "currency": "EUR"}
    costs = {"fixed": money("0.1"), "rate": "0.001", "uncertainty": money("0.1"), "fx": None}
    return {
        "kind": "FORMULA",
        "policy": {"version": 1, "entry_cash_fraction": "0.98", "confidence_entry_min": 90,
                   "max_target_fraction": "0.10", "exceptional_targets_enabled": False,
                   "max_initial_loss_fraction": "0.02", "min_reward_to_risk": "1.5",
                   "max_holding_sessions": 5, "leverage": False, "add_to_position": False,
                   "rotation_enabled": False},
        "envelope": {"max_entry_allowance_fraction": "0.005", "min_stop_atr_multiple": "1",
                     "max_stop_atr_multiple": "4", "min_target_net_fraction": "0.01",
                     "max_target_net_fraction": "0.03"},
        "parameters": {"version": 1, "family": "TREND", "entry_allowance_fraction": "0.001",
                       "stop_atr_multiple": "2", "target_net_fraction": "0.03",
                       "max_holding_sessions": 3, "atr_lookback": 14, "bar_seconds": 60},
        "max_account_risk_fraction": "0.01", "as_of": now,
        "input": {
            "account": {"snapshot_id": "account_1", "account_id": "paper_limits_test", "mode": "paper",
                        "spendable_cash": money("1000"), "positions": [], "orders": [], "observed_at": now},
            "market_snapshot_id": "market_1",
            "quote": {"instrument_id": "test_stock", "currency": "EUR", "bid": "99.99", "ask": "100",
                      "observed_at": now, "received_at": now, "tradable": True},
            "entry_range": {"minimum": "99", "maximum": "101", "currency": "EUR"},
            "entry_costs": deepcopy(costs), "exit_costs": deepcopy(costs),
            "rules": {"price_tick": "0.01", "quantity_step": "0.01", "minimum_quantity": "0.01", "currency_unit": "0.01"},
            "evidence": {"snapshot_id": "bars_1", "family": "TREND", "signal_eligible": True,
                         "atr": "0.5", "atr_lookback": 14, "bar_seconds": 60, "completed_bars": 30,
                         "last_completed_at": "2026-10-30T13:59:15Z", "available_at": "2026-10-30T13:59:30Z",
                         "valid_until": "2026-10-30T14:05:00Z", "minimum_stop_distance": "0.01",
                         "noise_distance": "0.1", "base_confidence_score": 100, "confidence_score": 100,
                         "sentiment_entry_allowed": True, "sentiment_target_ceiling": "0.03", "exceptional_verified": False},
            "holding_deadline_at": "2026-11-02T14:00:00Z", "valid_until": "2026-10-30T14:05:00Z"}}


def changed(request, updates):
    result = deepcopy(request)
    for path, value in updates.items():
        pieces = path.split(".")
        parent = result
        for piece in pieces[:-1]:
            parent = parent[int(piece)] if isinstance(parent, list) else parent[piece]
        parent[int(pieces[-1]) if isinstance(parent, list) else pieces[-1]] = deepcopy(value)
    return result


def formula_fixtures():
    base = baseline()
    cases = [Fixture("formula/accepted/fees", base, "ACCEPTED")]
    def add(name, updates, error=None):
        cases.append(Fixture("formula/" + name, changed(base, updates), "REJECTED" if error else "ACCEPTED", error))
    fx = {"from": "USD", "to": "EUR", "rate": "0.9", "observed_at": base["as_of"]}
    fx_updates = {"input.quote.currency": "USD", "input.entry_range.currency": "USD",
                  "input.entry_costs.fx": fx, "input.exit_costs.fx": fx}
    add("accepted/fx", fx_updates)
    add("accepted/reversal", {"parameters.family": "REVERSAL", "input.evidence.family": "REVERSAL"})
    add("accepted/fractional-lots", {"input.rules.quantity_step": "0.025", "input.rules.minimum_quantity": "0.025"})
    add("accepted/no-fees", {"input.entry_costs.fixed.amount": "0", "input.entry_costs.uncertainty.amount": "0",
                             "input.entry_costs.rate": "0", "input.exit_costs.fixed.amount": "0",
                             "input.exit_costs.uncertainty.amount": "0", "input.exit_costs.rate": "0"})
    add("accepted/target-ceiling", {"parameters.target_net_fraction": "0.02", "input.evidence.sentiment_target_ceiling": "0.02"})
    add("accepted/exceptional", {"policy.exceptional_targets_enabled": True, "envelope.max_target_net_fraction": "0.10",
                                 "parameters.target_net_fraction": "0.10", "input.evidence.sentiment_target_ceiling": "0.10",
                                 "input.evidence.exceptional_verified": True})
    add("accepted/quote-one-nanosecond-inside", {"input.quote.observed_at": "2026-10-30T13:59:50.000000001Z"})
    add("accepted/represented-leap-second", {"input.quote.observed_at": "2026-10-30T13:59:60.5Z"})
    add("accepted/utc-offset", {"input.quote.observed_at": "2026-10-30T16:00:00+02:00"})
    add("accepted/subnanosecond-truncation", {"input.quote.observed_at": "2026-10-30T13:59:59.123456789987Z"})
    for name, deadline in [
        ("ordinary-maximum", "+262142-12-31T23:59:59.999999999Z"),
        ("final-leap-second", "+262142-12-31T23:59:60Z"),
        ("final-leap-maximum", "+262142-12-31T23:59:60.999999999Z"),
    ]:
        add("accepted/" + name, {"input.holding_deadline_at": deadline})
    unicode_request = changed(base, {"input.account.snapshot_id": "account_😀𐀀\U0010ffff",
                                     "input.market_snapshot_id": "market_😀",
                                     "input.evidence.snapshot_id": "bars_𐀀"})
    for name, replacements in [
        ("escaped", ((r"\ud83d\ude00", r"\uD83D\uDE00"), ('"snapshot_id"', r'"\u0073napshot_id"'))),
        ("utf8", ((r"\ud83d\ude00", "😀"), (r"\ud800\udc00", "𐀀"), (r"\udbff\udfff", "\U0010ffff"))),
    ]:
        cases.append(Fixture("formula/accepted/unicode/" + name, unicode_request, "ACCEPTED",
                             wire_replacements=replacements))
    for mode in ["paper", "shadow", "live"]:
        add("accepted/account-mode/" + mode, {"input.account.mode": mode})
    add("accepted/maximum-account-id", {"input.account.account_id": "a" * 96})
    add("accepted/maximum-cash-rounding", {"input.account.spendable_cash.amount": "79228162514264337593543950335",
                                         "input.rules.quantity_step": "1", "input.rules.currency_unit": "1"})
    no_charges = {"input.entry_costs.fixed.amount": "0", "input.entry_costs.uncertainty.amount": "0",
                  "input.entry_costs.rate": "0", "input.exit_costs.fixed.amount": "0",
                  "input.exit_costs.uncertainty.amount": "0", "input.exit_costs.rate": "0",
                  "parameters.entry_allowance_fraction": "0"}
    tiny_lot = "0.0000000000000000000000000001"
    def at_price(value):
        return {"input.quote.ask": value, "input.quote.bid": value,
                "input.entry_range.minimum": value, "input.entry_range.maximum": value,
                "input.rules.price_tick": "1"}
    # A rounded quotient can buy a represented lot whose actual cost exceeds
    # the allocation. The expected message prevents an earlier sizing rejection
    # from silently replacing this boundary.
    add("rejected/rounded-acquisition-cost", {
        **no_charges, **at_price("2000000000000000000000000000"),
        "input.account.spendable_cash.amount": "1", "input.rules.quantity_step": tiny_lot,
        "input.rules.minimum_quantity": tiny_lot}, "rounded cost exceeds allocation")
    # A positive modeled loss divided by a huge cost may round to zero.
    add("rejected/loss-fraction-underflow", {
        **no_charges, **at_price("70000000000000000000000000000"),
        "input.account.spendable_cash.amount": "79228162514264337593543950335",
        "input.evidence.atr": "0.5", "input.rules.quantity_step": "1",
        "input.rules.minimum_quantity": "1"}, "invalid initial loss")
    # Quantity * FX can underflow in the target denominator even though the
    # ordered liquidation expression quantity * price * FX is positive.
    tiny_fx = {**fx, "rate": "0.000000000000000000000000001"}
    add("rejected/exit-proceeds-underflow", {
        **no_charges, **at_price("50000000000000000000000000000"),
        "input.quote.currency": "USD", "input.entry_range.currency": "USD",
        "input.entry_costs.fx": tiny_fx, "input.exit_costs.fx": tiny_fx,
        "input.account.spendable_cash.amount": "0.000000000000000000000000006",
        "input.rules.currency_unit": tiny_lot, "input.rules.quantity_step": tiny_lot,
        "input.rules.minimum_quantity": tiny_lot, "input.evidence.atr": "500000000000000000000000000",
        "max_account_risk_fraction": "0.02"}, "exit proceeds precision exhausted")
    add("rejected/target-before-entry-with-different-fx", {
        **no_charges, **at_price("100"), "input.rules.price_tick": "0.01",
        "input.quote.currency": "USD", "input.entry_range.currency": "USD",
        "input.entry_costs.fx": {**fx, "rate": "1"}, "input.exit_costs.fx": {**fx, "rate": "2"},
        "input.evidence.atr": "25.25"}, "formula target ordering or reward/risk failed")
    # One extra tick is deliberately bounded. If that addition disappears at
    # the available precision, a target still below required proceeds rejects.
    add("rejected/target-correction-precision", {
        **no_charges, "input.account.spendable_cash.amount": "52175873720230936603561656733",
        "input.quote.ask": "100.82985192273248137239415310",
        "input.quote.bid": "100.82985192273248137239415310",
        "input.rules.price_tick": "0.000000000000000000000000002",
        "input.rules.quantity_step": "1", "input.rules.minimum_quantity": "1",
        "input.entry_range.minimum": "99", "input.entry_range.maximum": "102"},
        "target price cannot conservatively satisfy net proceeds")
    # At this magnitude the bounded quantity correction rounds subtraction of
    # a 1.6-unit lot to 2 units. Every economic check passes; final grid fails.
    add("rejected/quantity-correction-lost-grid", {
        **no_charges, **at_price("1"), "input.rules.price_tick": "0.01",
        "input.account.spendable_cash.amount": "32000000000000000000000000015",
        "input.rules.quantity_step": "1.6", "input.rules.minimum_quantity": "1.6",
        "input.rules.currency_unit": "8", "input.evidence.atr": "0.01",
        "input.evidence.minimum_stop_distance": "0", "input.evidence.noise_distance": "0"},
        "final formula prices or quantity lost grid precision")
    for path, value, message in [
        ("policy.version", 0, "policy version must be positive"),
        ("policy.entry_cash_fraction", "0", "entry allocation exceeds 98% or is nonpositive"),
        ("policy.entry_cash_fraction", "0.9800000000000000000000000001", "entry allocation exceeds 98% or is nonpositive"),
        ("policy.confidence_entry_min", 89, "entry confidence must be 90 through 100"),
        ("policy.max_target_fraction", "0.1000000000000000000000000001", "target ceiling exceeds 10% or is nonpositive"),
        ("policy.max_initial_loss_fraction", "0.0200000000000000000000000001", "initial loss exceeds 2% or is nonpositive"),
        ("policy.min_reward_to_risk", "1.4999999999999999999999999999", "reward/risk must be at least 1.5"),
        ("policy.max_holding_sessions", 6, "holding limit must be one through five sessions"),
        ("policy.leverage", True, "leverage, additions and rotation are unsupported by the foundation"),
        ("envelope.max_entry_allowance_fraction", "0.0100000000000000000000000001", "invalid owner-approved formula envelope"),
        ("envelope.min_stop_atr_multiple", "0", "invalid owner-approved formula envelope"),
        ("envelope.max_target_net_fraction", "0.11", "invalid owner-approved formula envelope"),
        ("parameters.version", 2, "formula parameters exceed the approved domain"),
        ("parameters.entry_allowance_fraction", "0.0050000000000000000000000001", "formula parameters exceed the approved domain"),
        ("parameters.stop_atr_multiple", "4.000000000000000000000000001", "formula parameters exceed the approved domain"),
        ("parameters.max_holding_sessions", 0, "formula parameters exceed the approved domain"),
        ("parameters.atr_lookback", 253, "formula parameters exceed the approved domain"),
        ("parameters.bar_seconds", 59, "formula parameters exceed the approved domain"),
        ("input.account.snapshot_id", "", "snapshot ID is required"),
        ("input.account.spendable_cash.amount", "-0.0000000000000000000000000001", "negative spendable cash"),
        ("max_account_risk_fraction", "0", "owner account-risk budget is missing or exceeds policy"),
        ("max_account_risk_fraction", "0.0200000000000000000000000001", "owner account-risk budget is missing or exceeds policy"),
        ("input.market_snapshot_id", "", "missing, stale or expired formula inputs"),
        ("input.account.observed_at", "2026-10-30T13:58:59.999999999Z", "missing, stale or expired formula inputs"),
        ("input.valid_until", "2026-10-30T14:00:00Z", "missing, stale or expired formula inputs"),
        ("input.holding_deadline_at", "2026-10-30T14:04:59Z", "missing, stale or expired formula inputs"),
        ("input.evidence.signal_eligible", False, "formula requires current, warmed, causal volatility and an eligible signal"),
        ("input.evidence.atr", "0", "formula requires current, warmed, causal volatility and an eligible signal"),
        ("input.evidence.completed_bars", 14, "formula requires current, warmed, causal volatility and an eligible signal"),
        ("input.evidence.available_at", "2026-10-30T14:00:00.000000001Z", "formula requires current, warmed, causal volatility and an eligible signal"),
        ("input.evidence.last_completed_at", "2026-10-30T13:58:59.999999999Z", "formula requires current, warmed, causal volatility and an eligible signal"),
        ("input.evidence.noise_distance", "-0.1", "formula requires current, warmed, causal volatility and an eligible signal"),
        ("input.evidence.confidence_score", 101, "confidence score exceeds 100"),
        ("input.evidence.base_confidence_score", 99, "formula target or entry violates the confidence/sentiment gate"),
        ("input.evidence.sentiment_entry_allowed", False, "formula target or entry violates the confidence/sentiment gate"),
        ("input.evidence.sentiment_target_ceiling", "0.0300000000000000000000000001", "formula target or entry violates the confidence/sentiment gate"),
        ("input.entry_range.maximum", "99.99", "no marketable formula tick inside range"),
        ("input.quote.tradable", False, "unusable bid/ask"),
        ("input.quote.bid", "101", "unusable bid/ask"),
        ("input.quote.observed_at", "2026-10-30T13:59:49.999999999Z", "stale or future quote"),
        ("input.quote.received_at", "2026-10-30T14:00:00.000000001Z", "stale or future quote"),
        ("input.entry_range.currency", "USD", "invalid entry range or currency"),
        ("input.entry_range.minimum", "-1", "invalid entry range or currency"),
        ("input.entry_range.minimum", "100.5", "ask outside inclusive entry range"),
        ("input.rules.quantity_step", "0", "invalid instrument increments"),
        ("input.rules.minimum_quantity", "0", "invalid instrument increments"),
        ("input.entry_costs.fixed.currency", "USD", "cost currency mismatch"),
        ("input.entry_costs.fixed.amount", "-0.1", "negative costs"),
        ("input.account.spendable_cash.amount", "0.1", "cash cannot cover costs"),
        ("input.rules.minimum_quantity", "100", "below minimum quantity"),
        ("input.evidence.atr", "50.05", "formula stop is nonpositive or inside spread, noise or venue minimum"),
        ("input.evidence.noise_distance", "0.89", "formula stop is nonpositive or inside spread, noise or venue minimum"),
        ("input.evidence.minimum_stop_distance", "0.90", "formula stop is nonpositive or inside spread, noise or venue minimum"),
        ("input.exit_costs.rate", "1", "invalid formula liquidation costs"),
        ("input.exit_costs.fixed.currency", "USD", "invalid formula liquidation costs"),
        ("max_account_risk_fraction", "0.0004", "no feasible positive modeled downside"),
        ("max_account_risk_fraction", "0.0004001", "no supported quantity within account risk"),
        ("input.evidence.atr", "1", "final formula quantity fails allocation or account-risk budget"),
        ("parameters.target_net_fraction", "0.01", "insufficient reward/risk"),
        ("input.quote.observed_at", "2026-10-30T13:59:50Z", "no remaining valid formula input lifetime"),
        ("input.quote.ask", "79228162514264337593543950335", "decimal multiplication overflow"),
    ]:
        add("rejected/" + path + "/" + str(value), {path: value}, message)
    add("rejected/entry-confidence", {"parameters.target_net_fraction": "0.02", "input.evidence.sentiment_target_ceiling": "0.02",
                                      "input.evidence.confidence_score": 89}, "entry confidence below policy floor")
    add("rejected/missing-fx", {"input.quote.currency": "USD", "input.entry_range.currency": "USD"}, "FX observation required")
    add("rejected/stale-exit-fx", {**fx_updates, "input.exit_costs.fx.observed_at": "2026-10-30T13:58:59Z"}, "invalid or stale FX")
    add("rejected/fx-lifetime-boundary", {**fx_updates, "input.exit_costs.fx.observed_at": "2026-10-30T13:59:00Z"},
        "no remaining valid formula input lifetime")
    add("rejected/negative-modeled-downside", {**fx_updates, "input.exit_costs.fx.rate": "2"}, "no feasible positive modeled downside")
    pos = {"instrument_id": "test_stock", "quantity": "1"}
    add("rejected/nonpositive-position", {"input.account.positions": [{**pos, "quantity": "0"}]},
        "position must be positive; shorting is unsupported")
    add("rejected/duplicate-position", {"input.account.positions": [pos, pos]}, "duplicate position")
    order = {"broker_order_id": "broker_1", "client_intent_id": None, "instrument_id": "test_stock", "side": "BUY",
             "quantity": "1", "filled_quantity": "0", "status": "OPEN"}
    for side in ["BUY", "SELL"]:
        for status, filled in [("OPEN", "0"), ("PARTIALLY_FILLED", "0.5"), ("FILLED", "1"),
                               ("CANCELLED", "0"), ("REJECTED", "0"), ("UNKNOWN", "0")]:
            add("accepted/order/" + side + "/" + status, {"input.account.orders": [
                {**order, "side": side, "status": status, "filled_quantity": filled, "client_intent_id": "intent_1"}]})
    optional_order = {key: value for key, value in order.items() if key != "client_intent_id"}
    add("accepted/absent-optional-intent", {"input.account.orders": [optional_order]})
    optional_costs = {key: value for key, value in base["input"]["entry_costs"].items() if key != "fx"}
    add("accepted/absent-optional-fx", {"input.entry_costs": optional_costs, "input.exit_costs": optional_costs})
    for name, orders, error in [
        ("duplicate-order", [order, order], "invalid or duplicate broker order"),
        ("negative-order", [{**order, "quantity": "-1"}], "invalid broker quantity"),
        ("filled-remainder", [{**order, "status": "FILLED"}], "filled order has remaining quantity"),
        ("partial-empty", [{**order, "status": "PARTIALLY_FILLED"}], "partial order must have positive filled and remaining quantities"),
    ]:
        add("rejected/" + name, {"input.account.orders": orders}, error)
    add("rejected/timestamp-overflow", {
        "as_of": "+262142-12-31T23:59:58Z", "input.account.observed_at": "+262142-12-31T23:59:58Z",
        "input.quote.observed_at": "+262142-12-31T23:59:58Z", "input.quote.received_at": "+262142-12-31T23:59:58Z",
        "input.evidence.last_completed_at": "+262142-12-31T23:59:13Z", "input.evidence.available_at": "+262142-12-31T23:59:28Z",
        "input.evidence.valid_until": "+262142-12-31T23:59:59Z", "input.valid_until": "+262142-12-31T23:59:59Z",
        "input.holding_deadline_at": "+262142-12-31T23:59:59Z"}, "formula validity timestamp overflow")
    add("rejected/final-leap-duration-overflow", {
        "as_of": "+262142-12-31T23:59:60.5Z", "input.account.observed_at": "+262142-12-31T23:59:60.5Z",
        "input.quote.observed_at": "+262142-12-31T23:59:60.5Z", "input.quote.received_at": "+262142-12-31T23:59:60.5Z",
        "input.evidence.last_completed_at": "+262142-12-31T23:59:13Z", "input.evidence.available_at": "+262142-12-31T23:59:28Z",
        "input.evidence.valid_until": "+262142-12-31T23:59:60.9Z", "input.valid_until": "+262142-12-31T23:59:60.9Z",
        "input.holding_deadline_at": "+262142-12-31T23:59:60.999999999Z"}, "formula validity timestamp overflow")
    add("rejected/fx-timestamp-overflow", {
        **fx_updates, "as_of": "+262142-12-31T23:59:49Z", "input.account.observed_at": "+262142-12-31T23:58:50Z",
        "input.quote.observed_at": "+262142-12-31T23:59:40Z", "input.quote.received_at": "+262142-12-31T23:59:40Z",
        "input.evidence.last_completed_at": "+262142-12-31T23:58:50Z", "input.evidence.available_at": "+262142-12-31T23:59:19Z",
        "input.evidence.valid_until": "+262142-12-31T23:59:59Z", "input.valid_until": "+262142-12-31T23:59:59Z",
        "input.holding_deadline_at": "+262142-12-31T23:59:59Z", "input.entry_costs.fx.observed_at": "+262142-12-31T23:59:49Z",
        "input.exit_costs.fx.observed_at": "+262142-12-31T23:59:49Z"}, "formula FX validity timestamp overflow")
    return cases


def decimal_fixtures():
    maximum = "79228162514264337593543950335"
    smallest = "0.0000000000000000000000000001"
    cases = []
    def add(name, operation, a, b, error=None):
        cases.append(Fixture("decimal/" + name, {"kind": "DECIMAL", "operation": operation, "a": a, "b": b},
                             "REJECTED" if error else "DECIMAL", error))
    for operation, a, b, error in [
        ("ADD", maximum, "1", "decimal addition overflow"),
        ("SUB", "-" + maximum, "1", "decimal subtraction overflow"),
        ("MUL", maximum, "2", "decimal multiplication overflow"),
        ("DIV", "1", "0", "decimal division overflow or zero denominator"),
        ("DIV", maximum, "0.1", "decimal division overflow or zero denominator"),
        ("REM", "1", "0", "decimal remainder overflow"),
        ("FLOOR", "-1", "0.01", "invalid rounding step or value"),
        ("FLOOR", "1", "0", "invalid rounding step or value"),
        ("FLOOR", "10000000000000000000000000000", "0.6", "rounding precision exhausted"),
        ("CEIL", "10000000000000000000000000000", "0.6", "rounding precision exhausted"),
        ("CEIL", maximum, "2", "decimal addition overflow"),
    ]:
        add("rejected/" + operation + "/" + a + "/" + b, operation, a, b, error)
    for operation, a, b in [
        ("ADD", maximum, "0.4"), ("SUB", maximum, "0.6"),
        ("MUL", smallest, "0.5"), ("MUL", "0.0000000000000000000000000003", "0.5"),
        ("MUL", "-0.0000000000000000000000000003", "0.5"), ("MUL", smallest, smallest),
        ("DIV", "1", "3"), ("DIV", "2", "3"), ("DIV", "-1", "8"),
        ("DIV", maximum, maximum), ("DIV", "1", maximum),
        ("REM", "-5.5", "2"), ("REM", "5.5", "-2"),
        ("FLOOR", "7.123", "0.05"), ("CEIL", "7.123", "0.05"),
        ("MUL", "7922816251426433759354395033.5", "1.0000000000000000000000000001"),
    ]:
        add("accepted/" + operation + "/" + a + "/" + b, operation, a, b)
    return cases


def generated_fixtures():
    rng = random.Random(20261031)
    maximum = 2**96 - 1
    values = ["0", "1", "-1", "0.0000000000000000000000000001", str(maximum), "-" + str(maximum)]
    def decimal(coefficient, scale):
        sign = "-" if coefficient < 0 else ""
        digits = str(abs(coefficient)).zfill(scale + 1)
        return sign + (digits[:-scale] + "." + digits[-scale:] if scale else digits)
    for _ in range(40):
        coefficient = rng.choice([rng.randrange(-maximum, maximum), rng.randrange(-2**32, 2**32),
                                  rng.choice([maximum, maximum - 1, maximum - 5, maximum // 10])])
        values.append(decimal(coefficient, rng.randrange(29)))
    cases = []
    for index in range(400):
        request = {"kind": "DECIMAL", "operation": rng.choice(["ADD", "SUB", "MUL", "DIV", "REM", "FLOOR", "CEIL"]),
                   "a": rng.choice(values), "b": rng.choice(values)}
        cases.append(Fixture(f"generated/decimal/{index}", request, None))
    base = baseline()
    for index in range(128):
        request = changed(base, {
            "input.account.spendable_cash.amount": rng.choice(["0.1", "1", "100", "1000", "1000000000", "1000000000000000000000000000"]),
            "input.evidence.atr": rng.choice(["0.0000000000000000000000000001", "0.001", "0.1", "0.5", "0.9", "1", "50"]),
            "input.rules.quantity_step": rng.choice(["1", "0.1", "0.01", "0.003", "0.0000000000000000000000000001"]),
            "input.entry_costs.rate": rng.choice(["0", "0.001", "0.002", "0.01", "0.0000000000000000000000000001"]),
            "input.exit_costs.rate": rng.choice(["0", "0.001", "0.002", "0.01", "0.9999999999999999999999999999"]),
        })
        cases.append(Fixture(f"generated/formula/{index}", request, None))
    return cases


def fixtures():
    return formula_fixtures() + decimal_fixtures() + generated_fixtures()


def malformed_fixtures():
    base = baseline()
    order = {"broker_order_id": "broker_1", "client_intent_id": "intent_1", "instrument_id": "test_stock",
             "side": "BUY", "quantity": "1", "filled_quantity": "0", "status": "OPEN"}
    base["input"]["account"]["orders"] = [order]
    base["input"]["account"]["positions"] = [{"instrument_id": "test_stock", "quantity": "1"}]
    cases = []

    def add(name, updates, diagnostic):
        cases.append(MalformedFixture("parse/" + name, changed(base, updates), diagnostic))

    for path in ["input.account.account_id", "input.quote.instrument_id", "input.account.positions.0.instrument_id",
                 "input.account.orders.0.instrument_id", "input.account.orders.0.client_intent_id"]:
        for index, value in enumerate(["", "not an id", "../escape", "é", "a" * 97]):
            add(path + "/" + str(index), {path: value}, "invalid identifier")
    for path, values in [
        ("input.account.mode", ["PAPER", "", "demo"]),
        ("input.account.orders.0.side", ["buy", "", "HOLD"]),
        ("input.account.orders.0.status", ["filled", "", "CANCELED", "DONE"]),
        ("parameters.family", ["trend", "UNKNOWN"]),
        ("input.evidence.family", ["trend", "UNKNOWN"]),
    ]:
        for value in values:
            add(path + "/" + value, {path: value}, "unknown enum value")
    for path in ["input.account.spendable_cash.currency", "input.quote.currency", "input.entry_range.currency",
                 "input.entry_costs.fixed.currency", "input.exit_costs.uncertainty.currency"]:
        for value in ["eur", "EU", "EURO", "ÉUR"]:
            add(path + "/" + value, {path: value}, "currency must be three uppercase letters")
    for path, width in UNSIGNED_FIELDS:
        add(path + "/overflow", {path: 2**width}, "integer exceeds production field width")
        add(path + "/negative", {path: -1}, "Natural number expected")
        add(path + "/floating-integer", {path: 1.0}, "Natural number expected")
    # Exercise every nested object, not just the outer request envelope.
    for path in ["", "policy", "envelope", "parameters", "input", "input.account", "input.account.spendable_cash",
                 "input.account.positions.0", "input.account.orders.0", "input.quote", "input.entry_range",
                 "input.entry_costs", "input.entry_costs.fixed", "input.entry_costs.uncertainty",
                 "input.exit_costs", "input.exit_costs.fixed", "input.exit_costs.uncertainty", "input.rules", "input.evidence"]:
        add("unknown-field/" + path, {(path + "." if path else "") + "unexpected": True}, "unknown JSON field")
    for field in ["snapshot_id", "account_id", "mode", "spendable_cash", "positions", "orders", "observed_at"]:
        account = deepcopy(base["input"]["account"])
        del account[field]
        # The precise missing-field path is emitted by the real reference parser.
        add("missing-account/" + field, {"input.account": account}, field)
    for path, value, diagnostic in [
        ("input.account.mode", None, "String expected"),
        ("input.account.orders.0.side", False, "String expected"),
        ("input.account.orders.0.status", 1, "String expected"),
        ("input.account.orders.0.quantity", "NaN", "invalid checked decimal"),
        ("input.account.orders.0.quantity", "79228162514264337593543950336", "invalid checked decimal"),
        ("input.account.orders.0.filled_quantity", {}, "String expected"),
        ("input.account.positions", {}, "array expected"),
        ("input.account.orders", None, "array expected"),
        ("input.quote.tradable", "true", "Bool expected"),
    ]:
        add("invalid-type/" + path + "/" + repr(value), {path: value}, diagnostic)
    for value in ["2026-02-30T14:00:00Z", "2026-13-01T14:00:00Z", "2026-10-30T24:00:00Z",
                  "2026-10-30T14:60:00Z", "2026-10-30T14:00:61Z", "2026-10-30T14:00:00+24:00",
                  "-262143-01-01T00:00:00+00:01", "+262142-12-31T23:59:59-00:01",
                  "+262142-12-31T23:59:60-00:01", "+262142-12-31T23:59:60.999999999-00:01"]:
        add("invalid-calendar/" + value, {"input.quote.observed_at": value}, "Integer expected")
    fx = {"from": "USD", "to": "EUR", "rate": "0.9", "observed_at": base["as_of"]}
    add("fx/unknown-field", {"input.entry_costs.fx": {**fx, "unexpected": True}}, "unknown JSON field")
    add("fx/invalid-currency", {"input.exit_costs.fx": {**fx, "from": "usd"}}, "currency must be three uppercase letters")
    cases.append(MalformedFixture("parse/decimal/unknown-field", {"kind": "DECIMAL", "operation": "ADD", "a": "1", "b": "2", "unexpected": True}, "unknown JSON field"))
    cases.append(MalformedFixture("parse/decimal/unknown-operation", {"kind": "DECIMAL", "operation": "EXP", "a": "1", "b": "2"}, "unknown enum value"))
    return cases
