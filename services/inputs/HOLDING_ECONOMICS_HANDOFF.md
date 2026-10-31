# Holding economics correction handoff

## Interface

The existing CLI `trady autonomous review-holding --config FILE --request FILE` benefits from
`VerifiedHolding::validate_context` without any CLI changes. That route resolves
executor-attested economics, persists scoped context, obtains native advice and
queues a review. Final executor activation uses
`activate_reviewed_holding_update`; the fenced journal remains responsible for
root's exact persisted sentiment-dependency check before saving a new revision.
No manifest, migration, IPC, owner-control or CLI dispatch edit was required here.

Production changes:

- `crates/trady-storage/src/holding_research.rs`: advice acquisition uses the
  owner's policy target domain, rather than the original entry confidence.
  Positive-return prices must be tick-aligned and at most the reachable
  `target_price` for the nominal policy ceiling. EUR 205 / quantity 2 / tick 0.01
  permits the rounded 1% target at 103.53, but rejects 103.54 and off-tick prices.
- `crates/trady-domain/src/perpetual.rs`: entry scenarios retain `stop < entry`;
  explicit holding scenarios admit break-even/profitable stops below target.
  `ScenarioSupport` retains signed `(entry - stop) / entry` distances. Nonpositive
  bounds require explicit position support. Old positive-only manifests continue
  to reject profitable stops; no absolute-value conversion broadens their domain.
- `crates/trady-application/src/autonomous/updates.rs`: final activation checks
  current acquired quantity, account/instrument currency and fresh research
  lifetime. It applies the explicit nominal target against current research
  confidence even when an excessive fraction shares an allowed rounded price.
  A reference with maximum negative probability at least 0.60 subtracts ten
  points and independently forbids raises. Favorable sentiment adds no points.
  Live missing references remain rejected. Reference shape/freshness checks and
  the atomic persisted dependency match are root-owned; this overlay grants no
  authority to a locally supplied reference.

Tests:

- `crates/trady-domain/src/perpetual.rs`: holding/entry stop geometry regression.
- `crates/trady-storage/tests/holding_economics.rs`: four TEST ONLY verified
  holding regressions; no PostgreSQL instance is required.
- `crates/trady-adapters/src/perpetual/holding_economics_tests.rs`, registered by
  a narrow addition to `perpetual/tests.rs`: four tests use genuinely fitted
  native synthetic trees and the application review/persistence contract, then
  exercise final plan activation. Original score 75/target 1% successfully becomes
  fresh score 85/target 2%, including a USD-to-EUR profitable-stop scenario.
  Mutations of quantity, cost, fees, FX, model identity/approval, features/quote
  drift and missing/stale research are rejected. Separate sentiment regressions
  cover the exact boundary, penalty, no-raise veto, no positive bonus and live
  missing-reference rejection. Synthetic references test policy reduction only;
  root's PostgreSQL test owns actual atomic activation/revocation coverage.
- `crates/trady-adapters/src/financial/tests.rs`: added a focused Retry-After
  table for both 429 and 503. The earlier missing closure delimiter was fixed at
  root's request. No financial module or original six financial tests were
  authored by this worker. `financial/tests.rs` appeared during this task and
  another writer subsequently added a receipt-clock constructor argument and
  corrected long-delay handling, including the corresponding test call/table.
  Those concurrent edits were preserved. No child agent was spawned by this
  worker; the original financial author's identity is unknown in this session.

Contracts changed narrowly in `trady-domain/src/README.md`,
`trady-storage/src/README.md`, `trady-application/src/autonomous/README.md`,
`trady-adapters/src/perpetual/README.md`, the new financial component README,
and this local handoff/previous input-service handoff.

## Invariants

The fixes do not approve any real model, financial history or performance.
Broader holding support requires actual separately evaluated and approved native
artifacts with the declared signed support range. Existing artifacts are not
promoted or rewritten. The nominal policy/confidence ceiling and old-plan exits
remain authoritative; native advice cannot override either.

## Side Effects

Only source, tests and local contracts changed. Tests fit synthetic models in
memory. No provider, paid API, broker, startup, database migration, commit or push
was invoked. Build output is under `/tmp/trady-inputs-worker-build`; package
artifacts there were cleaned to recover disk space, then the holding tests reran.

## Verification

Targeted results, using `CARGO_TARGET_DIR=/tmp/trady-inputs-worker-build` and
`--offline`:

```sh
cargo test -p trady-storage --test holding_economics --offline        # 4 passed
cargo test -p trady-domain --lib perpetual --offline                   # 4 passed
cargo test -p trady-adapters --lib perpetual --offline                 # 15 passed
cargo test -p trady-adapters --lib financial::tests::retry_after --offline # 1 passed (22 cases)
cargo test -p trady-adapters --lib inputs_service::tests --offline     # 12 passed
```

Root owns global formatting, Clippy and full/isolated PostgreSQL checks. This
worker did not alter root's `tests/verified_holdings.rs`, migration 8 or atomic
dependency predicate. The scoped implementation is stable for those checks.
