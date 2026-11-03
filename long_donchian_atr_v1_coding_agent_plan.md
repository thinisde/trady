# Coding-Agent Plan: Validate and Release-Gate `long_donchian_atr_v1`

## Objective

Make Trady's pre-registered `long_donchian_atr_v1` baseline reproducible, independently validated, and fail-closed for live execution.

The agent must treat this as a release-readiness and evidence task. Do not tune the strategy to improve the 2026 result. Keep `long_donchian_atr_v1` immutable unless an implementation or accounting defect is found. Any rule, parameter, universe, cost-model, or execution-policy change requires a new authority ID and new hashes.

The final state must be one of:

- `research`: historical evidence exists, but live entries remain disabled;
- `shadow`: prospective shadow evidence is being collected;
- `paper_validated`: shadow and paper-execution gates pass;
- `approved`: all release gates pass and live execution is explicitly enabled;
- `paused` or `retired`: execution is blocked by policy or an unresolved defect.

The current expected state is `research`, with entries disabled and paused.

## Authoritative inputs

Use these files as the canonical inputs and verify their exact paths before editing:

- `config/iex30.universe.json`
- `config/formulas/long-donchian-atr-v1.authority.json`
- `config/autonomous.iex30.demo.json`
- `crates/trady-policy/src/limits/donchian.rs`
- `crates/trady-policy/src/limits/mod.rs`
- Existing validation/report tooling under `var/formula/long_donchian_atr_v1/` and its source scripts

If the repository or any required file is unavailable, stop before modifying anything and report the missing path. Do not create replacement files from assumptions.

## Frozen authority semantics

Verify that the implementation exactly matches these registered rules:

1. Long-only, daily, regular-session Donchian breakout.
2. Signal high is the highest high of the previous 20 completed bars; the current bar is excluded.
3. Wilder ATR(14) is seeded with the arithmetic mean of the first 14 true ranges.
4. True range includes the previous close.
5. A signal requires 35 valid completed bars.
6. Signal condition is strict: `close > previous_20_bar_high`.
7. The next regular-session open is accepted when it is no more than `0.10 ATR` above the signal close. The exact boundary is inclusive.
8. A lower open uses the first executable price under the declared execution-price policy.
9. Initial stop distance is `1.50 ATR`.
10. Target distance is `1.50R = 2.25 ATR`.
11. If stop and target are both touched in one bar, stop resolves first.
12. A gap through the stop is recorded at the first executable price.
13. An unresolved position exits at the close of the fifth holding session.
14. There is no trailing, averaging, pyramiding, leverage, or reopening without a new signal event.
15. FinBERT and model fusion are disabled for this authority.
16. A deterministic model-free proposal is accepted without a prediction/fusion payload only when the authority explicitly declares fusion disabled.

If any semantic ambiguity is discovered, document it and add a versioned authority clarification. Do not silently choose behavior in the runtime.

## Phase 0: Establish a clean baseline

Before making changes:

- Record the current Git commit.
- Record the current authority, universe, runtime, and implementation hashes.
- Record the existing test and validation commands.
- Run the existing test suite and save the result.
- Copy or preserve the current validation reports before regenerating them.
- Confirm that the demo runtime remains disabled and entry-paused.

Create a short baseline manifest containing:

- Git commit SHA
- Rust toolchain version
- Python version and dependency lock/hash
- Authority JSON SHA-256
- Universe JSON SHA-256
- Runtime-config SHA-256
- Dataset manifest/hash
- Report-generation command
- Test command and result

Do not overwrite existing reports until the new run has been independently compared with them.

## Phase 1: Implement authority and configuration integrity checks

Add a canonical integrity verifier that checks:

- Authority ID and version match across authority and runtime configuration.
- Universe hash in the authority matches `config/iex30.universe.json`.
- Formula parameters match between the authority and runtime copy.
- Fusion/model flags match and remain disabled.
- The implementation hash or registered code revision is the expected one.
- The demo configuration is disabled and pauses entries.
- The runtime cannot start live execution if any required hash or status check fails.

The verifier must fail closed. Missing evidence, missing hashes, or an unreadable configuration must produce a blocking error, not a default-allow behavior.

Add a CI test that fails when the runtime copy diverges from the canonical authority.

Add a runtime state check equivalent to:

```text
if authority.status != approved:
    reject_live_execution()

if any_registered_hash_mismatches:
    reject_live_execution()

if required_evidence_is_missing:
    reject_live_execution()
```

The error should identify the authority, failed gate, and reason for the block.

## Phase 2: Make the historical replay reproducible

Create an immutable input manifest for the historical package. It must identify:

- Raw Alpaca/IEX bar files or API snapshots
- Query timestamps and request parameters
- The 30-symbol universe and its effective date
- Session calendar and timezone
- Adjusted versus unadjusted price policy
- Split and dividend treatment
- Missing, duplicate, malformed, or partial bars
- Parser and dependency versions
- Cost-model parameters
- Position-sizing parameters

Do not rely only on derived bars or a mutable API query. Preserve the raw inputs or a content-addressed snapshot.

