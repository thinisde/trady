# Trady adapters

## Overview

Fixture providers and shared in-memory execution state for offline checks, plus
explicitly configured provider transports.
The source tree also groups automatic research inputs, point-in-time universe
ingestion, approved-artifact native inference and Telegram delivery state.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): clocks, calendar, broker, quotes and memory
  repositories and production component contracts.
- [Offline native training](./examples/README.md): research-only Perpetual fitting,
  bounded tuning on chronological training folds, calibrated binary prediction
  and separate chronological holdout scoring.

## Interface

- [Optional intelligence adapters](./src/intelligence/README.md): bounded
  normalized imports, SEC ownership XML, sequence-checked displayed books,
  in-memory research replay and optional read-only tools.
- `[src/api_health.rs:run]`, `PROVIDERS` and `HealthCheck`: explicit offline
  metadata or bounded online GET diagnostics with sanitized status and timing;
  [operator guide](../../docs/api-health.md).

- `[src/credentials.rs:CredentialStore]`: private named secrets and environment
  overrides shared by the CLI and provider adapters; [setup](../../docs/credentials.md).

- `[src/finbert/mod.rs:FinBertClient]` and `FinBertConfig`: explicitly enabled,
  bounded self-hosted sentiment HTTP adapter; see the [v1 contract](../../docs/v1/finbert/README.md).

- [Financial adapters](./src/financial/README.md): fixed-route provider reads and
  reviewed offline daily-bar imports through `[src/financial/mod.rs:FinancialClient]`.
- [OpenCode Go contract](./src/opencode/README.md): authenticated model calls and
  approved skill/memory tools. `[src/opencode/llm.rs:OpenCodeGoConfig::default]`
  selects `deepseek-v4.1-flash`; credential loading supplies no model override.
- `[src/sessions.rs:MemorySessionRepository]`: scoped session fixtures with
  generation/revision fencing.
- [X and news contract](./src/news/README.md): reviewed read-only sources, bounded
  HTTP, X polling/lookup, RSS/Atom and optional Alpha Vantage.
- [Automatic input contract](./src/autonomous/README.md): `AutomaticInputs`
  composes HTTP packets, screening, authoritative limits and persisted native advice;
  `MarketInputs` supplies separate executor market reads.
- [Trusted input service](./src/inputs_service/README.md): reviewed immutable
  provider documents, explicit calendars and scenario-bound predictions.
- [Research jobs](./src/research_jobs/README.md): scoped information tools,
  source collection and durable sentiment automation.
- [Weekly helpers](./src/weekly_lab/README.md): bounded pinned offline tool
  execution. `[src/shadow.rs:ShadowSource]` marks real/fixture provenance;
  `ShadowBroker` records observations and proposals without broker mutations.
- [Universe contract](./src/screening/README.md): `HttpUniverseSource` fetches an
  explicitly configured constituent/classification export with provenance and quotas.
- [Native advisor contract](./src/perpetual/README.md): `NativePerpetualAdvisor`
  loads deployment-supplied immutable artifacts and computes advisory probabilities.
- [Telegram contract](./src/telegram/README.md): fixed-origin Bot API transport and
  bot-bound locked durable inbox/offset/outbox state.

- [Cargo manifest](./Cargo.toml): package `trady-adapters`, default library
  `trady_adapters`, workspace metadata/lints and pinned `perpetual = "=3.0.0-rc.2"`.
- [Trading 212 contract](./src/trading212/README.md): separate information and
  executor HTTP clients, plus paced `AutonomousTrading212` observations/submissions.

## Invariants

- `[src/api_health.rs:run]` reports no entry authorization or collection-setting
  changes. `[src/api_health.rs:probe]` makes no transport call offline, leaves X
  behind the existing cost gate and supports yfinance only as offline imports.
- `[src/intelligence/book.rs:SequenceBookBuilder]` bounds instrument/venue
  tracks, displayed depth and history; recovery still yields partial depth.
  `[src/intelligence/tools.rs:IntelligenceTools::requires_ephemeral_session]`
  requires in-process conversation storage for enabled intelligence tools.

- `[src/lib.rs:FakeBroker::new]` requires paper snapshots.
  `[src/lib.rs:MemoryExecutionRepository::register_paper_account]` requires
  `paper_` identifiers; `ttl`/`check` enforce 1–300 second leases and exact
  unexpired fences. `save_reconciliation` requires Paper snapshots for the fenced
  account, nonfuture and no older than 60 seconds. New reservations clear recovery;
  matching immutable replays are checked before current recovery/flatness gates.
- `[src/credentials.rs:CredentialStore::read_file]` requires bounded strict-v1
  UTF-8 regular files with private Unix permission bits. Ownership is unchecked
  and symlinks are followed. `resolve` gives current environment values priority,
  including blank values masking file fallback.
