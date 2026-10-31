# Autonomous execution implementation

The runtime connects automatic S&P 500 screening, approved mathematical limits,
calibrated prediction fusion, native Perpetual advice, durable order submission,
native stops and position exits. The executable entry point is
[autonomous.rs](../crates/trady-cli/src/autonomous.rs); the original
`analyst|executor|gateway --once` commands remain foundation probes.

The executor invokes screening, prediction-service calls and native inference
through a bounded analyst transport. Live deployments require `analyst_socket`
and the separately managed analyst OS identity. Its private Unix socket directory
must not permit group/other writes; socket mode is `0660` for the shared IPC group.
Every ancestor must be a trusted directory without symlinks; Linux connections
also verify the peer UID against the protected endpoint owner.
The executor never loads the analyst DSN in this path. Paper deployments can use
the environment-cleared child transport. Both bind the original snapshot, cap
messages at 4 MiB and bound executor waits with a 30-second monotonic deadline.
Holdings use separate market reads and retain protection without candidate inference.

## Commands and configuration

```sh
cargo run --locked --bin trady -- autonomous check --config config/autonomous.example.json
cargo run --locked --bin trady -- autonomous run --config /path/to/reviewed-config.json --once
cargo run --locked --bin trady -- autonomous run --config /path/to/reviewed-config.json
cargo run --locked --bin trady -- autonomous analyst-service --config /path/to/analyst.json
cargo run --locked --bin trady -- autonomous holding --config /path/to/analyst.json --position ORIGINAL_PROPOSAL_ID
cargo run --locked --bin trady -- autonomous review-holding --config /path/to/analyst.json --request /path/to/review.json
cargo run --locked --bin trady -- autonomous recover-order --config /path/to/reviewed-config.json --intent INTENT --broker-order BROKER_ID
```

[autonomous.example.json](../config/autonomous.example.json) is deliberately
disabled. It contains unknown capabilities, no approved skill hashes, no formula
authority, no model artifacts and placeholder provider destinations. `check` reads
configuration and configured model files without credentials or network requests;
it reports configuration blockers and always reports `entry_authorized: false`.
It does not test provider availability, broker permissions or live acceptance.

`run` requires explicit `enabled: true`. `--once` executes one reconciliation and
decision cycle, which may submit an order under an enabled approved configuration.
The continuous loop waits 5–30 seconds between cycles; provider calls add latency.
Ctrl-C finishes the current synchronous operation, releases its database lease and
retains broker protection. Pausing entries also retains position management.
`close_position` latches liquidation and blocks new entries while configured.
Deployment controls are loaded at startup. The expanded Telegram gateway can also
request durable owner controls; the executor applies them only after reconciliation.
A deployment pause overrides a resume request. Close stays latched through restart
until flat reconciliation, plan removal and terminal outstanding orders. It also
cancels a pending entry; acknowledgement is separate from a broker-confirmed exit.

Broker `demo` binds `paper_` accounts to the Trading 212 demo origin. `live` binds
`live_` accounts to the live origin. Shadow uses a separate `shadow_` namespace and
an absolute `shadow_directory`. It constructs only the broker information reader,
records source-bound observations and checked proposals, and consumes at most one
hypothetical entry per verified session. It invents no fills or performance.
Release acceptance still needs twenty actual prospective calendar sessions.

## Deployment dependencies

The runtime is implemented, but a working financial deployment still needs real
approved artifacts and services. This repository does not supply a trained trading
model, approve data rights, manufacture evaluation reports or certify live release.

- Provision base and lab roles, then apply all eight migrations with the migrator role. Connect the runtime using
  restricted `TRADY_EXECUTOR_DSN` and `TRADY_ANALYST_DSN` credentials; the runtime
  does not migrate or accept an administrator connection in place of those roles.
- Install the reviewed [runtime unit templates](../services/runtime/README.md)
  under separate analyst/executor/gateway identities and apply the supplied
  credential, filesystem, resource and network restrictions. The private child
  compatibility path shares its parent's OS identity. Run retention cleanup when
  the runtime is absent. A running analyst uses a separate connection for cleanup,
  waking every 50 seconds with a 10-second statement timeout, including during
  analysis. A cleanup failure stops further service work when it is observed.
