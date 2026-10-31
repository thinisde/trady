"""Causality, purged partitions, reconciled accounting and prospective coverage."""
from datetime import timedelta
from fractions import Fraction
from bisect import bisect_right
from .common import decimal_string, integer, no_gate_assertions, number, ratio, require, sha, timestamp, unique, valid_hash


def calendar_index(bundle, cutoff):
    sessions = {key: dict(value) for key, value in unique(bundle["calendar"]["sessions"], "session_id").items()}
    ordered = sorted(sessions.values(), key=lambda s: timestamp(s["opens_at"]))
    previous = None
    for ordinal, row in enumerate(ordered):
        row["ordinal"] = ordinal
        # The approved exchange calendar, including its future sessions, is the
        # authority. Never infer weekdays or extend its coverage from seconds.
        row["following_session_ids"] = tuple(s["session_id"] for s in ordered[ordinal:ordinal + 5])
        start, end = timestamp(row["opens_at"]), timestamp(row["closes_at"])
        require(start < end and (previous is None or start > previous), "duplicate/overlapping exchange sessions")
        require(row["phase"] in ("TRAIN", "OUTER", "SHADOW") and isinstance(row["regime"], str), "calendar phase/regime missing")
        if row["phase"] != "SHADOW":
            require(end <= cutoff, "future historical session")
        previous = end
    outer = [s for s in ordered if s["phase"] == "OUTER"]
    require(len(outer) >= 2 and len({s["fold_id"] for s in outer}) >= 2, "sealed outer folds missing")
    fold_ids = list(dict.fromkeys(s["fold_id"] for s in outer))
    folds = [[i for i, s in enumerate(outer) if s["fold_id"] == identity] for identity in fold_ids]
    require(all(f == list(range(f[0], f[-1] + 1)) for f in folds), "interleaved outer folds")
    require(len({len(f) for f in folds}) == 1, "unequal exchange-session fold lengths")
    first, last = ordered.index(outer[0]), ordered.index(outer[-1])
    require(all(s["phase"] == "OUTER" for s in ordered[first:last + 1]), "omitted sessions inside stitched outer period")
    return sessions, outer, folds


def holding_span(calendar, entry_session_id, start, end, limit):
    """Validate a calendar holding horizon; the entry session is session one.

    Even an early exit needs coverage through its declared expiry, so a missing
    future calendar cannot make a longer declared horizon appear valid.
    """
    integer(limit, 1, 5)
    require(entry_session_id in calendar, "holding entry exchange session missing")
    entry = calendar[entry_session_id]
    require(timestamp(entry["opens_at"]) <= start < timestamp(entry["closes_at"]),
            "holding entry is outside its exchange session")
    future = entry["following_session_ids"]
    require(len(future) >= limit, "insufficient future exchange calendar coverage for holding horizon")
    deadline = timestamp(calendar[future[limit - 1]]["closes_at"])
    require(start <= end <= deadline, "outcome/trade exceeds calendar holding-session horizon")
    for count, identity in enumerate(future[:limit], 1):
        session = calendar[identity]
        if timestamp(session["opens_at"]) <= end <= timestamp(session["closes_at"]):
            return count
    require(False, "outcome/trade end is outside covered exchange sessions")


