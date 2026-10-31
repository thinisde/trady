# Autonomous execution and readiness

## Overview

[mod.rs](./mod.rs) coordinates fenced account observation, durable order attempts,
protection and latched liquidation. [readiness.rs](./readiness.rs) validates the
evidence, screening, advice and approved formula bound to an entry candidate;
its public items are re-exported. Concrete broker, research and journal adapters
remain outside this module. [updates.rs](./updates.rs) validates position-bound
native reviews before target activation and normalizes remaining exit charges.

## Interface

- `[mod.rs:TradingConfig]`: version, account/mode/currency/owner, policy,
  capabilities, approved skill hashes, optional `LiveRelease` and optional
  `optimization::FormulaAuthority`; `validate(now) -> Result<()>`.
- `[mod.rs:LiveRelease]`: account, policy version, expiry and named gate evidence.
  `[mod.rs:OwnerControls]` exposes `pause_entries` and `close_position`, both false
  by default.
- `[mod.rs:MarketSession]`: session ID/open/close/observation times, opening equity
  and confirmed gross deposits/withdrawals. Validation and entry-window methods are private.
- `[mod.rs:ExecutionObservation]`: snapshot, actual remaining acquisition
  `position_costs`, sellable `available_quantities` and total equity.
- `[mod.rs:ExecutionBroker]` extends `BrokerExecutionPort` with
  `observe(&mut self, account, tracked) -> Result<ExecutionObservation>` and
  `submit_order_before(request, expires_at)`. Adapters with waits must check expiry
  immediately before sending; the default method forwards to `submit_order`
  without checking expiry, permitting immediate fixture implementations.
- `[mod.rs:TradingInputs]`: mutable `session(account, now)`, `quote(instrument)`,
  `candidate(snapshot, now)` and `current_exit_costs(instrument, now)` ports.
  Exit costs default to `Ok(None)`.
- `[mod.rs:ExecutionJournal]`: mutable `acquire(config, worker, ttl_seconds)`,
  `renew(fence, ttl_seconds)`, `load(fence)`, `save(fence, expected_revision,
  state)` and `release(fence)` ports. `record_holding_observation` publishes actual
  holding economics; `pending_holding_update` resolves persisted reviews;
  `synchronize_owner_controls` applies durable controls after reconciliation;
  `validate_entry_release` resolves current live entry authority and expiry.
  The first three hooks default to no-op, `None` and cloned caller controls;
  their durable behavior requires an implementation.
- `[updates.rs:HoldingUpdate]`, `activate_reviewed_holding_update` and
  `holding_advisory_cost_fraction`: update identity/version/expiry,
  immutable advice/research and actual-cost validation, followed by pure activation.
- `[mod.rs:OrderRole]`: `Entry`, `Protection`, `Exit`.
  `[mod.rs:TrackedOrder]` stores request/role, attempted status, observation,
  cancellation request and failure; `broker_id() -> Option<&str>`.
- `[mod.rs:ActivePlan]`: candidate, acquired quantity, entry session/close/time,
  first order index, persistent `HoldingState`, observed holding sessions, exit latch,
  `market_exit_required` and unusable-data time.
  `[mod.rs:TradingState]` stores revision/position state/plan/orders,
  consumed/loss-paused sessions, last snapshot and recovery reason; defaults flat
  at revision zero. `[mod.rs:CycleReport]` returns revision/state/action/reason.
- `[mod.rs:AutonomousTrader::start]` accepts `(config, worker, journal, clock)`, validates and
  acquires a fence; `cycle(journal, broker, inputs, controls, clock)` returns a
  `Result<CycleReport>`. `[mod.rs:recover_order]` accepts `(journal, broker, fence,
  intent, broker_id, clock)` and binds an uncertain attempt after exact broker observation.
- `[readiness.rs:TradingResearchStore]` extends `PerpetualRepository` and
  `ScreeningRepository` with `store_calculation(input, calculation, retain_until)`,
  `store_dossier(dossier, retain_until)` and
  `store_context(context, retain_until)`.
- `[readiness.rs:EvidenceRecord]`, `SkillEvidence`, `Dossier`: scoped, versioned,
  expiring evidence/dependencies/lineages and approved skill artifact bindings.
