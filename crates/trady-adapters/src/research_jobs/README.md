# Durable research automation adapters

## Overview

This component registers bounded information tools and composes existing source
collectors. The Telegram analyst worker supplies PostgreSQL repositories and the
existing self-hosted FinBERT client. Collected observations, source verification,
sentiment classification and entry readiness remain distinct outcomes.

## Interface

- `[mod.rs:ResearchTools]` wraps the approved `RuntimeTools` skill/memory tools with a
  `ResearchInformation` repository. `RuntimeTools::with_research` constructs it.
  Its `base`, `repository` and `sentiment_enabled` fields are public;
  `[mod.rs:ResearchTools::revision]` hashes the base revision and research catalog.
- `[mod.rs:research_tool_specs]` registers `news.search`, `news.read_item`, `sources.health`,
  `sources.lookup`, `profiles.get`, `evidence.compare`, `research.request`,
  `research.get_artifact` and `research.job_status`. `sentiment.assess` and
  `sentiment.read` are advertised when the supplied `sentiment_enabled` boolean
  is true. The composition caller must bind that flag to an enabled approved
  FinBERT revision; the registry does not check provider availability itself.
  These names describe the implemented subset, not every v1 tool.
- `[mod.rs:research_tool_specs]` always registers `financial.snapshot`, `fundamentals.snapshot`,
  `filings.search`, `macro.snapshot` and `prices.features`. Each requires
  `source_id` and `limit` (1–3). The PostgreSQL backend reads stored financial
  context with source, instrument, type and external-model rights checks;
  `ResearchTools::execute` preserves its financial status in `ToolOutput.status`.
  Historical context grants no executable quote or order authority.
- `[mod.rs:collect_registered]` applies the existing source registry, rights, durable
  request/cost gates and watermarks. The worker queues one `COLLECT_SOURCE` child
  per source; each child collects at most two pages and preserves continuation
  gaps. The helper itself visits sources synchronously; child-job scheduling is
  owned by the CLI. `[mod.rs:lookup_registered]` accepts an approved source ID
  and an X post ID. Both helpers validate policy/registry but leave the
  `JobPolicy.enabled` check to their caller.
- [Application jobs](../../../trady-application/src/research_jobs.rs) define typed
  work, policy, provenance/dependency records, syndication clustering and the
  readiness reference. [PostgreSQL storage](../../../trady-storage/src/research_jobs.rs)
  implements the queue, tool backend, packet publication and control getters.
- [Telegram worker](../../../trady-cli/src/telegram/README.md) is the executable
  composition. `telegram worker --config FILE [--once]` uses the analyst role.

The production research/source-verifier service publishes permitted evidence by
calling `[crates/trady-storage/src/research_jobs.rs:PgResearchJobs::publish_verified_packet]`
as `publish_verified_packet(scope, instrument_id,
profile_revision_id, verifications, claims, expires_at, now)`. It must supply real
persisted source item IDs, independently resolved issuer mappings, primary-source
verification references, reporting origins and unresolved ambiguity. This is a
trusted service interface; no LLM tool can publish verification outcomes. Unknown
origins and missing primary confirmation remain blockers. The worker discovers
new packets and durably queues approved sentiment scoring automatically.

`[crates/trady-storage/src/research_jobs.rs:PgResearchJobs::checked_readiness_reference]`
returns an immutable `SentimentReadinessRef` only after claim/dependency checks. Persist that reference
with the exact dossier and plan revision.
`[migrations/0008_verified_holdings.sql:execution.save_trader]`
wrapper calls `research.sentiment_reference_current` in the same PostgreSQL transaction
that accepts a new plan, holding revision or entry attempt. This predicate locks policy, packet and source dependencies
until transaction end; invalidation writers use compatible locks. Reading a
reference through a separate connection cannot replace the transaction check.

## Invariants

- `[crates/trady-application/src/research_jobs.rs:JobPolicy::validate]` requires
  explicit `paper_`, `shadow_` or `live_` account
  namespaces. `JobPolicy.mode` must match the namespace. Owner, account and policy
  predicates are applied to every job, packet, outcome and session lookup. A mode
  label or a queued job confers no execution permission.
