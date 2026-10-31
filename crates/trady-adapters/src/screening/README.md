# Automatic S&P 500 universe ingestion

## Overview

[mod.rs](./mod.rs) fetches an explicitly approved constituent/classification JSON
export at runtime. It supplies the point-in-time universe to the
[application screener](../../../trady-application/src/screening.rs) and the
[domain gates](../../../trady-domain/src/screening.rs), following the
[sector workflow](../../../../docs/v1/sector-workflow.md). It contains no built-in
constituent list and grants no trading authority.

## Interface

- `[mod.rs:ApprovedUniverseConfig::validate]` checks the trusted deployment configuration.
  `[mod.rs:HttpUniverseSource::new]` selects the production `SafeHttp` transport;
  `with_bearer(Option<String>)` supplies or clears an out-of-band header credential. Automatic inputs
  share their service credential only for the same HTTPS origin.
  `with_transport` supports controlled offline transport tests.
  [universe.example.json](./universe.example.json) is a valid disabled template.
  Replace its placeholder endpoint/provider/edition, update `allowed_hosts`
  to match the new endpoint host exactly, and record reviewed storage rights
  before explicitly enabling it. No approval is implied by the example.
- `[mod.rs:HttpUniverseSource::fetch_universe]` implements the application
  `UniverseSource` port and retrieves one complete JSON document;
  `retry_at()` exposes provider backoff. Every request fetches provider data.
- `[mod.rs:UniverseDocument]` is the strict v1 wire contract. Required top-level fields are
  `schema_version: 1`, `index_id: "SP500"`, `complete: true`, `snapshot_id`,
  `classification_snapshot_id`, `provider`, `taxonomy_edition`, `as_of`,
  `available_at`, `expires_at`, and `members`.
- `[crates/trady-domain/src/screening.rs:UniverseMember]` has `issuer_id`, `share_class_isin`, `symbol`, `listing_mic`,
  `currency`, `membership: {from, until}`, and `classification: {sector_code,
  industry_group_code, effective: {from, until}, observed_at}`. Timestamps are UTC
  RFC 3339; interval ends are exclusive and nullable. `issuer_id` is a stable
  provider-resolved issuer/CIK reference. ISIN identifies the actual share class.
- The response cannot supply source rights or source approval. The adapter binds
  configured rights, source ID, exact URL, fetch time, `[mod.rs:PARSER_VERSION]` and payload
  SHA-256 to the returned universe. Unknown and duplicate JSON fields fail parsing.

Use `[crates/trady-application/src/screening.rs:run_screening]`, called as
`run_screening(&request, &mut source, &mut data, &mut repository, now)`, from
`trady_application::screening`. `ScreeningRequest` binds trusted owner/run IDs,
an explicit `BTreeSet<InstrumentId>` allowlist and `ScreeningPolicy` thresholds.
Implement `ScreeningDataPort` for verified `BrokerStockSupport`, current
`MarketObservation` and evidence-backed `PriorityObservation` inputs. Implement
`ScreeningRepository::store_screening_snapshot` to commit immutable full snapshots
to the production store before using the result. Identical owner/run retries must
deduplicate; changed retries must fail. The result nominates at most five research
candidates and two disjoint same-group peer controls. It never authorizes an entry.
`ScreeningSnapshot.failure` distinguishes `UNIVERSE_UNAVAILABLE`,
`SOURCE_STORAGE_NOT_PERMITTED`, `OBSERVATIONS_UNAVAILABLE`, and `INVALID_INPUTS`;
a completed pass can separately contain domain blockers or no eligible candidates.

## Invariants

- `[mod.rs:ApprovedUniverseConfig::validate]` checks the deployment-supplied
  HTTPS JSON export URL, exact hosts, provider and taxonomy edition. Fetching
  additionally requires explicit enablement and reviewed `SourceRights` with
  storage permission and retention. Owner review and source entitlement are
  deployment obligations; validation does not contact the configured URL. No vendor,
  URL, approval, credentials, or redistributable membership dataset is supplied.
  A licensed provider or an owner-approved normalizing export must implement the
  documented JSON contract; arbitrary vendor JSON/CSV is not automatically mapped.
  Even disabled configurations require a 60–86,400 second minimum interval,
  daily quota 1–10,000 and retention 1–8,760 hours.
- `[mod.rs:HttpUniverseSource::with_bearer]` accepts `None` or a nonempty token
  of at most 8,192 bytes without ASCII control bytes.
- `[mod.rs:HttpUniverseSource::fetch_universe]` uses the existing
  [protected news HTTP transport](../news/http.rs): public pinned DNS, HTTPS,
  no proxies, 15-second HTTP timeout, and a 2 MiB body limit. Redirects are disabled.
  URL credentials, query strings and fragments are forbidden. No credential store
  or environment secret is read. HTTP 304 cannot replace a full point-in-time feed.
- `[mod.rs:HttpUniverseSource::fetch_universe]` requires matching configured
  provider/taxonomy, at most 2,048 members, valid completeness, bounds,
  effective snapshot availability and expiry must be valid. Future memberships
  remain in the input for explicit exclusion by domain gates. Nothing is stamped
  current merely because it was downloaded now.
- [crates/trady-domain/src/screening.rs:screen_universe](../../../trady-domain/src/screening.rs)
  checks ISIN checksum, listing/currency identity,
  exclusive effective dates, active duplicates, effective 11/25 GICS pairs,
  explicit owner permission, and fresh verified broker `STOCK` support. Metadata
  presence alone must not be promoted to broker verification.
- `[crates/trady-domain/src/screening.rs:screen_universe]` requires quote,
  spread, notional liquidity/participation, all-in costs, order minimum,
  fractional affordability, actual exchange session, event clearance and all five
  known `0..4` priority components are required. Priority inputs must come from a
  trusted verifier/calculator with current evidence and consistent peer observation
  basis; downloaded documents cannot supply them. Ranking is score, coverage,
  spread, liquidity, then stable instrument ID. It is a research priority, not a
  return forecast, probability or entry-confidence score.
- `[crates/trady-application/src/screening.rs:run_screening]` persists all supplied
  inputs, exclusions, chosen controls and
  lineage, or a sanitized failed attempt; storage failures propagate. Forbidden
  source content is omitted. Production storage must enforce retention and dependent
  purge; this module does not change storage tables or the research tool schema.

## Side Effects

`[mod.rs:HttpUniverseSource::fetch_universe]` makes one bounded HTTPS GET per
eligible attempt. It records the attempt and consumes UTC-day quota before
transport or parsing, so failures also count. Interval, daily quota and 429/503
backoff are instance-local and do not survive restarts. Integer `Retry-After`
values in 1–86,400 seconds are accepted; other values use 60 seconds. The application calls the supplied snapshot repository. There
are no order submissions, cancellations, filesystem writes, credential reads,
source approval changes, or persistence implementation in this module.

## Verification

Run `cargo test -p trady-domain --test screening`,
`cargo test -p trady-application --test screening`, and
`cargo test -p trady-adapters screening` from the repository root.
[tests.rs](./tests.rs) exercises real adapter parsing through injected HTTP
responses, provenance, host restrictions, scope/time/completeness rejection,
response limits and throttling. Domain/application tests cover membership,
share classes, missing/unsupported broker records, fresh execution inputs,
deterministic ties, limits, explicit empty universes, and immutable persistence.