- `[readiness.rs:AdvisorReceipt]`, `ScreeningReceipt`: scenario/account/quote/model
  review and universe/screening/finalist identities with observation/expiry times.
- `[readiness.rs:EntryCandidate]`: account/snapshot/policy/instrument identities,
  validity, price range/stop/target/horizon/deadline, confidence/cost/rule inputs,
  dossier/receipts, optional persisted `sentiment_reference` and optional formula. Methods: `liquidation_costs`,
  `scenario_hash`, `validate(config, snapshot, quote, now)`,
  `authoritative_limits(config, snapshot, quote, now) -> Result<&CalculatedLimits>`,
  `net_proceeds(quantity, price, now)`.
  `validate_refreshed_exit_costs(current, now)` requires unchanged fees/FX rate
  and a current conversion observation before cross-currency final entry.

## Invariants

- `[mod.rs:TradingConfig::validate]` requires version 1, valid policy, nonempty
  owner, account prefix matching paper/shadow/live mode and 64-character
  hexadecimal approved skill hashes. Live requires current account/policy-bound
  release gates: `account_binding`, `broker_capabilities`, `protection_recovery`,
  `paper_acceptance`, `shadow_acceptance`, `costs_and_market_data`,
  `model_validation`, `owner_approval`.
  `validate_for_management` retains structural/account checks while permitting an
  expired release to restart management/recovery; `validate(now)` blocks new entries.
  `ExecutionJournal::validate_entry_release` defaults to rejecting live entries;
  production journals resolve persisted evidence before analysis and dispatch.
- `[readiness.rs:EntryCandidate::authoritative_limits]` requires the optional
  configured authority and candidate formula for entry. It binds the entire
  account snapshot, instrument/currency, range, costs, rules, confidence,
  exceptional gate and validity. Stop, target and holding sessions must exactly
  match reproducible approved calculations. Deserialization alone permits absent
  formula/authority and grants no readiness.
  Supplied sentiment at maximum negative probability at least `0.60` reduces
  permitted formula confidence by ten points; at least `0.80` rejects entry.
- `[readiness.rs:EntryCandidate::validate]` requires matching account/snapshot/
  policy/instrument/currencies, lifetime at most five minutes, current screening
  and advice, tick-aligned positive stop, policy-bounded holding sessions and a
  future deadline within 21 days. Advice must match the scenario hash, actual
  snapshot and current bid/ask within tolerance no greater than `0.001`.
  Reported spread must cover observed spread, and recomputed confidence must
  meet the policy entry floor.
- `[readiness.rs:EntryCandidate::validate_dossier]` requires a finalized,
  blocker-free dossier with at most 128 unique current records, matching scope/
  policy/hash, no material conflicts and resolved acyclic dependencies. Required
  kinds are universe, screening, company_profile, relationships, watchdog,
  economics and confidence; primary verification and independent secondary
  lineage evidence are required.
  Claimed independent-lineage counts cannot exceed available secondary lineages;
  an exceptional target requires `exceptional_target` evidence. Revision hashes
  receive format checks; trusted origins and immutable dependencies are upstream
  requirements. Supplied sentiment references remain current within 900 seconds,
  expire within 15 minutes of scoring and bind present FinBERT model revisions.
  Live requires a scoped current sentiment reference. The PostgreSQL journal
  rechecks its source/claim/model dependencies in the plan-acceptance transaction;
  reconciliation and protection saves remain available after research expiry.
- `[readiness.rs:EntryCandidate::validate_dossier]` requires exactly 13 unique
  skill names with matching configured hashes and resolved artifact IDs: conversation-and-tools,
  search-planning, market-research, news-analysis, source-comparison,
  entry-readiness, sector-selection, the dossier's `sector-*` and `subsector-*`,
  company-profile, relationship-analysis, holding-watchdog and perpetual-review.
- `[mod.rs:AutonomousTrader::cycle]` requires current matching broker observations,
  base-currency accounting and nonregressing timestamps. New entries require an
  unused permitted session, no loss pause, validated policy/capabilities and no
  active plan; shadow mode, `pause_entries` and `close_position` block entry.
  Session loss of at least 2% pauses entries, adjusting confirmed cash flows and
  dividing by opening equity plus gross deposits. `close_position` also latches exits.