def validate_fits(bundle, sessions, outer, policy):
    samples = unique(bundle["samples"], "sample_id")
    fits = unique(bundle["fits"], "fit_id")
    embargo = integer(policy["embargo_seconds"], 1, 10 * 365 * 86400)
    horizon = integer(policy["max_label_horizon_seconds"], 1, 365 * 86400)
    latency = integer(policy["publication_latency_seconds"], 0, 365 * 86400)
    require(embargo >= horizon + latency, "embargo shorter than horizon plus publication delay")
    first_by_fold = {}
    for session in outer:
        first_by_fold.setdefault(session["fold_id"], timestamp(session["opens_at"]))
    for fit in fits.values():
        require(valid_hash(fit["model_hash"]) and valid_hash(fit["transform_hash"]), "unidentified model/preprocessing")
        fitted_at = timestamp(fit["fitted_at"])
        intervals = []
        used_ids = set()
        for role in ("train", "calibration", "development"):
            part = fit[role]
            start, end = timestamp(part["opens_at"]), timestamp(part["closes_at"])
            require(start < end and part["sample_ids"], "training/calibration/development partition missing")
            intervals.append((start, end))
            for identity in part["sample_ids"]:
                require(identity in samples and identity not in used_ids, "sample reused across fit partitions")
                used_ids.add(identity)
                sample = samples[identity]
                require(sample["session_id"] in sessions and sessions[sample["session_id"]]["phase"] == "TRAIN",
                        "fit sample is not in the declared training calendar")
                fs, fe = timestamp(sample["feature_start_at"]), timestamp(sample["feature_end_at"])
                available, recorded = timestamp(sample["available_at"]), timestamp(sample["recorded_at"])
                ls, le = timestamp(sample["label_start_at"]), timestamp(sample["label_end_at"])
                la, lr = timestamp(sample["label_available_at"]), timestamp(sample["label_recorded_at"])
                require(fs <= fe <= available <= recorded <= ls <= le <= la <= lr <= fitted_at,
                        "future/backfilled sample or unavailable label used by fit")
                session = sessions[sample["session_id"]]
                require(timestamp(session["opens_at"]) <= ls <= timestamp(session["closes_at"])
                        and valid_hash(sample["label_source_hash"]), "sample decision/calendar or label provenance differs")
                require(start <= ls and le <= end and la <= end + timedelta(seconds=latency), "unpurged label crossing partition")
                require((le - ls).total_seconds() <= horizon and (la - le).total_seconds() <= latency,
                        "label exceeds frozen horizon/availability budget")
                holding_span(sessions, sample["session_id"], ls, le, policy["max_label_horizon_sessions"])
        for (_, previous_end), (next_start, _) in zip(intervals, intervals[1:]):
            require(previous_end + timedelta(seconds=embargo) <= next_start, "unpurged adjacent fit partitions")
        for fold in fit["outer_fold_ids"]:
            require(fold in first_by_fold and intervals[-1][1] + timedelta(seconds=embargo) <= first_by_fold[fold]
                    and fitted_at <= first_by_fold[fold], "fit or development data crosses sealed boundary")
    return fits, samples


def trade_session_index(sessions):
    ordered = sorted((timestamp(row["opens_at"]), identity) for identity, row in sessions.items())
    return [start for start, _ in ordered], [identity for _, identity in ordered]


def trade_session_at(sessions, index, instant, event):
    starts, identities = index
    position = bisect_right(starts, instant) - 1
    require(position >= 0, "trade " + event + " exchange session missing")
    identity = identities[position]
    close = timestamp(sessions[identity]["closes_at"])
    require(instant < close if event == "entry" else instant <= close,
            "trade " + event + " is outside covered exchange sessions")
    return identity


def trade_lifecycle(trade, sessions, cutoff, index):
    """Facts shared by OPEN/CLOSED actual and shadow trade records.

    Entry uses [open, close); an actual exit may equal the approved close.
    Recording can occur after hours. An OPEN position has no realized exit.
    No holding-age limit or hypothetical close is invented for it.
    """
    require({"state", "entry_at", "recorded_at", "source_hash"} <= set(trade), "missing trade lifecycle fields")
    require(trade["state"] in ("OPEN", "CLOSED"), "invalid trade state")
    require(valid_hash(trade["source_hash"]), "trade provenance missing")
    entry, recorded = timestamp(trade["entry_at"]), timestamp(trade["recorded_at"])
    require(entry <= recorded <= cutoff, "trade entry/recording chronology exceeds cutoff")
    entry_session = trade_session_at(sessions, index, entry, "entry")
    if trade["state"] == "OPEN":
        require(trade.get("exit_at") is None, "OPEN trade has a realized exit")
        return entry_session, None
    require(trade.get("exit_at") is not None, "CLOSED trade exit missing")
    exit_at = timestamp(trade["exit_at"])
    require(entry < exit_at <= recorded, "invalid CLOSED trade closure chronology")
    return entry_session, trade_session_at(sessions, index, exit_at, "exit")


def validate_trades(rows, sessions, run_keys, cutoff):
    """Validate every supplied trade, including open and unreferenced rows."""
    trades = unique(rows, "trade_id")
    index, entry_sessions = trade_session_index(sessions), {}
    for identity, trade in trades.items():
        require(isinstance(trade.get("strategy_id"), str) and isinstance(trade.get("scenario_id"), str)
                and (trade["strategy_id"], trade["scenario_id"]) in run_keys, "trade has no evaluated strategy/run")
        require(trade["strategy_id"] != "cash", "cash benchmark contains asset trading")
        entry_sessions[identity], _ = trade_lifecycle(trade, sessions, cutoff, index)
    return trades, entry_sessions