Explicitly document the frozen-universe limitation. If the same current IEX30 list is replayed backward, report the resulting survivorship or membership-history limitation instead of presenting it as a point-in-time universe study.

Define and test the exact meaning of:

- regular-session open;
- first executable price;
- missing or stale next-session open;
- holding-session number one;
- close of the fifth holding session;
- price rounding and tick-size handling;
- 20 bps round-trip cost.

The 20 bps proxy must be applied exactly as declared. State whether it is 20 bps total round-trip or 10 bps per side, and ensure it is not applied twice.

## Phase 3: Build an independent trade and portfolio ledger

Implement or verify an independent replay path, preferably in Python if the production implementation is Rust. It must consume the same immutable raw-data snapshot and independently calculate:

- valid-bar count;
- previous 20-bar high;
- true range;
- Wilder ATR seed and updates;
- signal eligibility;
- opening-gap acceptance;
- entry, stop, target, and exit prices;
- exit reason;
- per-trade gross and net return;
- chronological portfolio equity;
- mark-to-market drawdown.

Persist one immutable record per signal and per position lifecycle. Each record should include:

- symbol and session dates;
- authority hash;
- code and dataset hashes;
- signal close;
- previous 20-bar high;
- ATR;
- valid-bar count;
- next open and executable-price source;
- acceptance/rejection decision and reason;
- entry, stop, target, quantity, and notional;
- holding-session counter;
- exit price and reason;
- gross return;
- cost amount and cost model;
- net return;
- portfolio equity before and after the event.

Report trade-level and portfolio-level metrics separately. Do not calculate portfolio compounding by multiplying trade returns unless the trades are strictly non-overlapping and fully reinvested.

Verify that:

- arithmetic mean net return uses closed-trade net returns;
- compounded return comes from the chronological equity ledger;
- maximum drawdown comes from the equity curve;
- overlapping positions and cash allocation are handled consistently;
- costs are applied once;
- gap-through-stop losses are represented at the executable gap price;
- no skipped or missing trade is silently converted to a zero return.

Reconcile the reported figures for 2024, 2025, and 2026 from the independent ledger. The 2026 report currently shows 172 trades, arithmetic mean net return `+0.0163%`, and compounded return `-34.87%`. Treat that as a metric-reconciliation blocker until the independent ledger reproduces it and explains the dispersion.

## Phase 4: Expand deterministic tests and parity coverage

Retain the existing Rust/Python parity report, but expand it beyond three representative cases.

Add unit, integration, property, and generated-boundary tests for at least these cases:

- 34 valid completed bars: no signal.
- Exactly 35 valid completed bars: signal may be evaluated.
- Current bar creates a new high: current bar remains excluded from the previous-20 high.
- `close == previous_20_bar_high`: no signal.
- Next open exactly `signal_close + 0.10 ATR`: accepted.
- Next open above the threshold: rejected and logged.
- Next open below the signal close: accepted at the first executable price.
- Missing next open: skipped and logged with a reason.
- Open below the stop: exit recorded at the first executable price.
- Stop and target touched in the same daily bar: stop wins.
- Exit exactly at the close of the fifth holding session.
- Target strictly above entry.
- Stop strictly below entry.
- No duplicate position while a symbol is open.
- No reopening without a new signal event.
- Wilder ATR seed uses the arithmetic mean of the first 14 true ranges.
- True range includes the previous close.
- Disabled model fusion accepts a model-free proposal without a prediction payload.
- Enabled model fusion rejects a proposal with a missing prediction payload.
- Authority/runtime hash mismatch blocks startup or live execution.

Use generated vectors/property tests for numeric edge cases and a numeric tolerance appropriate to unrounded plan values. Preserve the exact tolerance in the report.

Run parity over the complete deterministic fixture set and, where possible, over the complete frozen IEX30 replay. Record any mismatch with the first divergent event and all input hashes.

## Phase 5: Diagnose the 2026 retrospective result

Once the ledger is reproducible, produce a diagnostic report without changing v1 parameters.

Break the 2026 result down by:

- month;
- symbol;
- gross versus cost-adjusted return;
- stop, target, gap-stop, and fifth-session exits;
- entry-gap size;
- ATR percentile or volatility bucket;
- overlapping-position count;
- largest wins and losses;
- consecutive losses;
- data-quality exceptions;
- rejected and accepted opening gaps.

Run a cost-sensitivity report for at least:

- 0 bps;
- the frozen 20 bps proxy;
- measured paper cost;
- a conservative stress cost.

Classify the outcome:

1. **Data or implementation defect:** fix it, regenerate all periods, and issue a corrected authority/release record.
2. **Accounting defect:** fix the ledger and reports; preserve the old report as superseded evidence.
3. **Policy defect:** fix enforcement and invalidate reports that violated the policy.
4. **Genuine strategy weakness:** retain the negative result and keep v1 research-only.

Do not remove, truncate, or optimize around the 2026 period.

If a new filter, lookback, ATR value, cost model, universe, or execution rule is proposed, create a separately pre-registered `v2` authority with a new development/holdout design and a new prospective period.