- `[mod.rs:AutonomousTrader::cycle]` reobserves after research, requires unchanged
  cash, flat holdings and terminal existing orders, then rechecks session, quote
  and candidate before consuming the session. Existing plans are managed before
  entry controls and configuration revalidation.
  Initial observation time cannot regress from the persisted snapshot; the final
  re-observation checks freshness without comparing its timestamp to the initial
  observation. Dispatch expiry intersects candidate/advice, session entry cutoff,
  quote/account freshness, capabilities, release, dossier and prediction expiries.
- `[mod.rs:ExecutionJournal::save]` requires atomic fenced revision comparison.
  `[mod.rs:reconcile_orders]` sends unexplained/inconsistent holdings, ambiguous
  attempts, absent nonterminal history, unknown orders, regressing fills and
  untracked pending orders to recovery. Missing observations do not erase known
  terminal facts. Unknown submissions are never automatically replayed.
- `[mod.rs:AutonomousTrader::manage]` latches exits for owner close, deadline/
  holding/session-loss limits, persistent trailing floors, stop/net target, rejected
  or expired protection evidence, missing actual cost basis/exit
  economics or at least 60 seconds of unusable quotes. Liquidation waits for
  entry/protection termination, usable quote/session and sufficient available
  quantity. Native protection remains while the executable quote/session is unavailable.
  Protection reservations cannot exceed holdings. Profit exits use a rounded target limit;
  a reversal cancels that limit before market liquidation.
- `[updates.rs:activate_reviewed_holding_update]` requires current scoped research,
  exact actual-cost/quantity/FX/exit-charge/stop/target/deadline bindings and fresh
  approved native advice. Active-plan exits run first; updates cannot change the
  native stop, extend deadlines, loosen trailing rules or reverse latched exits.
  Fresh research lifetime, current acquired quantity and account currency must
  match. The proposed nominal target uses the fresh research confidence ceiling;
  equal rounded sell prices cannot authorize an excessive nominal fraction.
  A current sentiment reference with maximum negative probability at least 0.60
  subtracts ten confidence points and independently forbids target increases.
  Favorable sentiment adds no points or authority. The fenced database save must
  still recheck the exact persisted reference before activating the revision;
  application-side reference validation cannot replace that atomic check.
  Policy rejection can return `Ok(PlanActivation)` with `activated=false` and
  the evaluated old state; application binding failures can return `Err` first.
- `[mod.rs:recover_order]` accepts an unused 1–128-character ASCII alphanumeric/
  underscore broker ID for an attempted unbound intent. Fresh exact-ID facts
  must match account/instrument/side/quantity, original mode/currency and nonregressing
  observation time, and have a known status.
- `[mod.rs:dispatch]` persists prepared and attempted state before broker I/O.
  A returned submission action describes the attempt; persisted rejection or
  ambiguity can remain, so it does not establish acceptance, fill or protection.

## Side Effects

`[mod.rs:AutonomousTrader::start]` acquires a 120-second journal lease. Cycles renew
it, load/persist revisions, snapshots, plans, sessions and order history, and call
the supplied broker/input/clock ports. Callers own lease release.

`[mod.rs:dispatch]` can submit Day limit buys, GoodTillCancel protective stop sells
and target limit or market liquidation sells with extended hours disabled. `[mod.rs:cancel]`
persists cancellation intent before I/O. Definitive no-send failures permit a new
attempt after reconciliation; uncertain/accepted cancellation requests remain pending;
acceptance is not terminal cancellation or released quantity. Recovery observes
the proposed binding and persists verified facts. Research-store methods declare
calculation/dossier/context persistence; concrete database/network effects belong to adapters.

## Verification

From the repository root,
`cargo test -p trady-adapters --lib perpetual::tests::holding_economics --locked`
fits a TEST ONLY native classifier and exercises this module's final reviewed
target activation: original score 75/target 1% becomes fresh score 85/target 2%,
including a profitable FX stop. Changed economics, native provenance/drift,
missing/stale research and nominal target overshoot must reject activation.

Run `cargo test --workspace --locked` from the repository root. Operational use
also requires concrete journal/broker/input implementations, current evidence,
approved skills/formula and, for live mode, the configured release gates.