def equity_series(bundle, sessions, outer, strategies, scenarios, policy, cutoff, engine_hash, dataset_hash):
    rows_by_run = {}
    complexities = {}
    all_sessions = [s["session_id"] for s in outer]
    opening = currency = flows = None
    result, details = {}, {}
    for run in bundle["equity"]:
        key = (run["strategy_id"], run["scenario_id"])
        require(key not in rows_by_run, "duplicate equity run")
        rows_by_run[key] = run
    require(set(rows_by_run) == {(s, c) for s in strategies for c in scenarios}, "missing/extra strategy or stress case")
    trades, entry_sessions = validate_trades(bundle["trades"], sessions, set(rows_by_run), cutoff)
    # Check every supplied closed candidate/neighbor trade, including a trade
    # omitted from closed_trade_ids. Omitting its reference cannot waive its
    # actual holding interval. Benchmarks retain their frozen strategy contract.
    for trade in trades.values():
        if trade["strategy_id"] not in policy["benchmark_ids"] and trade["state"] == "CLOSED":
            holding_limit = integer(strategies[trade["strategy_id"]]["max_holding_sessions"], 1,
                                    min(5, policy["max_label_horizon_sessions"]))
            holding_span(sessions, entry_sessions[trade["trade_id"]], timestamp(trade["entry_at"]), timestamp(trade["exit_at"]), holding_limit)
    for key, run in rows_by_run.items():
        strategy, scenario = key
        expected_spec = {"engine_hash": engine_hash, "dataset_hash": dataset_hash, "strategy_id": strategy, "parameters": strategies[strategy],
                         "scenario_id": scenario, "cost_multiple": scenarios[scenario]["cost_multiple"],
                         "spread_multiple": scenarios[scenario]["spread_multiple"], "latency_ms": scenarios[scenario]["latency_ms"]}
        require(valid_hash(engine_hash) and run["execution_spec"] == expected_spec
                and run["execution_receipt_hash"] == sha(expected_spec), "execution receipt parameters/engine differ from the frozen run")
        require(number(run["spread_multiple"]) == number(scenarios[scenario]["spread_multiple"])
                and run["latency_ms"] == scenarios[scenario]["latency_ms"] and valid_hash(run["execution_receipt_hash"]),
                "unbound execution stress assumptions")
        rows = run["sessions"]
        require([r["session_id"] for r in rows] == all_sessions, "partial/misaligned outer equity series")
        require(run["complexity_count"] == (7 if strategy not in policy["benchmark_ids"] else policy["benchmark_complexities"][strategy]),
                "complexity differs from frozen application convention")
        complexities[strategy] = integer(run["complexity_count"], 0, 10000)
        this_opening = number(rows[0]["opening_equity"])
        this_flows = [number(r["boundary_cash_flow"]) for r in rows]
        if opening is None:
            opening, currency, flows = this_opening, run["currency"], this_flows
        require(this_opening == opening and run["currency"] == currency == policy["account_currency"] and this_flows == flows,
                "benchmarks use different capital, currency or cash flows")
        returns, exact_returns, exposures, turnovers, closed = [], [], [], [], set()
        previous_close = previous_snapshot = None
        sectors = {}
        total_costs = Fraction(0)
        for row in rows:
            session = sessions[row["session_id"]]
            start, end = number(row["opening_equity"]), number(row["closing_equity"])
            cash = number(row["boundary_cash_flow"])
            gross, fixed, variable = number(row["gross_pnl"]), number(row["fixed_cost"]), number(row["variable_cost_unscaled"])
            multiple = number(scenarios[scenario]["cost_multiple"])
            require(start > 0 and end > 0 and fixed >= 0 and variable >= 0 and (previous_close is None or start == previous_close),
                    "equity reset, nonpositive value or invalid costs")
            require(end == start + cash + gross - fixed - variable * multiple, "unreconciled cash/cost/equity ledger")
            require(timestamp(session["closes_at"]) <= timestamp(row["available_at"]) <= timestamp(row["recorded_at"]) <= cutoff,
                    "future/unavailable session valuation")
            require(row["unresolved_order_ids"] == [] and row["source_receipts"]
                    and all(valid_hash(h) for h in row["source_receipts"]), "unresolved or unreferenced execution")
            require(valid_hash(row["start_snapshot_hash"]) and valid_hash(row["end_snapshot_hash"]), "equity snapshots missing")
            require(previous_snapshot is None or row["start_snapshot_hash"] == previous_snapshot,
                    "portfolio snapshot continuity broken")
            net_return = ratio(end - cash - start, start)
            require(-1 < net_return <= number(policy["maximum_session_return"]), "undefined/unbounded account return")
            returns.append(float(net_return))
            exact_returns.append(net_return)
            exposure, turnover = number(row["exposure_fraction"]), number(row["turnover"])
            require(0 <= exposure <= 1 and turnover >= 0, "invalid exposure/turnover")
            sector_pnl = {name: number(value) for name, value in row["sector_gross_pnl"].items()}
            sector_exposure = {name: number(value) for name, value in row["sector_exposure_fraction"].items()}
            require(sector_pnl and set(sector_pnl) == set(sector_exposure)
                    and all(isinstance(name, str) and 0 < len(name) <= 100 for name in sector_pnl)
                    and sum(sector_pnl.values()) == gross and sum(sector_exposure.values()) == exposure
                    and all(0 <= value <= 1 for value in sector_exposure.values()), "unreconciled sector attribution")
            for name in sector_pnl:
                accumulator = sectors.setdefault(name, {"gross_pnl": Fraction(0), "exposure_sum": Fraction(0)})
                accumulator["gross_pnl"] += sector_pnl[name]
                accumulator["exposure_sum"] += sector_exposure[name]
            total_costs += fixed + variable * multiple
            exposures.append(exposure); turnovers.append(turnover)
            attempts, fills = integer(row["orders_attempted"]), integer(row["entry_fills"])
            require(fills <= attempts, "entry fills exceed attempts")
            if strategy == "cash":
                require(exposure == turnover == attempts == fills == fixed == variable == 0 and not row["closed_trade_ids"],
                        "cash benchmark contains asset trading or execution costs")
            for identity in row["closed_trade_ids"]:
                require(identity in trades and identity not in closed, "missing/duplicate closed trade")
                trade = trades[identity]
                require((trade["strategy_id"], trade["scenario_id"]) == key and trade["state"] == "CLOSED",
                        "open or misbound trade counted as closed")
                require(timestamp(trade["entry_at"]) < timestamp(trade["exit_at"]) <= timestamp(row["available_at"])
                        and timestamp(trade["exit_at"]) <= timestamp(trade["recorded_at"]) <= cutoff
                        and timestamp(session["opens_at"]) <= timestamp(trade["exit_at"]) <= timestamp(session["closes_at"]),
                        "trade has not closed in its reported session")
                require(valid_hash(trade["source_hash"]), "trade provenance missing")
                closed.add(identity)
            previous_close = end
            previous_snapshot = row["end_snapshot_hash"]
        result[key] = returns
        details[key] = {"exact_returns": exact_returns, "exposures": exposures, "turnovers": turnovers, "closed_trade_ids": sorted(closed),
                        "open_trade_ids": sorted(identity for identity, trade in trades.items()
                                                 if (trade["strategy_id"], trade["scenario_id"]) == key and trade["state"] == "OPEN"),
                        "fill_rate": sum(integer(r["entry_fills"]) for r in rows) / max(1, sum(integer(r["orders_attempted"]) for r in rows)),
                          "opening_equity": opening, "rows": rows,
                          "sector_attribution": {name: {"gross_pnl": decimal_string(values["gross_pnl"]),
                                                         "mean_exposure": float(ratio(values["exposure_sum"], len(rows)))}
                                                for name, values in sectors.items()},
                          "unallocated_execution_costs": decimal_string(total_costs)}
    return result, complexities, details


