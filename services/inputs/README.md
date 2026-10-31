# Research and market inputs

## Overview

The Rust `inputs-service` command supplies the existing autonomous service
contract. It reads actual provider/operator exports through a durable review
boundary and calls the distinct point-in-time `model-lab` four-outcome model when
approved artifacts exist. Neither importing documents nor fitting a model grants
entry, model promotion or live-release authority.

No financial dataset, approved trading model, data license, prospective performance
claim, provider purchase or service startup is supplied by these examples.
`config.example.json` deliberately has no sources, reviewers or accounts and fails
validation until the operator supplies a reviewed registry. All test observations
are explicitly TEST ONLY and cannot receive production approval.

## Interface

The component contracts are [input service](../../crates/trady-adapters/src/inputs_service/README.md)
and [model lab](../../crates/trady-application/src/model_lab/README.md).
The CLI modules are [inputs_service.rs](../../crates/trady-cli/src/inputs_service.rs)
and [model_lab.rs](../../crates/trady-cli/src/model_lab.rs), each exposing `HELP`
and `run(args: &[String]) -> Result<serde_json::Value>` for the composition root.

The server binds loopback. An approved TLS reverse proxy exposes the public HTTPS
origin, strips any base-path prefix and preserves the authorization header.
Both service and client credentials must contain the same private bearer value:
`TRADY_INPUTS_SERVICE_TOKEN` for the server and `TRADY_TRADING_INPUTS_TOKEN` for the
existing client. The server uses the central credential loader. Secrets never
belong in registry JSON, source URLs, CLI arguments or review notes.

| Method/path | Actual Rust wire contract |
| --- | --- |
| `GET /session/{account}` | `SessionPacket` with verified calendar times and actual account opening equity/cumulative deposits/withdrawals; `session:null` only outside an explicitly covered session. |
| `GET /quote/{instrument}` | Executable `Quote` with bid/ask, currency, original observation and receipt times. |
| `GET /costs/{instrument}` | `CostsPacket` with current exit charges, uncertainty and FX. Deploy separately for different account currencies. |
| `GET /research/{account}` | `AnalystPacket` with separately reviewed dossier artifacts, skill hashes, original source comparisons and screening observations. |
| `POST /predictions/{account}` | The actual `PredictionRequest` containing `LimitsInput` and `BoundCalculation`; response is `ScenarioPredictionInput` for that exact calculation. |
| `GET /universe/SP500` | The actual `UniverseDocument`, without a hardcoded member list. |

Unknown/missing/stale/conflicting/unapproved inputs return a bounded sanitized
error. There is no neutral forecast, fabricated constituent list or fallback
financial history. The HTTP read credential cannot import, review, mutate accounts
or select a model path.

### Operator imports

Use strict `SourceImport` records with the typed `ImportPayload` enum. Supply
actual source bytes as `raw_document`, their SHA-256, original source URL/provider,
original lineage, observed/first-available/import times, expiry/retention, and
dependency IDs. The typed normalized payload is checked against that envelope.
An export from a genuine provider is supported; it need not be a model JSON file.

```text
trady inputs-service check --config /private/inputs.json
trady inputs-service import --config /private/inputs.json --file /private/import.json
trady inputs-service inspect --config /private/inputs.json --id original_import_id
trady inputs-service review --config /private/inputs.json --file /private/review.json
trady inputs-service export-universe --config /private/inputs.json --out /private/universe.json
trady inputs-service deadline --config /private/inputs.json --account paper_owner --sessions 3
trady inputs-service prune --config /private/inputs.json
```

`import` returns `STAGED_UNVERIFIED` and the exact import/raw hashes. A separately
authored `SourceReview` must pin both hashes, name an allowlisted independent
reviewer, and attest actual source identity, rights, original availability,
normalization and original lineage. `model_self_attestation` must be false.
The configured reviewer's reporting group differs from the source's group;
primary records additionally need explicit primary-source/reviewer permission.
This is a privileged local operator trust boundary. The code verifies hashes,
scope and chronology; it cannot verify legal rights or factual truth from a
claimed JSON flag. Restrict local write access to the import/review workflow.

