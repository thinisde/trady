# Verified input service

## Overview

This component supplies the concrete market/research HTTP service consumed by
the autonomous adapter. It serves imported provider/operator documents after a
separate local review. It never treats an LLM's self-reported verification, a
model file, a syntactically valid dossier or a copied article as independent
financial evidence. See the [operator guide](../../../../services/inputs/README.md).

## Interface

- `[mod.rs:InputsServiceConfig]::{load,validate,account}`, `SourcePolicy`,
  `ReviewerPolicy`, `ServiceAccount` and `ModelDeployment` define deployment-owned
  sources, identities, rights, account/currency/formula/skill bindings and model pins.
- `[mod.rs:SourceImport]`, `ImportPayload`, `ImportKind`, `SourceReview`,
  `EvidenceArtifact` and `ForecastImport` define strict, versioned import/review
  contracts. Payloads reuse the actual `SessionPacket`, `Quote`, `CostsPacket`,
  `AnalystPacket`, `UniverseDocument` and `ForecastObservation` Rust types.
- `[store.rs:ImportStore]::{open,config,import,staged,review,resolve,latest,prune_expired}`
  stage immutable documents, verify exact review/dependency bindings, select
  current observations and remove expired licensed content.
- `[calendar.rs:CalendarDocument]::{validate,active_session,deadline}`, `CalendarSession`
  and `ClosedDate` require explicitly complete exchange date coverage.
- `[mod.rs:InputsService]::{open,config,quote,costs,calendar,session,research,universe,
  forecast_observation,predictions,respond}` expose validated reads and the
  scenario-bound model pipeline. `forecast_observation` is also available to an
  independently authorized holding-context builder; it supplies no holding economics.
  The public `store` field also exposes local staging/review operations.
- `[http.rs:HttpServer]::{bind,local_addr,serve_until}` and `HttpServiceResponse` expose
  bounded HTTP/1.1 serving, controlled by an atomic shutdown flag.
- `[http.rs:InputsService::respond]` dispatches GET `/session/{account}`,
  `/quote/{instrument}`, `/costs/{instrument}`, `/research/{account}` and
  `/universe/SP500`, plus POST `/predictions/{account}`. Direct method calls
  perform no bearer authentication; the socket receiver enforces it.

## Invariants

- `[store.rs:ImportStore::review]` grants served-read authority only after an independent,
  allowlisted operator records exact import/raw-content hashes. Provider names, source
  HTTPS hosts, rights expiry, retention, original availability, raw hashes and
  typed payload scope/chronology must match the deployment registry.
  `[store.rs:ImportStore::staged]` remains a local unreviewed-document read.
- `[store.rs:ImportStore::validate_review]` checks a privileged local attestation,
  not a cryptographic provider signature
  or automatic factual verification. The reviewer must check actual originals,
  mapping, source ownership, permissions and original lineage. A JSON boolean
  supplied by a model cannot provide that authority. The HTTP credential exposes
  no import, review, model-approval or broker-write route.
- `[store.rs:ImportStore::import]` rejects content, observation/availability or
  expiry changes for an existing ID and rejects ID reuse after retention deletion.
  `[store.rs:ImportStore::resolve_inner]` requires current, bounded, acyclic,
  separately reviewed dependencies. Derived retention cannot exceed any dependency.
- `[validate.rs:quote]` and `costs` require original observations at most
  60 seconds old, positive executable prices, uncrossed spreads and current FX. `[store.rs:ImportStore::latest]`
  orders by observation then availability time; conflicting payloads fail when
  both times tie. Failure resolving the newest entry does not select an older one.
  `[mod.rs:InputsServiceConfig::validate]` requires one account currency per
  deployment because the existing costs route has no account segment. Cost imports
  and reads require both fee and uncertainty amounts in that configured currency;
  FX must convert into it at a positive rate using a current original observation.
- `[calendar.rs:CalendarDocument::validate]` enumerates sessions and explicit
  closed dates, including supplied DST shifts, special closes and holidays. Missing dates are errors. Entry counts as
  holding session one. No weekday or wall-clock-day fallback supplies a deadline.