def search_accounting(search, request, now):
    no_gate_assertions(search)
    trials = unique(search["trials"], "trial_id")
    run, plan = request["manifest"], request["plan"]
    related = [t for t in trials.values() if t["account_id"] == run["account_id"] and t["mode"] == run["mode"]
               and t["dataset_hash"] == run["dataset_hash"]]
    require(related and any(t["candidate_id"] == request["trial"]["candidate_id"]
            and t["parameters_hash"] == request["parameters_hash"] and t["run_id"] == request["run_id"]
            and t["plan_hash"] == request["plan_hash"] for t in related), "frozen candidate absent from cumulative search ledger")
    for trial in related:
        require(trial["status"] in ("REGISTERED", "FAILED", "COMPLETE") and timestamp(trial["registered_at"]) <= now,
                "invalid/future trial ledger record")
    reads = unique(search["holdout_reads"], "read_id")
    for read in reads.values():
        require(valid_hash(read["dataset_hash"]) and valid_hash(read["input_hash"]), "invalid holdout read scope hash")
        require(timestamp(read["consumed_at"]) <= now, "future holdout read")
    # Exposure belongs to the original source dataset, even across accounts or
    # runs. Other source datasets do not spend this inference's look allowance.
    current_reads = [r for r in reads.values() if r["dataset_hash"] == run["dataset_hash"]]
    require(len(current_reads) <= 1 and all(r["run_id"] == request["run_id"] and r["candidate_id"] == request["trial"]["candidate_id"]
            and r["input_hash"] == request["input_hash"] for r in current_reads), "sealed period already exposed or selected again")
    ceiling = integer(plan["max_cumulative_trials"], 1, 10000)
    require(len(related) <= ceiling, "cumulative trial budget exhausted")
    return max(len(related), ceiling), len(current_reads)