An `EvidenceArtifact` contains the finalized artifact and matching
`EvidenceRecord.revision_hash`; its ID/dependencies/times must match the import.
Every dossier record must resolve to one such separately reviewed import. A
source record's original lineage must match the source envelope. Copies and
common reporting ownership cannot increase independent corroboration. Counting
uses a maximum set with distinct original lineages and distinct ownership groups;
multiple secondary publishers copying one original still contribute once. Finalized
company-profile, relationships, watchdog and economics artifacts and all required
approved skill references must exist. The runtime still creates and checks its
own native universe/screening/confidence/advisory receipts.

`CalendarDocument` must explicitly cover every date in its bounded interval with
a supplied session or a documented closure. The actual UTC instants support DST,
holidays and special closes. Session one is the entry session. Missing future
coverage blocks the deadline. The service does not substitute weekdays for an
exchange calendar or guess funding/cash events.

Quote/cost observation and FX ages are capped at 60 seconds. Import/review times
cannot renew them. A real deployment needs a trusted producer and independent
mapping verifier that publish new actual observations within that window; this
repository does not purchase or enable a market feed. Explicit pruning enforces
original retention even when the server is stopped. Derived imports cannot retain
source content beyond the earliest dependency retention deadline.

Cost staging and reads require fee and uncertainty currencies to match the
configured account currency. FX must convert into that currency with a positive,
current rate; reversing a pair or renewing only the cost timestamp cannot make
stale FX usable. The active index has a 20,000-entry publication/read bound,
separate from permanent identity tombstones. Reviews remove expired index entries
before capacity checks. `prune` streams and deletes expired indexes, including
legacy histories larger than the lookup scan limit, while preserving immutable
IDs. A shared OS file lock serializes import/review/cleanup across local processes;
readers remain bounded and fail closed during unavailable or inconsistent reads.

### Offline model commands

```text
trady model-lab fit --dataset /private/development.json --plan /private/plan.json --out /private/candidate.json
trady model-lab evaluate --candidate /private/candidate.json --holdout /private/sealed.json --journal /private/evaluation-journal --out /private/evaluation.json
trady model-lab inspect --candidate /private/candidate.json
trady model-lab bundle --candidate /private/candidate.json --evaluation /private/evaluation.json --approval /private/independent-approval.json --out /private/reviewed-model.json
trady model-lab offline-infer --bundle /private/offline-model.json --request /private/offline-request.json --out /private/offline-result.json
```

`ObservationDataset` uses supplied original feature records, actual feature-window
start/availability/decision times, exact scenario/market/source/execution hashes,
label-end/availability and a reconciled `PayoffRecord` for each matured class.
The scenario adds target fraction, stop distance, spread and exchange-session
horizon to the ordered feature vector. All payoff amounts use one account
currency; gross P&L excludes separately supplied fees, FX costs and slippage, which
are each subtracted once. Acquisition cost and starting equity determine account
normalization. Actual supplied idle-cash P&L is the evaluation comparator. Unknown,
partial, censored or ambiguous outcomes remain pending, not reconstructed wins.

The pure Rust baseline fits training-only scaling and a regularized softmax
classifier, plus one regularized conditional net-payoff regression per outcome.
It selects temperature and empirical residual-error radius using a separate
calibration interval, then reports untouched development validation. Its
predeclared windows purge crossing feature/outcome/availability intervals and
apply an explicit embargo. The independent holdout is a different dataset after
the frozen candidate/fit time. A durable interval claim prevents renamed,
overlapping, revised or retried holdouts from resetting consumption. A crash lock
fails closed until an operator inspects the claims and repairs the journal.

Reports contain actual class counts, Brier/log loss, reliability bins,
regime/horizon diagnostics, coverage/pending counts, conditional payoff errors,
and supplied cash comparisons. These measurements are diagnostics. Serial market
data do not establish independent trials or guaranteed interval coverage. The
uncertainty radius is an empirical calibration error in net-return units, not
subjective confidence or a statistical performance guarantee. Independent-test
residuals never refit it. Unseen regimes/horizons or out-of-support features abstain.

### Deployment and research-worker integration

