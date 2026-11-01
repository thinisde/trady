# Adapter boundaries

## Overview

[lib.rs](./lib.rs) implements application ports over explicit fixture data.
Its public modules also expose configured provider transports, automatic research
inputs, native advisory inference and Telegram delivery state.

## Interface

- [Optional intelligence adapters](./intelligence/README.md): bounded normalized
  JSON imports, SEC Forms 3/4/5 XML parsing, sequence-checked displayed books,
  immutable memory replay storage and nine optional read-only research tools.

- `[api_health.rs:run]` and `PROVIDERS` expose explicit metadata-only or bounded
  GET diagnostics over fixed API routes. `HealthCheck` contains sanitized status,
  timing, record counts and credential availability; see the
  [operator guide](../../../docs/api-health.md). No database or broker port is used.

- [Financial adapters](./financial/README.md): `[financial/mod.rs:FinancialClient]`
  implements the application `FinancialProvider` port with fixed routes and central
  credential lookup; `offline_page` handles explicit yfinance research imports.

- [Weekly lab](./weekly_lab/README.md): bounded pinned offline helpers.
- `[shadow.rs:ShadowSource]`: explicit `Real { market_data_hash }` or
  `Fixture { description }` provenance.
- `[shadow.rs:ShadowBroker]`: `new`, `update`, `source`, `record_proposal` and
  `observed_at`; recording-only `ExecutionBroker::observe` and
  `BrokerExecutionPort` implementations without a network client or real broker handle.

- [Verified inputs service](./inputs_service/README.md): immutable provider imports,
  separate operator reviews, explicit calendars and bounded authenticated HTTP
  reads for the actual autonomous DTOs. Model bundles require deployment pins and
  resolved approval evidence; HTTP exposes no import or broker mutation route.
- [Research automation](./research_jobs/README.md): scoped information tools,
  registered-source collector composition and durable sentiment jobs.

- `[credentials.rs:CredentialStore]`: bounded strict private-file parsing,
  environment overrides, redacted secret values and metadata-only status.
  Public methods are `load`, `from_path`, `optional`, `required` and `status`.
- `[credentials.rs:SecretValue]`: redacted `Debug`, explicit `expose`/`into_inner`
  access and no serialization implementation. `[credentials.rs:CredentialSource]`
  identifies `Environment`, `File` or `Missing`; `[credentials.rs:CredentialStatus]`
  exposes `name`, `configured` and `source` without secret values.
- `[credentials.rs:file_path]`, `initialize`, free `required`/`optional`, and
  `DEFAULT_CREDENTIALS_FILE`, `CREDENTIALS_FILE_ENV`, `CREDENTIALS_TEMPLATE`,
  `KNOWN_CREDENTIALS`: central path selection, exclusive initialization,
  per-call loading and the status catalog.

- [Automatic inputs](./autonomous/README.md): `TradingInputsConfig`, packet DTOs
  including `PredictionRequest`, and `AutomaticInputs::{new, with_ports, with_clock}`.
  Its `TradingInputs` implementation combines configured HTTP observations,
  screening, authoritative limits and persisted native advice. `MarketInputs`
  exposes separate session/quote/cost reads without research or model state.
- [Universe ingestion](./screening/README.md): `ApprovedUniverseConfig`,
  `UniverseDocument` and `HttpUniverseSource` implement approved point-in-time
  universe fetching with instance-local throttling.
- [Native Perpetual](./perpetual/README.md): `ApprovedArtifact`,
  `NativePerpetualAdvisor` and pinned runtime/format constants; deployment-owned
  immutable classifiers implement the application advisory port.
- [Telegram](./telegram/README.md): token/identity/client/failure types,
  `MESSAGE_UNITS`/`split_message`, and `spool` state records with `StateStore`.

- `[finbert/mod.rs:FinBertConfig]`, `FinBertClient`: `SentimentProvider` implementation
  for pinned self-hosted CPU classification; [contract](./finbert/README.md).

- [OpenCode Go](./opencode/README.md): provider transport and approved runtime tools;
  `[opencode/llm.rs:OpenCodeGoConfig::default]` uses `deepseek-v4.1-flash`.
  [Setup](../../../docs/opencode/README.md).
- `[sessions.rs:MemorySessionRepository]`: owner/account/policy-scoped session
  fixtures implementing `SessionRepository::{is_ephemeral, create, load, list,
  acquire, save, release}`, with generation and revision checks and explicit
  in-process conversation storage.
- [News source contract](./news/README.md): `SourceClient`, safe HTTPS transport,
  RSS/Atom/news API normalization and X identity/timeline/lookup adapters.

- `[lib.rs:FakeClock]`: `new` and shared time with checked `advance`;
  `[lib.rs:SystemClock]`: UTC wall time. Both implement the application `Clock` port.