- Configure the expected actual broker account number and base currency. Broker
  environment selects `TRADY_T212_DEMO_API_KEY` / `TRADY_T212_DEMO_API_SECRET` or
  the corresponding `LIVE` pair. Follow the [credentials guide](./credentials.md).
- Supply the reviewed HTTPS analyst/market/prediction service described below.
  `TRADY_TRADING_INPUTS_TOKEN` supplies its optional bearer token. Credentials stay
  outside JSON deployment configuration.
- Approve a current constituent/classification exporter implementing
  [UniverseDocument](../crates/trady-adapters/src/screening/README.md). Configure
  actual provider identity, effective taxonomy and retention rights. There is no
  hardcoded membership fallback. An empty owner instrument allowlist selects nothing.
- Load exact approved runtime skill hashes, verified account/instrument/protection
  capabilities and a deployment-owned
  [FormulaAuthority](../crates/trady-application/src/optimization/mod.rs). Its
  prediction authority must approve the fusion revisions, models, calibration and
  net payoff estimation/cost model. Missing forecasts produce no entry.
- Supply approved Perpetual manifests and fitted native JSON artifacts. Model paths
  resolve relative to the configuration file. The adapter verifies artifact hashes,
  pinned runtime/schema/label identities, chronological training/calibration/validation,
  scenario support and current feature lineage. See the
  [native model contract](../crates/trady-adapters/src/perpetual/README.md).
  PostgreSQL resolves each entry context against its immutable calculation,
  successful instrument/market screening and scoped profile/feature records.
  Holding contexts resolve a current executor-attested `VerifiedHolding`: actual
  remaining quantity/acquisition costs, active thresholds, current exit costs and
  FX. `review-holding` persists native advice, final review and a scoped queued
  update. The executor rechecks the old exits before activation; a target update
  cannot change the native stop, extend the deadline or undo an exit latch.
- Live mode additionally requires current deployment-owned references for
  `account_binding`, `broker_capabilities`, `protection_recovery`, `paper_acceptance`,
  `shadow_acceptance`, `costs_and_market_data`, `model_validation` and `owner_approval`.
  The configuration's `release_candidate` pins source, calendar, models and formula.
  Before analysis and final entry dispatch, the executor resolves the current
  PostgreSQL ACTIVE release, checks exact bindings and bounds the dispatch deadline
  by actual approval expiry. `owner_approval` names the persisted owner evidence;
  `shadow_acceptance` is `shadow_<subject_hash>`; `model_validation` names independent
  verification. The other five gates name the persisted readiness evidence. Existing
  position management does not require fresh entry approval.
  The analyst returns the model identity from its persisted native assessment;
  the executor compares its exact artifact hash with the release's allowed models.
  Queued holding advice receives the same model check.

## Approved input service contract

The service supplies verified research and market facts; a model-generated JSON
object does not become a trusted service. It must resolve source truth, publication
times, source permissions, source independence, event calendars, broker support,
instrument rules, costs and finalized skill artifacts. The Rust runtime independently
checks bindings, calculations and readiness. The [input service](../services/inputs/README.md)
implements these routes over independently reviewed imports, with an explicit
calendar and a separate four-outcome model pipeline. Deployment supplies the actual
licensed records, verified source truth and approved artifacts.

All routes are relative to `inputs.base_url` and use strict JSON DTOs in
[the adapter](../crates/trady-adapters/src/autonomous/mod.rs):

| Method and path | Response / request |
| --- | --- |
| `GET session/{account}` | `SessionPacket`: account and optional `MarketSession`; actual exchange times, opening equity and confirmed cumulative gross deposits/withdrawals. |
| `GET quote/{instrument}` | `Quote`: instrument/currency, executable bid/ask, observation/receipt timestamps and tradability. |
| `GET costs/{instrument}` | `CostsPacket`: instrument, observation/expiry and current account-currency exit fees, uncertainty and FX. |
| `GET research/{account}` | `AnalystPacket`: owner/account/policy, freshness/retention, screening observations and at most five `ResearchDraft`s. |
| `POST predictions/{account}` | `PredictionRequest` carries the frozen `LimitsInput` and `BoundCalculation`; return `ScenarioPredictionInput` for that exact calculation hash, instrument, market and holding horizon. |

