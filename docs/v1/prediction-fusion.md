# Prediction fusion and mathematical limits

Status: v1 design contract, updated 2026-10-31. Typed Rust contracts and pure
calculations are the first implementation milestone. No approved fusion model,
four-outcome predictor, historical dataset, LEAN experiment, Lean 4 proof,
deployment approval or live activation is established by this document. The
[weekly workflow](./weekly-optimization.md) owns research and release evidence.
The [implementation record](./prediction-implementation.md) identifies the tested
Rust subset and the trusted services still required.

## Authority and data flow

The target flow is:

```text
Point-in-time LLM research + FinBERT + Perpetual + quantitative observations
    -> immutable predictions and historical reliability records
    -> PredictionFusionEngine: calibrated baseline or ABSTAIN
    -> finite scenario calculation and expected-utility selection or NO_TRADE
    -> independent Rust readiness, financial policy and execution checks
    -> durable intent, broker reconciliation and original-plan protection
```

The LLM selects research subjects, extracts attributed facts and proposes bounded
scenario constraints and hypotheses. The approved mathematical engine supplies
the numerical buy limit, stop, target, holding horizon and quantity. LLM price
hints remain comparisons; they cannot replace a calculation. This supersedes the
earlier design in which the LLM selected final numerical limits. It does not
enable a dormant implementation or weaken [trading policy](./strategy.md).

| Component | Information it supplies | Authority boundary |
|---|---|---|
| LLM research | News, X, filings, announcements, catalysts, management statements, counterevidence, lineage and uncertainty | Subjective confidence and narrative stance are not calibrated outcome probabilities |
| FinBERT | Positive/negative/neutral probabilities for exact permitted text and model revision | Sentiment classes are not price direction or target-hit events; preserve the deterministic adverse-sentiment overlay |
| Perpetual | A scenario-specific target-hit probability within approved model support | Its current binary output alone cannot establish expected return or a complete outcome distribution; unsupported outputs stay absent |
| Quantitative collector | Lagged momentum, ATR, realized volatility, volume, VWAP, relative strength, liquidity, regime and missingness | Uses completed observations with known availability; indicators are features rather than independent news confirmation |
| Historical evaluator | Matured forecast errors, calibration, support and regime reliability | Uses preserved original predictions, chronological splits and independently controlled outcomes |
| Rust calculator/executor | Decimal prices, lots, costs, risk checks, final scenario binding and intents | Predictions cannot relax owner policy, readiness, permissions, protection or order-state reconciliation |

## Typed prediction contract

The provider-neutral contract belongs in `trady-domain`; the pure
`PredictionFusionEngine`, utility calculation and financial solver belong in
`trady-policy`. Application ports resolve approved revisions, input references and
persisted results. Provider adapters never choose a model path from model output.
Every version-1 prediction has the following fields, even when a value is unknown:

| Field | Meaning and validation |
|---|---|
| `schema_version`, `prediction_id` | Exact supported version and immutable bounded application identity |
| `source`, `model_version` | LLM research, FinBERT, Perpetual or quantitative source; exact approved model/extractor revision |
| `instrument_id`, `market_snapshot_id`, `scenario_hash` | Instrument and observations; a scenario forecast additionally binds the exact final scenario |
| `observed_at`, `available_at`, `generated_at`, `valid_until` | Event/observation time, first information availability, original forecast production and expiry, all UTC; never substitute retrieval for availability |
| `horizon` | Decision instant, exchange-calendar deadline and one through five exchange sessions; not an ambiguous number of wall-clock days |
| `target_definition` | Typed evidence direction, text sentiment, target-before-stop or mutually exclusive scenario outcomes; includes label/execution convention version |
| `uncertainty` | Explicit absent value or validated uncertainty in a specified scale; absence is not zero |
| `calibration` | Unknown/uncalibrated/validated/drifted, report and calibrator IDs, training cutoff, independent evaluation interval and sample count |
| `regime`, `dependence_group_id`, `evidence_lineages` | Supported regime, shared-error grouping and original evidence origins; copied stories retain the original lineage |
| `payload` | Tagged research, sentiment, binary forecast, quantitative features or complete scenario-distribution data; never one interchangeable `confidence` field |