- `[lib.rs:FakeCalendar]`: public `sessions` and `ExchangeCalendar` lookup from
  explicitly supplied sessions; absent sessions remain absent.
- `[lib.rs:FakeBroker]`: `new`, `set_snapshot`, validated paper snapshot reads
  and client-intent lookup through the application broker ports.
- `[lib.rs:FakeQuotes]`: public `quotes` map and `QuotePort` lookup by instrument.
- `[trading212/mod.rs:Trading212Reader]`, `Trading212Executor` and reexported
  `AutonomousTrading212`: typed broker information, guarded execution and paced
  execution observations; see the [provider contract](./trading212/README.md).
- `[lib.rs:MemoryExecutionRepository]`: account registration, leases, reconciliation,
  paper reservations and outbox-count inspection. Public methods are `new`,
  `register_paper_account` and `outbox_count`; `ExecutionRepository` supplies
  `acquire`, `renew`, `unresolved_intents`, `save_reconciliation`,
  `latest_reconciliation` and `reserve_paper_intent`. Clones share process memory.
  The port traits, session types and domain values are imported contracts, not
  locally defined or reexported types.

## Invariants

- `[api_health.rs:run]` always reports `entry_authorized=false` and
  `collection_settings_changed=false`. `[api_health.rs:probe]` makes no transport
  calls offline, leaves X behind its existing cost gate and treats yfinance as
  offline-import-only. Missing required credentials prevent diagnostic requests;
  reports omit raw response bodies and credential values.
  `[api_health.rs:probe]` assigns `gdelt_response_invalid` specifically when a
  GDELT response is classified `invalid_response`. GDELT invalid/rate-limited
  diagnostics receive a suggested 60-second backoff when no parsed retry delay
  is available; other classified provider failures retain their own status.
  A valid empty article array remains `empty_data`.

- `[weekly_lab/mod.rs:HelperLab]` accepts candidate parameters as data;
  `[weekly_lab/runner.rs:HelperCommand::run_linux]` clears inherited environment.
  `[weekly_lab/mod.rs:HelperLab::verify_candidate]` distinguishes full formula
  verification from arithmetic-only scope.
- `[shadow.rs:validate_observation]` requires valid Shadow snapshots, `shadow_`
  accounts and nonfuture observations. Real sources require a valid market-data
  hash; fixture descriptions have 1–200 bytes. `ShadowBroker::update` preserves
  account identity and nondecreasing observation time.
- `[shadow.rs:ShadowBroker::record_proposal]` preserves the complete immutable
  envelope, including source, snapshot and time, across retries/restart.
  `[shadow.rs:BrokerExecutionPort for ShadowBroker]` returns `Unsupported` after
  successful suppression recording; recording failures return their own errors.
  These operations create no broker acknowledgement or fill.
- `[shadow.rs:ShadowBroker::new]` requires a nonsymlink recording directory.
  `write_once` checks existing records for regular nonsymlink files, exact bytes
  and a 4 MiB ceiling. Unix additionally requires group/other permission bits
  clear for the directory and existing records; that size ceiling does not guard initial writes.

- `[credentials.rs:CredentialStore::read_file]` requires a regular UTF-8 file,
  private Unix group/other permission bits, at most 262,144 bytes and strict
  configuration version 1. Only an absent default file is tolerated; an explicit
  path must exist. Loading does not reject symlinks or verify ownership.
- `[credentials.rs:CredentialStore::resolve]` gives environment values precedence;
  a blank environment value masks file fallback. Names allow 1–96 uppercase ASCII
  letters, digits or `_`; values allow at most 65,536 bytes and no NUL.
  `KNOWN_CREDENTIALS` is a status catalog, not a name allowlist.
  `[credentials.rs:CredentialStore::status]` reports known and file-defined names,
  rather than discovering arbitrary environment-variable names.
- `[sessions.rs:MemorySessionRepository::create]` validates scope/limits and
  requires version 1, revision 0 and a valid session ID unique across scopes in
  this shared repository map. Independently constructed repositories have separate maps.
  `acquire` requires a valid worker, expired lease and TTL 1–300 seconds, then
  increments generation. `save` checks scope, worker, generation, unexpired lease
  and revision before incrementing revision. `release` checks worker/generation
  only. `save` does not revalidate session schema/limits. These session leases
  use UTC wall time rather than `FakeClock`.
- `[autonomous/mod.rs:AutomaticInputs::candidate]` requires scoped provider inputs,
  active authoritative limits and validated native advice before returning a
  candidate. `[perpetual/mod.rs:NativePerpetualAdvisor::from_approved_artifacts]`
  consumes deployment-supplied artifacts; an empty registry provides no usable model.

- `[finbert/mod.rs:FinBertConfig::validate]` requires config v1, a 40-character model
  revision, a 1–60 second timeout, and root HTTPS or loopback/`finbert` HTTP without
  credentials/query/fragment. `[finbert/mod.rs:FinBertClient::new]` requires explicit
  enablement and disables redirects/proxies. `predict` validates bounded requests,
  HTTP 200, a 64 KiB response, exact model/item/hash identity and valid probabilities;
  application callers separately reject truncation and enforce source permissions.