The POST is an analysis operation and carries financial account/scenario data only
to the approved service. Responses must contain the required LLM research, FinBERT,
quantitative and complete calibrated outcome evidence accepted by the
[fusion policy](../crates/trady-policy/src/prediction/mod.rs). Binary target-hit
probability cannot supply missing stop/expiry/non-fill probabilities or returns.
The service must return real approved estimates; unavailable support fails closed.

The adapter uses approved public HTTPS hosts, connection-pinned public DNS,
no proxies or redirects, bounded bodies and deadlines, strict duplicate/unknown
field rejection and shared 429/503 backoff. The service token goes in the bearer
header. Error messages omit response bodies and credentials.

Cross-currency entry refreshes exit economics before final dispatch. Any fee or
FX-rate change requires a new calculation and assessment; exact FX equality is
stricter than the native model's declared drift tolerance.

Only the first ranked screening finalist is considered. The adapter calculates
limits before asking for forecasts, refreshes the quote, validates the frozen
formula, creates native evidence records and persists context before native
inference. It records the final Perpetual review and returns an initially unusable
receipt only after persisted inference and readiness checks succeed. Evidence IDs
for native universe/confidence records are scoped per run; their hashes bind the
underlying universe and inputs. Research retention cannot exceed provider retention.

## Execution and recovery behavior

[AutonomousTrader](../crates/trady-application/src/autonomous/README.md) uses a
database-time lease, generation and revision compare-and-swap. A prepared intent
and attempted state are committed before every submission. Its daily session
slot remains consumed after rejection, cancellation, closure or restart. Confirmed
session loss of 2% pauses further entries for that session; gross deposits remain
in the performance denominator and withdrawals adjust performance separately.

Partial entries cancel the remaining buy and protect only observed acquired stock.
Stops reserve only uncovered sellable quantity. Actual broker wallet cost determines
net return and rounded profit limits. Durable trailing peaks/floors and exit latches
survive restart. A delayed stop rejection latches liquidation. Unavailable quotes,
calendar or research cannot raise targets or undo an exit latch.

Liquidation retains native protection until a tradable quote/session is available,
then requests cancellation and waits for terminal broker history and refreshed
available quantity. Profit exits use a calculated net target limit. A reversal
cancels that limit before market liquidation; no concurrent software sale can
oversell reserved stock. Stop, deadline, loss and owner exits use market orders.
Market orders and native stops remain subject to venue gaps, liquidity and fills.
Holding reviews are explicitly queued through the analyst repository. The runtime
activates only fresh position-bound native advice with current dossier evidence;
missing advice retains the active plan. Exit checks always use the old plan first.

An expired live release blocks new entry but permits structurally valid restart,
reconciliation and management of the already bound account. Expired model/formula
approvals do not disable the existing holding's protective management.

Unknown submissions are never replayed or matched by ticker, quantity or time.
The runtime scans pending orders and bounded terminal history for exact persisted
broker IDs. `recover-order` binds an ambiguous attempted intent to an operator-reviewed
exact ID only after independently matching broker facts and the original account
mode/currency. Missing history, unexplained holdings, corporate actions or conflicting
facts stay in recovery. A prepared intent whose dispatch was never recorded also
requires operator investigation; this command does not fabricate a no-send resolution.
An ambiguous cancellation stays pending for observation rather than being blindly retried.

The journal and research tables are separate from legacy paper fixtures. Restricted
roles use narrow security-definer functions; runtime roles cannot directly update
the autonomous ledger. Research attempts may leave earlier immutable records when
a later readiness check fails. Cleanup removes expired trading records and the
research/context copies in queued holding updates; only hash identities survive.

## Verification

The [completion record](./reports/2026-10-31_IMPLEMENTATION.md) records the latest
checks and activation dependencies. Tests use local fixtures, loopback HTTP and
a disposable PostgreSQL container. No real broker is contacted by verification.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
```

Behavioral fixtures exercise valid formula/fusion entry, partial fills, stop failures,
fencing, crash boundaries, cancellation races, exact recovery, persistent daily/loss
latches, profit limits and trailing exits. Broker tests use loopback HTTP; native
inference tests fit synthetic data. PostgreSQL checks use a new disposable container.
These checks do not contact a broker, train/approve a financial model or run a live
acceptance exercise. Provider latency, outage recovery and real account permissions
must be demonstrated in the deployment's demo/shadow release evidence.

Remaining contract differences are tracked in the
[README drift report](./reports/2026-10-31_DRIFT_REPORT.md).