- `[mod.rs:research_tool_specs]` rejects extra properties in schemas.
  `[mod.rs:ResearchTools::execute]` also validates
  arguments before backend access. Neither shell commands, SQL, credentials nor
  arbitrary URLs are arguments. Source lookup constructs its supported URL in
  Rust; source text cannot enlarge source or instrument scope.
- `[crates/trady-storage/src/research_jobs.rs:enqueue]` rejects changed work keys.
  Active identical research work coalesces;
  dialogue messages retain distinct update IDs.
  `[crates/trady-storage/src/research_jobs.rs:PgResearchJobs::claim]` uses server-time
  leases to fence stale writers, serialize each owner's queue and allow at most
  three abandoned claims.
  Per-owner daily and pending-job caps are enforced inside the SQL enqueue
  `[migrations/0007_research_jobs.sql:control.enqueue_information_work]` function.
  Collector/API cost limits remain separate from these job caps.
- `[mod.rs:ResearchTools::execute]` checks cancellation/deadline before dispatch
  and after a successful backend return. It returns `DENIED` for unknown tools
  or invalid arguments and masks backend errors as `UNAVAILABLE`. A completed
  backend mutation can precede a `CANCELLED` response; no rollback is provided.
- `[crates/trady-application/src/research_jobs.rs:score_sentiment_job]` obtains
  FinBERT predictions through `FinBertClient` and the existing correlated
  `assess_sentiment` path. Missing/disabled/unavailable providers produce failed
  jobs. No fallback probabilities, neutral classifications or model promotion
  exist. One representative per concrete syndication cluster is scored; all
  underlying source dependencies and ambiguity remain in the packet.
- `[crates/trady-application/src/research_jobs.rs:VerifiedResearchPacket::validate]`
  and `check_dependencies` enforce rights, registry, content, issuer/instrument,
  model revision, expiry, contradiction and independence gates before use.
  `[crates/trady-storage/src/research_jobs.rs:PgResearchJobs]` repeats checks when
  persisting immutable outcomes. A valid sentiment classification
  may be stored with a readiness blocker; classification is not an entry permit.
  `[crates/trady-application/src/research_jobs.rs:readiness_reference]` binds the
  actual maximum negative probability and
  scoring time. It expires within 15 minutes and before any original observation
  becomes 24 hours old. The executor independently applies the adverse overlay;
  an imported forecast cannot bypass that bound.

## Side Effects

`[mod.rs:ResearchTools::execute]` calls the supplied information backend, which
reads permitted PostgreSQL content and may enqueue bounded work.
`[mod.rs:collect_registered]` delegates sequential provider reads and persistence;
earlier effects remain when a later source fails. `[mod.rs:lookup_registered]`
reserves a request before reading and commits items/state on success. On failure,
it commits status/gap/retry time before returning an unavailable lookup error.
Paid X access remains behind its
existing durable cost ledger; this implementation/testing did not call paid APIs.
`[crates/trady-application/src/research_jobs.rs:score_sentiment_job]` calls an
explicitly supplied sentiment provider and persists actual responses through its
repository; the configured production provider is the enabled FinBERT client.
Nothing here starts containers, downloads weights, sends Telegram messages,
changes trading mode or mutates broker/execution state.

## Verification

Run `cargo test -p trady-application --test research_jobs --locked` and
`cargo test -p trady-adapters --test research_jobs --locked`. Run
`python3 scripts/test-postgres.py` for guarded storage and CLI fixtures. The
FinBERT PostgreSQL regression uses a one-shot loopback HTTP fixture; it loads no
model and establishes no production model quality or latency evidence.

## Remaining integration limits

This worker collects observations and consumes trusted verifier packets. It does
not infer issuer mappings, produce a verified company profile from arbitrary
articles, or run every sector/calculator/dossier stage by itself. The production
research service must publish its verified packet and attributed reports through
the interfaces above. Live execution requires a persisted sentiment reference;
its dependency check and plan acceptance are atomic. Missing reports, models and verified
packets stay unavailable; they never become fabricated performance or evidence.