- `[opencode/llm.rs:OpenCodeGoConfig::validate]` requires HTTPS or loopback HTTP;
  credentials stay outside sessions and errors. Native tools are dispatched only
  after complete provider output and local schema validation.
- `[opencode/runtime_tools.rs:SkillBundle::load]` requires exact deployment hashes,
  valid catalog/frontmatter/taxonomy and safe local paths.
- `[lib.rs:FakeBroker::set_snapshot]` preserves account identity and paper mode;
  ambiguous order lookup fails and capabilities remain unknown.
- `[lib.rs:FakeClock::advance]` permits negative advances and checks duration/time
  overflow. `[lib.rs:FakeCalendar::session]` selects the first
  `opens_at <= now < closes_at` session; `[lib.rs:FakeQuotes::quote]` does not
  independently validate quote freshness.
- `[lib.rs:MemoryExecutionRepository::register_paper_account]` requires account
  identifiers beginning with `paper_`.
- `[lib.rs:ttl]` bounds leases to 1–300 seconds. Acquisition increments the fencing
  generation, rejects time overflow and requires reconciliation after takeover.
  `[lib.rs:check]` rejects stale/expired fences for writes.
- `[lib.rs:MemoryExecutionRepository::save_reconciliation]` requires snapshots
  for the fenced account in Paper mode, nonfuture and no older than 60 seconds
  through `trady_policy_fresh`. It rejects
  regressing snapshots, currency changes, reused snapshot IDs with different content,
  and unknown resolution IDs before mutating intents. Outbox keys deduplicate snapshots.
- `[lib.rs:MemoryExecutionRepository::reserve_paper_intent]` supports prepared buy
  fixtures only, requires fresh flat reconciliation in the current generation, and
  blocks a consumed session or unresolved intent. Replays with the same immutable
  intent fields return false; changed fields fail. Replay comparison precedes
  current recovery, flatness and consumed-session checks. Every successful new
  reservation clears recovery, requiring reconciliation before another reservation.
- Memory persistence ends when all repository handles are dropped. It is not durable storage.
- `[trading212/http.rs:Http::build]` fixes authenticated production requests to
  HTTPS origins with redirects, proxies and automatic retries disabled.
  `[trading212/mod.rs:Trading212Executor::submit_order]` verifies account/currency
  binding and stock metadata before sending; ambiguous outcomes require recovery.

## Side Effects

`[api_health.rs:run]` loads the credential store even offline. Eligible online
probes perform at most one transport GET per provider through `SafeHttp` and
measure elapsed time; no database or broker mutation port is used.

`[weekly_lab/runner.rs:HelperCommand::run_linux]` spawns resource-bounded local
processes and persists hashed tool artifacts, including fixed `prlimit`/`kill`
programs as documented by the child contract. `[shadow.rs:ShadowBroker::write_once]`
exclusively creates and fsyncs local records; retries validate exact envelopes and
reject changed or tampered content. Construction does not record the initial
observation; `update` records before replacing memory. Failed writes/syncs can
leave artifacts without rollback.

`[credentials.rs:CredentialStore::load]` reads environment and configured files;
free `required`/`optional` reload per call. `[credentials.rs:initialize]` exclusively
creates a template file with Unix mode `0600`, requiring an existing parent and
refusing overwrite; a failed write can leave the newly created file.

Explicit `[finbert/mod.rs:FinBertClient::predict]` posts supplied text to the configured
model service and reads a bounded response; it performs no database or broker writes.

Explicit OpenCode calls perform authenticated model HTTP requests; approved skill
and memory tools read local instruction files or scoped session records.

Clocks, fixture snapshots, account leases, intent state, reconciliation and outbox
entries can change in shared process memory. `SystemClock` reads real UTC time.
`[sessions.rs:MemorySessionRepository::acquire]` updates worker/generation/expiry;
`save` replaces shared session state, increments the caller's revision and renews
expiry; `release` expires the lease. Clones share process memory and use real UTC.
The explicitly constructed Trading 212 clients make broker network calls;
`Trading212Executor` and `AutonomousTrading212` expose guarded order mutation,
and explicit pacing may sleep. `[autonomous/mod.rs:AutomaticInputs::candidate]`
writes bound calculations and research records through its repository before
final validation, without an enclosing rollback. `[telegram/spool.rs:StateStore]` locks and writes private
plaintext delivery state. Native Perpetual inference computes in memory.

## Verification

Targeted checks: `cargo test -p trady-adapters --test weekly_helpers --test shadow_recording --locked`.

Run `cargo test -p trady-adapters --locked` from the repository root.
Workspace verification is `cargo test --workspace --locked`.