Use closed enums/objects, bounded collections, checked decimal strings for numeric
wire values and explicit missing/abstained states. Validate chronology, horizon,
probability range/sums, scope, hashes and model support after deserialization.
`available_at <= generated_at <= decision_at` is required for replay, and neither
observations nor availability may lie after the information cutoff. A training
cutoff and independent calibration outcomes must precede the production forecast.
An artifact's claimed validation status is accepted only after the application
resolves its deployment-approved calibration record.

The current `perpetual.assess` wire schema remains binary/advisory. Do not add
invented four-way probabilities to its result. A later separately trained model
and reviewed versioned adapter must supply a complete distribution. Existing
v2 LLM proposal schemas remain research contracts, not `FORMULA_V1` readiness
certificates. No newly documented `fusion.*` tool is advertised as available.

## Initial fusion baseline

Start with a deterministic, approved single calibrated outcome forecaster and
explicit evidence gates. The engine validates LLM research, FinBERT and quantitative
coverage, and uses the configured compatible Perpetual forecast when supported.
It does not average LLM confidence, sentiment and hit probability. The existing
[FinBERT veto and score reduction](./finbert/README.md#threshold-calculation-version-1)
remain independent constraints. Positive sentiment contributes no confidence bonus.

The result is `READY` with one supported calibrated forecast, contributing IDs,
scenario/horizon/market bindings, uncertainty, regime support, duplicate-lineage
and dependence diagnostics; otherwise it is `ABSTAIN` with bounded reason codes.
A valid binary forecast may support advisory comparison, but yields abstention
for expected-utility selection when expiry/non-fill/payoff information is absent.
Required missing, stale, conflicting, drifting or unsupported predictions cannot
be replaced with neutral probabilities or an optimistic renormalization.

The executable baseline checks original observation and information-availability
age as well as generation age. Fusion/utility results expire at the earliest
input/age/horizon deadline; scoring and selection recheck explicit current time.
Selection also binds the frozen objective, cash hurdle, account/snapshot,
currency and starting-equity normalization. A generation refresh cannot renew an
old observation's validity.

Recognize shared source lineages, identical prediction IDs and common dependence
groups. Sharing a news item between LLM extraction and FinBERT is expected and
reported once; it is not two corroborating news sources or two probability votes.
Identical IDs with changed content are integrity failures. Empirical residual
correlations require sufficient common matured out-of-sample observations; textual
similarity alone cannot claim statistical independence.

Historical reliability is a gate in the baseline, not an invented weighting
scheme. Regime-specific support requires recorded sample counts and calibration;
an unseen regime abstains or uses an explicitly evaluated pooled baseline.
Learned weights, stacking, nonlinear ensembles, dynamic model selection and
calibrator refits are future candidate revisions. Require out-of-sample improvement
over this baseline, cash and the approved strategy, complexity/latency costs,
ablation, and independent promotion evidence before activation.

## Scenario calculation and utility

The target pure boundary is:

```rust
calculate_limits(
    policy, approved_formula, evidence_constraints,
    account_snapshot, market_snapshot, cost_model, prediction_fusion,
) -> AcceptedCalculation | RejectedCalculation
```

Implement this as composition of the checked `FORMULA_V1` financial calculator,
fusion validation and a bounded scenario selector. A result is a calculation, not
an order. The full selector is usable only with approved compatible forecasts;
the existing binary Perpetual task cannot be silently upgraded to expected utility.

For every predeclared parameter vector, calculate a feasible buy `B`, protective
trigger `S`, target `T`, supported quantity `q` and exchange-session horizon `h`.
Use the [weekly formula](./weekly-optimization.md#example-parameterized-family):

```text
B = floor_tick(min(U, ask * (1 + entry_allowance_fraction)))
S = ceil_tick(B - stop_atr_multiple * ATR)
q = largest supported lot satisfying allocation and owner account-risk budgets
T = smallest permitted tick satisfying the configured net-return target
```

Require a marketable `B` within the verified range and a meaningful stop below
the executable bid, outside broker minimum distance and ordinary spread/noise.
Reject unrepresentable grid rounding, overflow, zero volatility or an empty
feasible set. Never tighten a stop artificially to fit the risk budget.

Freeze the final scenario before asking for predictions. Its hash includes
instrument, market/account/evidence snapshots, policy/formula/fusion revisions,
`B,S,T,q,h`, costs/FX, execution/label conventions and the original deadline.
Changing any bound value invalidates forecasts and Perpetual review. Enumerate
at most the registered finite domain; every forecast/scenario trial counts toward
cumulative search and paid-call budgets, including rejected/failed trials.

Use a two-pass handshake to avoid a circular dependency:
`generate_feasible_scenarios` first runs financial feasibility without predictive
authority and freezes bounded scenario IDs/hashes. Approved workers assess those
exact scenarios and persist original predictions. `select_limits` validates
fusion/payoffs and ranks only these frozen feasible results. Before submission,
the executor refreshes account/quote/FX/capabilities and recomputes feasibility.
The original forecast remains usable only under independently evaluated drift
tolerance with unchanged numerical limits, costs, exposure, regime and horizon;
record refresh snapshots as additional readiness evidence. Otherwise reject and
create/assess a new scenario. Never relabel an old forecast with a new hash or
chase the price during submission.

### Outcome partition

Use four mutually exclusive, collectively exhaustive modeled events:

| Event | Interpretation |
|---|---|
| `TARGET_FIRST` | Some entry fills; the target exit occurs before stop/deadline under the frozen execution convention |
| `STOP_FIRST` | Some entry fills; protection exits first, including modeled gaps/slippage |
| `EXPIRY` | Some entry fills; neither target nor stop completes first and the deadline/other modeled residual exit applies |
| `NON_FILL` | No entry fill before cancellation/expiry; include any actual attempted-order cost |

Partial fills belong to the first three events with conditional quantity/cost
distributions. Non-fill does not mean a full-sized losing trade. Ambiguous
same-bar stop/target ordering and unresolved horizons are unknown/censored labels,
not wins or invented probability mass. A binary target-before-stop probability
conditional on entry is incompatible with unconditional four-way probabilities
unless a separately validated mapping and fill model exist.

The initial execution-label convention uses the first terminal liquidation
decision latched under the **original frozen plan**: target, protective stop or
expiry/residual exit. Subsequent partial liquidation fills remain in that event's
conditional payoff. Non-fill means zero acquisition throughout the entry window.
Preserve actual fill history and distinguish triggers from fills. If an owner
intervention, corporate action or validated update changes the policy before
maturity, mark the original forecast censored/off-policy unless its trained label
explicitly supports that intervention. A separate permitted replay may label the
unchanged scenario with counterfactual provenance. Never substitute the changed
strategy's realized return. Strategy performance records every actual fill.

Let `p_o` be calibrated unconditional event probabilities, summing exactly to one.
Let `R_o` be the separately estimated conditional **net account return**, after
modeled entry/exit/FX/fees/slippage, normalized to the same starting account equity.
Require explicit, validated uncertainty `u` and a fixed complexity measure `c`.
One transparent initial objective is:

```text
E[R]       = sum_o p_o * R_o
downside   = sum_o p_o * max(0, -R_o)
utility    = E[R] - lambda_risk * downside
                 - lambda_uncertainty * u - lambda_complexity * c
choose argmax utility among feasible scenarios only if utility > cash_hurdle
otherwise NO_TRADE
```

All weights, return/uncertainty units, complexity convention and the nonnegative
cash hurdle are frozen by approved research configuration. Expected downside is
not a VaR/CVaR guarantee. Stop-first conditional returns must include stress/gaps;
the policy's 2% bound concerns planned modeled downside, not guaranteed fills.
Costs already inside `R_o` are never subtracted again in utility. Spread is
represented by executable bid/ask and the fill model; explicit slippage is only
the residual beyond those prices. Acquisition and liquidation charge each fee/FX
component once. Account versus position-return denominators remain explicit.

Concretely, `R_o = conditional_net_account_profit_o / E0`, where `E0` is starting
reconciled account equity common to all scenarios. `u` is a validated nonnegative
error radius for expected return in those same units, not a probability standard
error. `c` is a nonnegative integer complexity count from the approved feature/model
manifest. Lambdas and the cash hurdle are required nonnegative decimals with no
inferred defaults. Compare cumulative return to a common calendar-resolved
terminal instant within the five-session envelope: after an earlier exit/non-fill,
proceeds remain cash until that instant. V1 assumes zero cash interest, no
reinvestment and no annualization. Other opportunity-cost/interest models are
separate experiments; a short forecast cannot be multiplied into repeated trades.

Selection rejects non-finite/invalid distributions, incompatible units or
scenario bindings, unknown payoffs/uncertainty and unsupported horizons. It
reports `NO_TRADE` when no supported scenario has positive utility above cash.
It must not infer the three missing events or loss amounts from target-hit
probability, LLM confidence, target percentage or sentiment.

## Hard constraints and active positions

The financial feasibility pass precedes utility ranking: at most 98% of reconciled
cash including entry costs; one stock/no leverage/no additions; the unchanged
confidence floor and required research; sentiment vetoes; confidence-dependent
target ceiling and initially disabled above-3% targets; at most 2% planned net
loss and at least 1.5 reward/risk; owner account-risk ceiling; positive valid price
ticks and lot increments; current quotes/FX/capabilities; protected holding and
maximum five sessions. Mathematical success is not readiness or authorization.

Record target tick overshoot separately from the configured return. Verify final
grid remainders and original inequalities after all rounding: a non-overflowing
decimal operation can still lose precision at extreme magnitudes. Fail closed
when exact admissibility cannot be established.

Weekly formula/model changes apply to new plans. Preserve the existing plan's
revision, filled-cost basis, age, original deadline, favorable peak, monotonic
stop/trailing floor, consumed entry slot and exit latch. Check the old plan's exit
before any validated update; a new forecast cannot clear a latched exit or cause
weekly sell/rebuy rotation. Existing protection continues during model/lab failure.

## Historical calibration and evaluation

Store original forecasts before outcomes, with model/prompt/extractor versions and
what was known at the time. Maintain append-only matured-outcome records with
effective and available timestamps; revisions link to the previous record. Open
positions, partial/unresolved execution and incomplete five-session horizons are
pending, not closed wins. Reconcile actual broker-confirmed quantities, charges,
FX and cash events before scoring live trading outcomes.

Use cumulative historical evidence spanning multiple weeks and market regimes.
A weekly run adds a diagnostic batch; it is not the training set or proof of
independent improvement. Compute per-target and multiclass Brier score, appropriate
log loss, reliability bins, sample counts, abstention/coverage and forecast errors.
Report model/regime/horizon breakdowns, drift and uncertainty. Comparing a text
sentiment class with stock-return outcomes is a distinct downstream experiment,
not validation of FinBERT's original class probabilities.

Chronological training, calibration, development validation and independent test
intervals have separate identities. Fit transformations, feature selection,
calibrators and ensemble weights inside their own training windows. Purge every
sample whose feature or outcome horizon overlaps a later partition; apply the
predeclared embargo using actual exchange-calendar label end times. A pending
Friday horizon cannot train a weekend model. Freeze the candidate before consuming
an independent holdout; evaluation consumption is durable and not reset by retries.

Never ask a contemporary LLM to recreate past predictions and label the result
contemporaneous. Record possible model-training contamination and require
prospective shadow evidence for end-to-end claims. Use original predictions or
strictly point-in-time structured features for replay.

Run ablations for price-only baseline, research gate, FinBERT overlay, binary
Perpetual advice, complete scenario forecasts where supported, and each proposed
fusion extension on identical dates/capital/execution assumptions. Record feature
contributions, shared-error correlations and parameter neighborhoods with their
sample counts. Contribution is an explanation of a calculation, not causation.
All related trials, failures and prior searches count toward the cumulative
budget and search-adjusted uncertainty. Do not select on the sealed holdout.

## Weekly experiment and verification boundaries

The [weekend lifecycle](./weekly-optimization.md#weekend-lifecycle) freezes broker
reconciliation, immutable prediction ledger, matured labels, dataset and all
model/calibrator/fusion/formula versions. Codex diagnoses errors/redundancy and
registers one falsifiable change. Deterministic workers run chronological
development experiments; independent workers own sealed evaluation and release.
The local [Codex skill](../../.agents/skills/trady-weekly-optimization/SKILL.md)
guides this offline work and has no broker or deployment authority.

QuantConnect LEAN remains offline and uses the shared Rust calculator through a
reviewed bounded bridge. Its point-in-time replay must model spreads, commissions,
FX, residual slippage, latency, partial/non-fills, settlement, halts, corporate
actions, gaps and protection conflicts. Daily price touches alone cannot validate
intraday execution. Report net performance, drawdown, tail losses, turnover,
exposure, calibration, costs and robustness against the approved champion, cash
and simpler baselines. This document does not install or execute LEAN.

Lean 4 specifies accepted/rejected exact numeric semantics, ticks/lots,
affordability, planned downside/reward, holding caps, revision binding and monotonic
protection. Add probability partition validity, bounded scenario enumeration,
consistent utility units and `NO_TRADE` behavior under explicit forecast
assumptions. Audit transitive axioms, reject `sorry`/unreviewed assumptions,
require non-vacuity and exact Lean/Rust/bridge parity. Arithmetic theorems do not
prove calibration, dataset causality, profitability or broker fills. No proof or
backtest has run merely because the Rust interfaces exist.

Keep the release lifecycle `CANDIDATE -> VERIFIED -> SHADOW -> APPROVED -> ACTIVE`,
then retirement/quarantine/rollback. Formula, feature, calibrator, fusion, model,
cost and evaluator hashes form one compatible bundle; changing a hash invalidates
its evidence. Compare cost-stressed, search-adjusted independent improvement with
predeclared economic and risk gates, at least six outer intervals/100 closed
trades as screening floors, and at least four prospective shadow weeks with
adequate outcomes. These floors are not statistical sufficiency. Otherwise retain
the champion, continue exploratory shadow, or block new entries if it is unsafe.
Structural model/code/verification changes require explicit deployment review;
parameter automation remains a future narrowly preapproved mode.

## Acceptance and failure modes

| Case | Required result |
|---|---|
| Sentiment, subjective confidence or binary hit probability supplied as four-way probability | Reject the semantic mismatch; never fabricate expiry/non-fill outcomes |
| Missing/stale required model, unknown uncertainty, drift or unsupported regime | `ABSTAIN`; selector returns `NO_TRADE`; protection continues |
| Repeated news origins or correlated models | Retain provenance/diagnostics; no extra evidence points or independent probability votes |
| Future publication, overlapping label window, immature outcome or replayed contemporary LLM | Purge/censor or mark contamination; cannot certify an independent release |
| Scenario price/lot/horizon/cost/model changes after assessment | Invalidate binding; recompute and reassess the final scenario |
| Positive expected utility violates any financial constraint | Reject before ranking; prediction cannot override policy |
| All utilities are below cash, or only binary forecasts exist | `NO_TRADE`; explain absent support without inventing returns |
| Large decimal magnitude loses grid precision without overflow | Reject unless final ticks/lots and inequalities are exactly verified |
| One lucky week, repeated holdout or failed proof/parity | Keep/shadow/block; no self-approved activation |
| Weekly change while a position/exit latch exists | Retain original plan, age and protection; no forced rotation |

## Interface, invariants and side effects

The typed interfaces, fusion engine and scenario utility are pure computation
boundaries. Predictions remain advisory inputs to mathematical calculations;
financial and execution policy remain independent. Persistence, inference,
experiments, theorem checking and release are separate scoped application ports.
No new source of broker credentials, order submission or automatic activation is
introduced by these contracts.

## Verification

Require pure semantic/causality, probability-partition, duplicate-evidence,
abstention, utility, cost/FX, tick/lot, overflow and revision-binding tests. Record
actual evidence separately from requirements. Repository checks are
`cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked` and `python3 scripts/verify-assets.py`.