`ModelDeployment` must pin the exact serialized `ApprovedModel` digest, its fixed
local path and approved reviewer. `ModelApproval` pins candidate/evaluation hashes
and exact dataset-verification, prospective-shadow and owner-approval document
hashes. `evidence_import_ids` must resolve to current independently reviewed
`EvidenceArtifact` imports with matching raw hashes and kinds
`dataset_verification`, `prospective_shadow`, `owner_approval`. Fitting/evaluation
does not create these documents. TEST ONLY artifacts cannot be bundled or loaded
for production even if a review JSON claims approval.

Set each account's `FormulaAuthority.prediction.fusion` to the exact approved
research, FinBERT, quantitative and outcome-model revisions. The full forecast
uses `ApprovedModel::report_id()` and `calibrator_id()`; utility uses
`estimation_id()` and the actual training/deployment cost-model hash. `model-lab
bundle` reports `model_id`, `calibration_report_id`, `payoff_estimation_report_id`
and `cost_model_hash`; it does not print the calibrator ID. Obtain that binding
through `ApprovedModel::calibrator_id()`, or prefix the candidate `sha256` from
`model-lab inspect --candidate FILE` with `temperature_` for the same reviewed
candidate. The v1 domain calls its full-forecast slot
`PERPETUAL`; put this baseline's distinct model ID in that slot. This does not
replace, fit or convert native binary Perpetual advice. Native entry/holding
assessment remains an independent readiness requirement.

The research worker can publish the public `SourceImport`/`SourceReview` types
through `ImportStore::{import,review}` or the CLI after actual independent review.
Publish original evidence first, then finalized dossier artifacts, then the
scoped `AnalystPacket` with all artifact IDs in import dependencies. Publish
`ForecastImport {account_id, observation: ForecastObservation}` for the exact
market snapshot. Its features and `SupplementalSignals` preserve original
research/FinBERT/quantitative observation, availability and generation times;
all lineages resolve to reviewed dependency imports. The prediction service binds
these observations to the authoritative calculation rather than asking the worker
to guess a future calculation hash or decision instant.

Holding-context builders may call `InputsService::forecast_observation` for fresh
research/features and resolve actual evidence via `ImportStore::resolve`. They
must obtain economics and the original plan from the executor-attested
`VerifiedHolding` boundary and construct its `holding_` calculation binding.
The service's existing prediction request is entry-specific; it does not invent
position quantities, acquisition costs, remaining deadlines or holding advice.

`HttpUniverseSource::with_bearer` reads the authenticated universe route.
`AutomaticInputs::new` shares the configured input-service bearer only when the
universe URL has the same HTTPS origin. A separately hosted licensed export can
use its own source credential. No token is put into a URL; existing deployment
HTTPS/DNS/redirect restrictions remain in force.

## Invariants

Deployment owns sources, reviewers, licenses, calendars, funding records, broker
capabilities, skills, original source comparison and model approval. Absent data
or evidence blocks the corresponding operation. Model fitting and diagnostics
remain offline and cannot activate trading. The server does no live-provider
fetch, order submission, broker reconciliation, schema migration or paid API call.
License retention and real prospective/independent evidence remain mandatory.

## Side Effects

Explicit import/review/output commands write private synchronized immutable files.
Evaluation durably consumes a sealed interval before scoring. Pruning removes
expired licensed source content and keeps non-content identity/hash tombstones.
Explicit `serve` binds loopback and runs bounded workers until shutdown; it is an
operator action and is not started by validation or model fitting. No such
deployment startup is required to implement or review these changes.

## Verification

```text
cargo test -p trady-application model_lab::tests --locked
cargo test -p trady-adapters inputs_service::tests --locked
cargo test -p trady-cli --test inputs_model_pipeline --locked
```

The CLI integration test runs real fit/evaluate/import/review/offline-infer
commands against unique `/tmp/trady-inputs-worker-*` directories, reads the actual
service DTOs and verifies that production rejects its explicitly marked TEST ONLY
model. Adapter tests exercise authenticated loopback HTTP, strict bounded reads,
expiry, retention, immutable evidence, DST/special-close calendars and source
independence. These tests establish implementation behavior, not real financial
calibration, prospective profitability, live readiness or a production data license.