- `[validate.rs:InputsService::validate_research]` requires every dossier record
  to match a separately reviewed artifact and content hash.
  Skill hashes and artifact references resolve against deployment-approved skills.
  Independent sources require different reviewed original reporting groups and
  different original lineages; common-owner/copy/repost URLs add no corroboration.
  A maximum matching across groups and lineages counts a set distinct on both
  dimensions, including overlapping secondary-source copies.
- `[store.rs:ImportStore::index_entries]` caps the active read index independently
  from lifetime review history.
  Review removes expired entries and checks capacity before publishing either
  review or index; lookup also enforces active and total-scan bounds. Explicit
  pruning streams expired legacy indexes so cumulative history cannot permanently
  exhaust lookup capacity. Content deletions retain immutable identity tombstones.
- `[mod.rs:InputsService::open]` reads a fixed operator model path and exact
  content pin. `[validate.rs:InputsService::validate_model_evidence]` requires
  non-test candidates, independent evaluation, active approval and
  resolved dataset-verification, prospective-shadow and owner-approval evidence.
  Generic model JSON is not an import payload. Inference recomputes the exact
  authoritative Rust calculation and binds account, market, costs and calendar.
- `[mod.rs:InputsServiceConfig::validate]` requires a loopback bind and bounded
  workers/body limits. `[http.rs:HttpServer::bind]` requires a 32–256-byte
  ASCII-graphic bearer token; `receive` and `send` bound
  headers, request/response bytes and total read/write deadlines. Duplicate
  headers/JSON fields, transfer encoding, query/escaped paths and redirects are
  unavailable. Receive/send deadlines cover socket operations; inference and
  queue residence have no deadline here. Deploy behind an approved TLS reverse
  proxy for the HTTPS client.
- `[validate.rs:universe]` requires a complete, current SP500 import of 1–2,048
  members observed within seven days. `[validate.rs:InputsService::validate_forecast]`
  resolves reviewed origins for forecast/signal lineage; payload validation
  bounds forecasts to 1–24 features.

## Side Effects

`[mod.rs:InputsServiceConfig::load]` reads configuration and resolves relative
store/model paths against its parent. `[mod.rs:InputsService::open]` reads the
optional pinned model and current UTC time. `[store.rs:ImportStore::open]`
creates private directories and applies Unix `0700` permissions.
Imports/reviews/index/tombstones use synchronized immutable files with `0600`
permissions; publication uses an atomic hard link. `[store.rs:ImportStore::review]`
cleans expired indexes, then publishes review and index separately; a failure can
leave a review without its index. `[store.rs:ImportStore::prune_expired]` deletes
expired raw/typed imports and expired index entries, synchronizes their directories,
and preserves identity/hash tombstones. Imports, reviews and cleanup share an
[OS file lock](https://doc.rust-lang.org/1.93.0/std/fs/struct.File.html#method.try_lock)
from `[store.rs:ImportStore::lock_writes]`, released when its handle closes,
including process termination. Readers do not acquire that lock. Reads inspect
bounded private files and UTC time; response errors omit source bodies/secrets.
`[http.rs:HttpServer::bind]` opens a nonblocking loopback TCP listener;
`serve_until` starts a fixed worker pool with a queue twice the worker count.
Queue saturation produces a best-effort 503; shutdown drains queued work and joins
workers.
No provider HTTP request, paid API, database migration or broker call occurs here.

## Verification

Run `cargo test -p trady-adapters inputs_service::tests --locked`.
Focused regressions cover copied secondary lineages/ownership overlaps, active
capacity enforcement and recovery after cumulative/legacy retention cleanup,
writer serialization, account-currency mismatch and FX direction/freshness.
Run `cargo test -p trady-cli --test inputs_model_pipeline --locked` for the actual
CLI ingestion/training/evaluation/serving/offline-inference path. Every generated
observation, license, source, review and model in these tests is TEST ONLY;
production approval/loading is explicitly rejected for those artifacts.