## Phase 6: Implement the prospective shadow ledger

Register the exact authority and hashes before the shadow run begins. The shadow run must use the same signal and plan implementation intended for paper execution.

For every session and every IEX30 symbol, append:

- data snapshot identifier and timestamp;
- bar timestamps and quote freshness;
- signal close;
- previous 20-bar high;
- ATR;
- valid-bar count;
- signal decision;
- next regular-session open;
- gap threshold;
- accepted/rejected decision and reason;
- executable entry price;
- stop, target, quantity, and risk budget;
- simulated exit event and reason;
- simulated gross, proxy-cost, and net result;
- authority, code, and data hashes.

The ledger must be append-only and immutable. Never rewrite prior observations because a later data request produces a different result.

Predeclare the shadow acceptance window before observing its outcome. A practical starting gate is at least 60 trading sessions and preferably at least 50 closed trades, but the final requirement must be written into policy before the run begins.

## Phase 7: Implement paper execution and reconciliation

Use the intended broker adapter and the same order-state machine that would be used for live execution.

Record:

- client order ID;
- submission timestamp;
- broker acknowledgement;
- intended versus simulated fill price;
- bid/ask and spread at decision and fill time;
- slippage;
- reported or modelled fees;
- partial fills;
- order rejections;
- cancellations and replacements;
- stop/target protection order creation and state;
- exit fill;
- local position state;
- broker position state;
- cash and buying-power reconciliation;
- restart/recovery behavior;
- unresolved discrepancies.

Compare observed paper costs with the frozen 20 bps proxy and report cost sensitivity. Paper execution evidence validates the adapter and order lifecycle; it does not prove live-market slippage or market-impact behavior. Keep those claims separate.

The paper gate must fail if there are unexplained order, position, cash, protection-order, or event-ledger discrepancies.

## Phase 8: Enforce risk and execution policy

Verify that quantity sizing uses the declared initial-risk policy and the actual stop distance:

```text
risk_per_share = entry_price - stop_price
quantity = floor(allowed_initial_risk / (risk_per_share + expected_slippage))
```

Enforce all of the following:

- per-position initial-risk limit;
- aggregate open-risk limit;
- maximum notional exposure;
- maximum simultaneous positions;
- no leverage unless explicitly enabled by policy;
- no averaging;
- no pyramiding;
- no unintended reopening;
- stale-data rejection;
- missing-open rejection;
- kill switch for reconciliation failure;
- kill switch for stale quotes or abnormal prices;
- kill switch for daily loss or portfolio drawdown limits.

Document that the initial-risk calculation is an ex-ante risk estimate. A gap through the stop can create a larger realized loss.

## Phase 9: Create machine-checkable release gates

Add a release-evidence manifest that contains:

- authority ID and version;
- authority hash;
- universe hash;
- runtime hash;
- code commit;
- dataset hash;
- parity report;
- test report;
- historical replay report;
- prospective shadow report;
- paper execution report;
- risk-policy result;
- reconciliation result;
- reviewer or approval identity;
- release timestamp.

Require all of these gates before transitioning to `approved`:

- authority and runtime hashes match;
- complete parity suite passes;
- complete boundary/property suite passes;
- data provenance is complete;
- historical metrics reconcile from the independent ledger;
- shadow period meets the predeclared duration/trade-count gate;
- paper execution has no unexplained reconciliation gaps;
- observed execution costs are within the predeclared budget;
- per-position and aggregate risk policies pass;
- no leverage, averaging, pyramiding, duplicate entries, or unintended reopenings occur;
- a maximum-drawdown and kill-switch threshold is explicitly defined;
- the release record is attributable and immutable.

Until every gate passes, keep:

```text
status = research or shadow or paper_validated
live_enabled = false
entries_paused = true
```

## Required deliverables

The coding agent must produce:

1. Authority/runtime integrity verifier.
2. CI test for authority and universe hash consistency.
3. Expanded Rust unit/integration/property tests.
4. Independent Rust/Python parity fixture set and report.
5. Immutable raw-data/input manifest.
6. Independent trade and portfolio ledger replay.
7. Reconciled historical report for 2024, 2025, and 2026.
8. 2026 diagnostic and cost-sensitivity report.
9. Append-only prospective shadow ledger and report.
10. Paper execution ledger and reconciliation report.
11. Machine-checkable release-evidence manifest.
12. Fail-closed runtime gate preventing live execution before approval.
13. Documentation of all unresolved limitations, including frozen-universe survivorship, IEX feed scope, opening-price semantics, gap risk, and paper-execution limitations.

## Completion criteria

The task is complete only when:

- the historical reports reproduce from immutable inputs;
- the 2026 result is reconciled and retained;
- all boundary and parity tests pass;
- shadow and paper evidence are collected according to predeclared gates;
- all order and position state reconciles;
- live enablement remains impossible while evidence is incomplete;
- any rule change is registered as a new authority instead of mutating v1.

Do not declare the strategy live-ready merely because the Rust/Python parity tests pass. Parity proves implementation agreement; it does not prove prospective performance or execution quality.