def shadow_coverage(shadow, sessions, week_manifest, candidate, frozen_at, cutoff, max_holding_sessions):
    require(shadow["candidate_id"] == candidate, "shadow candidate differs")
    started = timestamp(shadow["program_started_at"])
    require(started >= frozen_at, "shadow program predates frozen candidate")
    observed = unique(shadow["observations"], "session_id")
    covered = set()
    for identity, row in observed.items():
        require(identity in sessions and sessions[identity]["phase"] == "SHADOW", "shadow calendar session missing")
        session = sessions[identity]
        first, last = timestamp(row["first_observed_at"]), timestamp(row["last_observed_at"])
        start, end = timestamp(session["opens_at"]), timestamp(session["closes_at"])
        require(start >= started and started <= first <= last and row["data_kind"] == "REAL" and integer(row["order_mutations"]) == 0,
                "nonprospective, synthetic or mutating shadow record")
        require(len(set(row["observation_hashes"])) >= 2 and all(valid_hash(h) for h in row["observation_hashes"])
                and valid_hash(row["market_data_hash"]), "unreferenced shadow observation")
        if first <= start < end <= last <= timestamp(row["recorded_at"]) <= cutoff:
            covered.add(identity)
    weeks = unique(week_manifest, "week_id")
    require({identity for week in weeks.values() for identity in week["session_ids"]}
            == {identity for identity, s in sessions.items() if s["phase"] == "SHADOW"}, "calendar week manifest incomplete")
    completed = []
    for identity, week in weeks.items():
        required = set(week["session_ids"])
        require(required and len(required) == len(week["session_ids"])
                and all(sessions[s]["week_id"] == identity for s in required), "duplicate or misbound calendar week")
        require(timestamp(week["closes_at"]) >= max(timestamp(sessions[s]["closes_at"]) for s in required), "week boundary precedes exchange close")
        if all(timestamp(sessions[s]["opens_at"]) >= started for s in required) and timestamp(week["closes_at"]) <= cutoff and required <= covered:
            completed.append(identity)
    trades = unique(shadow["trades"], "trade_id")
    index = trade_session_index(sessions)
    count = 0
    for row in trades.values():
        require(valid_hash(row["source_hash"]) and row["candidate_id"] == candidate, "shadow trade provenance/binding missing")
        entry_session, exit_session = trade_lifecycle(row, sessions, cutoff, index)
        require(started <= timestamp(row["entry_at"]) and row["entry_session_id"] == entry_session,
                "shadow trade entry/program/calendar binding failed")
        require(row.get("exit_session_id") == exit_session, "shadow trade exit/calendar/state binding failed")
        if row["state"] == "CLOSED":
            number(row["net_profit"])
            holding_span(sessions, entry_session, timestamp(row["entry_at"]), timestamp(row["exit_at"]), max_holding_sessions)
        if row["state"] == "CLOSED" and row["entry_session_id"] in covered and row["exit_session_id"] in covered:
            require(started <= timestamp(row["entry_at"]) < timestamp(row["exit_at"]) <= timestamp(row["recorded_at"]) <= cutoff,
                    "unmatured shadow trade")
            require(timestamp(sessions[row["entry_session_id"]]["opens_at"]) <= timestamp(row["entry_at"])
                    < timestamp(sessions[row["entry_session_id"]]["closes_at"])
                    and timestamp(sessions[row["exit_session_id"]]["opens_at"]) <= timestamp(row["exit_at"])
                    <= timestamp(sessions[row["exit_session_id"]]["closes_at"]), "shadow trade session binding differs")
            count += 1
    return {"completed_weeks": len(completed), "closed_trades": count, "covered_sessions": sorted(covered),
            "complete_week_ids": sorted(completed)}