- `[src/shadow.rs:validate_observation]` requires valid shadow-account snapshots
  and real/fixture provenance. `ShadowBroker::record_proposal` preserves the whole
  immutable envelope. Suppressed broker operations return `Unsupported` only
  after successful local recording; they do not create fills or acknowledgements.
- `[src/trading212/mod.rs:Trading212Config::validate]` binds Trading 212 access to
  an explicit demo/live environment, account and currency. `submit_bound_before`
  retains request/account/currency/`STOCK` metadata guards. The application
  separately owns recovery, readiness, policy and protection; low-level transport
  grants no policy or deployment authority.
- `[src/perpetual/mod.rs:NativePerpetualAdvisor::from_approved_artifacts]` accepts
  deployment-supplied pinned model bytes and approval manifests. This crate ships
  no trained deployment model.
- `[src/weekly_lab/mod.rs:HelperLab::verify_candidate]` distinguishes full formula
  verification from arithmetic-only proof. `[src/weekly_lab/runner.rs:HelperCommand]`
  uses deployment-owned pins and Linux resource bounds; containment/isolation
  remains a deployment responsibility.

## Side Effects

`[src/api_health.rs:run]` loads the credential store even offline; eligible online
probes perform GETs through `SafeHttp`. `[src/intelligence/mod.rs:JsonFeatureSource]`
reads local normalized imports. The displayed-book builder mutates bounded
memory; `[src/intelligence/mod.rs:MemoryIntelligenceRepository]` has explicit
purging without a configured collection capacity. Research-tool effects depend
on the supplied repository and base executor.

`[src/opencode/llm.rs:OpenCodeGoProvider]` performs model HTTP calls;
`[src/opencode/runtime_tools.rs:SkillBundle::load]` reads approved skill files
and `RuntimeTools` reads attributed in-session memory. Streaming text callbacks
may precede a later provider failure.

`[src/news/mod.rs:SourceClient]` performs read-only provider HTTPS requests and
resolves named credentials. `[src/news/x_meter.rs:MeteredHttp::get]` reserves and
settles the durable X cost ledger. `[src/financial/mod.rs:FinancialClient::fetch]`
invokes its credential, transport and receipt-clock ports; observations do not
write broker or execution state.

`[src/credentials.rs:CredentialStore::load]` reads environment and configured files;
`initialize` exclusively creates a private template file. FinBERT, universe and
automatic-input adapters perform explicitly configured HTTP calls.
`[src/autonomous/mod.rs:AutomaticInputs::candidate]` writes screening, bound
calculation, dossier, context, assessment and review through the supplied
repository before final candidate validation, without an enclosing rollback.
Native inference itself computes in memory.

`[src/inputs_service/store.rs:ImportStore]` creates private import/review/index/
tombstone files, locks writers and prunes expired content. Review and index
publication can partially complete. `[src/inputs_service/http.rs:HttpServer]`
opens a loopback listener and bounded worker pool.
`[src/research_jobs/mod.rs:ResearchTools::execute]` invokes information backends;
registered-source helpers read providers and persist success/failure state.
Earlier effects can survive later failure or cancellation.

`[src/weekly_lab/runner.rs:HelperCommand::run_linux]` spawns fixed `prlimit` and
configured helpers; `kill_group` invokes fixed `/bin/kill`. Hashed artifacts may
persist before later validation fails. `[src/shadow.rs:ShadowBroker::write_once]`
creates and synchronizes private recording files; failed writes can leave artifacts.

`[src/telegram/mod.rs:TelegramClient]` performs authenticated Bot API requests;
`[src/telegram/spool.rs:StateStore]` locks and persists private plaintext delivery state.

`[src/lib.rs:MemoryExecutionRepository]` and fixture clocks/brokers mutate shared
process memory; `SystemClock::now` reads UTC. `[src/sessions.rs:MemorySessionRepository]`
mutates shared sessions/leases using UTC; saves renew expiry and the caller's
revision. These repositories are not durable.
`[src/trading212/http.rs:Http::request]` sends authenticated broker GETs and guarded
POST/DELETE requests, using the live origin only under explicit `Live` configuration.
`Http::pace` may sleep. HTTP acknowledgments do not establish filled/protected
positions or live deployment acceptance.

The [offline native training executable](./examples/README.md) performs CPU
fitting and exclusively writes local fit/tuning/holdout claims, native model JSON,
candidate metadata and diagnostic reports. Claims are reserved before fitting or
evaluation; interrupted runs can leave partial artifacts that prevent reuse of
the same output directory. It does not promote models or call the broker.

## Verification

Run `cargo test --workspace --locked` from the repository root.
For scoped verification, run `cargo test -p trady-adapters --locked`.
