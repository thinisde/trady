## README.md

No drift detected.

## index.md — current implementation status

No drift detected.

## docs/README.md

No drift detected.

## docs/v1/README.md — current implementation status

No drift detected.

## docs/v1/implementation.md — updated verification contract

No drift detected.

## docs/foundation.md

No drift detected.

## crates/README.md

No drift detected.

## crates/trady-domain/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-policy/README.md

No drift detected.

## crates/trady-policy/src/README.md

No drift detected.

## crates/trady-application/README.md

No drift detected.

## crates/trady-application/src/README.md

No drift detected.

## crates/trady-adapters/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-storage/README.md

No drift detected.

## crates/trady-storage/src/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## crates/trady-cli/src/README.md

No drift detected.

## migrations/README.md

No drift detected.

## schemas/README.md

No drift detected.

## config/README.md

No drift detected.

## scripts/README.md

No drift detected.

## skills/conversation-and-tools/SKILL.md — referenced instruction artifact

- **Resolved:** The instruction required research schema version 1 while
  `schemas/llm-contracts.v2.json:$defs.Research` and
  `crates/trady-application/src/contracts.rs:Contracts::validate` require version 2.
  Corrected the instruction to research version 2 with protocol/proposal versions 1;
  advanced the skill to 1.0.1 and its catalog entry/bundle to 1.0.1/2.0.1.

## skills/catalog.yaml — corrected skill metadata

No drift detected.

Trading 212 follow-up scope: the new broker adapter/port, snapshot command,
examples and their 13 contract/guide rollups. Concurrent LLM/news interfaces are
reviewed by their source owners; this follow-up makes no completeness claim for them.

## docs/trading212.md — Trading 212 follow-up

- **Resolved:** The guide described successful CLI snapshot collection as fixture-tested.
  Only adapter collection is exercised through HTTP fixtures; CLI tests cover help,
  missing credentials and input/command rejection. Corrected the verification claim
  against `crates/trady-cli/tests/trading212_cli.rs` and
  `crates/trady-adapters/src/trading212/tests.rs`.
- **Resolved:** The guide claimed cancellation-race coverage. Tests cover acknowledgment,
  pending-order absence and connection loss, without racing cancellation against fills.
  Corrected the coverage statement and retained fill races as future work.

## crates/trady-adapters/src/trading212/README.md — Trading 212 follow-up

- **Resolved implementation finding:** Direct pending-order/history reads originally
  bypassed configured broker account verification. `Trading212Reader::ensure_binding`
  now covers every direct information read with identity evidence cached for at most
  60 seconds. Snapshots and mutations force a fresh summary check. The independent
  re-review found no remaining binding objection; mismatched-credential and shared-cache
  behavior is covered in `trading212/tests.rs`.

## README.md — Trading 212 follow-up

- **Resolved:** Global side effects omitted the authenticated broker snapshot command
  and the separate executor client's ability to submit/cancel real orders. Added those
  effects, with the distinction between transport and the unfinished live workflow.

## crates/README.md — Trading 212 follow-up

No drift detected.

## config/README.md — Trading 212 follow-up

No drift detected.

## docs/README.md — Trading 212 follow-up

No drift detected.

## docs/foundation.md — Trading 212 follow-up

No drift detected within the Trading 212 scope. Foundation evidence remains a dated
first-milestone record.

## crates/trady-adapters/README.md — Trading 212 follow-up

No drift detected.

## crates/trady-adapters/src/README.md — Trading 212 follow-up

No drift detected.

## crates/trady-application/README.md — Trading 212 follow-up

No drift detected.

## crates/trady-application/src/README.md — Trading 212 follow-up

- **Resolved:** Public broker interface coverage omitted `BrokerFailure::new`, optional
  HTTP/retry/broker-ID diagnostics, time-in-force variants and `Market.extended_hours`.
  Added these contracts and failure-kind meanings from `broker.rs`.

## crates/trady-cli/README.md — Trading 212 follow-up

No drift detected.

## crates/trady-cli/src/README.md — Trading 212 follow-up

No drift detected.

## crates/trady-cli/src/telegram/README.md

No drift detected.

## crates/trady-adapters/src/telegram/README.md

- **Low — incomplete public API coverage:** The
  [Interface section](./crates/trady-adapters/src/telegram/README.md#interface)
  omits the exported `BotIdentity` return type and its `id: i64` and
  `username: String` fields, the exported `MESSAGE_UNITS` constant, and the
  explicit `BotToken::new` / `BotToken::bot_id` method names. These declarations
  exist in [mod.rs](./crates/trady-adapters/src/telegram/mod.rs) at lines 15,
  23, 41 and 93–96; `TelegramClient::identity` returns `TelegramResult<BotIdentity>`
  at line 137. Add these names and the identity shape to the component's public
  interface contract so consumers can discover the complete exported boundary.

## crates/trady-adapters/src/telegram/spool/README.md

No drift detected.

## README.md — FinBERT follow-up (2026-10-31)

No drift detected.

## config/README.md — FinBERT follow-up

No drift detected.

## docs/README.md — FinBERT follow-up

No drift detected.

## docs/v1/README.md — FinBERT follow-up

No drift detected.

## docs/v1/finbert/README.md — FinBERT follow-up

No drift detected.

## docs/v1/architecture.md — FinBERT follow-up

- **G1 — planned, unimplemented: automatic decision integration.** The
  [research-to-entry flow](./docs/v1/architecture.md#research-to-entry-flow) and
  illustrative `sentiment.required_for_entry_and_target_increase` setting require
  assessment-bound readiness and executor checks. Production source references
  show that the only caller of
  [assess_sentiment](./crates/trady-application/src/sentiment/mod.rs) is
  [the explicit CLI](./crates/trady-cli/src/finbert/mod.rs), line 53. Its optional
  `sentiment_thresholds` call at line 58 uses supplied features, default policy and
  no exceptional gate; no analyst/executor caller applies these constraints.
  [FoundationConfig](./crates/trady-application/src/lib.rs) has no required-sentiment
  setting. Add approved-model/policy binding, runtime configuration and readiness/
  plan-activation enforcement before treating the proposed required gate as active.
  The design already labels this work as planned; this is an open implementation
  gap, not a regression or an assertion that it was completed.

## docs/v1/strategy.md — FinBERT follow-up

- **D1 — medium: summary overstates the adverse-sentiment entry veto.** The
  [adaptive-holding table](./docs/v1/strategy.md#adaptive-holding-and-threshold-changes),
  line 136, groups unavailable/stale inference and adverse sentiment under a rule
  to block entry/target increases. However,
  [sentiment_thresholds](./crates/trady-policy/src/sentiment/mod.rs), lines 87–100,
  deducts 10 points at negative probability 0.60 and vetoes entry independently of
  score only at 0.80. With base score 100 and negative probability 0.79, adjusted
  score is 90, `review_required=true`, `entry_score_passes=true` and
  `target_increase_allowed=false`. The existing
  [boundary regression](./crates/trady-policy/tests/sentiment/main.rs),
  `exact_negative_veto_and_weak_evidence_cannot_gain_points`, explicitly expects
  entry-score checks to pass at 0.60 and 0.79; that test passed during this audit.
  Split the table row into unavailable/stale, adverse-review/score-penalty and
  hard-veto cases so it agrees with the detailed
  [version-1 rules](./docs/v1/finbert/README.md#threshold-calculation-version-1).
  These flags remain calculation results, not order authorization.

## docs/v1/research.md — FinBERT follow-up

No drift detected.

## docs/v1/watchdogs-and-x.md — FinBERT follow-up

- **G1 — planned, unimplemented: sentiment-triggered holding reviews.** The
  [holding-watchdog design](./docs/v1/watchdogs-and-x.md#holding-watchdog) requires
  coalesced reviews on adverse/changed sentiment, recorded model/item references
  and refresh on unavailable inference. The
  [application adapter](./crates/trady-application/src/sentiment/mod.rs) returns an
  in-memory assessment, and its only production caller is the explicit scoring
  command; there is no sentiment-aware listener/watchdog scheduler, durable review
  event or automatic holding-plan consumer. Implement those parts with the
  readiness/executor integration in G1 above. This is the durable scheduling and
  binding work already identified as planned in the document.

## docs/v1/operations.md — FinBERT follow-up

No drift detected.

## docs/v1/memory-and-data.md — FinBERT follow-up

- **G2 — planned, unimplemented: durable assessments and invalidation.** The
  [sentiment lineage contract](./docs/v1/memory-and-data.md#sentiment-assessment-lineage)
  names `research.sentiment_assessments` and `research.sentiment_evidence`, together
  with issuer/instrument scope, source identity, calculation/policy binding,
  readiness references and source/model invalidation. There are no definitions or
  references for those tables in [migrations](./migrations/README.md),
  [storage](./crates/trady-storage/README.md) or the canonical schemas. The current
  [SentimentAssessment/SentimentEvidence](./crates/trady-domain/src/sentiment/mod.rs)
  hold model identity/time plus item/lineage/hash/time/probabilities; they contain
  no instrument, source-registry revision, policy or readiness binding, and the
  [CLI](./crates/trady-cli/src/finbert/mod.rs) only prints JSON. Implement the
  persistence/dependency model and permission/edit/deletion invalidation before
  using retained results in automatic decisions. This is expressly planned work,
  not missing DDL claimed to exist by the documentation.

## docs/v1/llm-and-tools.md — FinBERT follow-up

No drift detected.

## docs/v1/implementation.md — FinBERT follow-up

No drift detected.

## docs/v1/references.md — FinBERT follow-up

No drift detected.

## services/README.md — FinBERT follow-up

No drift detected.

## services/finbert/README.md — FinBERT follow-up

- **D2 — low: server timeout behavior is absent from the contract.** The
  [service contract](./services/finbert/README.md#invariants) documents body and
  concurrency bounds but omits
  [Handler.setup](./services/finbert/serve.py), lines 90–92, which sets a hardcoded
  10-second socket-I/O timeout. This differs from the configurable 1–60 second
  client request timeout in
  [FinBertClient::new](./crates/trady-adapters/src/finbert/mod.rs), lines 53–60.
  `Handler.do_POST` calls `predictor.predict` synchronously and holds its inference
  lock until that call returns; there is no inference deadline or cancellation
  mechanism. A client timeout therefore does not guarantee that inference stops,
  and subsequent calls can receive `503 inference_busy` while that work continues.
  Document the socket-I/O limit and distinguish client timeout from model-job
  cancellation; do not describe the former as a server inference deadline.

## crates/trady-adapters/README.md — FinBERT follow-up

No drift detected.

## crates/trady-adapters/src/README.md — FinBERT follow-up

No drift detected.

## crates/trady-application/README.md — FinBERT follow-up

No drift detected.

## crates/trady-application/src/README.md — FinBERT follow-up

No drift detected.

## crates/trady-domain/README.md — FinBERT follow-up

No drift detected.

## crates/trady-domain/src/README.md — FinBERT follow-up

No drift detected.

## crates/trady-policy/README.md — FinBERT follow-up

No drift detected.

## crates/trady-policy/src/README.md — FinBERT follow-up

No drift detected.

## crates/trady-cli/src/README.md — FinBERT follow-up

No drift detected.

## README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-domain/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-domain/src/README.md — OpenCode/bootstrap follow-up

- The redaction invariant names title, summary, text and expanded social context text, then says other fields remain unchanged. [SourceItem::redact](./crates/trady-domain/src/research.rs) also clears `feed_title`. Add `feed_title` to the documented content fields that redaction removes.

## crates/trady-policy/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-policy/src/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-application/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-application/src/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-application/src/telegram/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-adapters/README.md — OpenCode/bootstrap follow-up

- The public interface and side effects omit the exported `telegram` adapter. [TelegramClient::send_message](./crates/trady-adapters/src/telegram/mod.rs) sends messages over HTTP, and [StateStore::commit](./crates/trady-adapters/src/telegram/spool.rs) writes, syncs and replaces local queue state. Link the [Telegram child contract](./crates/trady-adapters/src/telegram/README.md) and describe the transport and durable local state boundary.

## crates/trady-adapters/src/README.md — OpenCode/bootstrap follow-up

- The module inventory and side effects omit `pub mod telegram` from [lib.rs](./crates/trady-adapters/src/lib.rs). Its [TelegramClient](./crates/trady-adapters/src/telegram/mod.rs) performs bot API requests, including message sends, and its [StateStore](./crates/trady-adapters/src/telegram/spool.rs) persists polling and outbox state on disk. Add the [Telegram child contract](./crates/trady-adapters/src/telegram/README.md) and these network/filesystem effects.

## crates/trady-adapters/src/opencode/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-storage/README.md — OpenCode/bootstrap follow-up

No drift detected.

## crates/trady-storage/src/README.md — OpenCode/bootstrap follow-up

- The session repository invariants omit the SQL time limits installed by [PgSessionRepository::connect](./crates/trady-storage/src/sessions.rs): both `statement_timeout` and `lock_timeout` are set to five seconds. Document these limits and the resulting operation failures alongside the lease and revision checks.

## crates/trady-cli/README.md — OpenCode/bootstrap follow-up

- The command inventory and side effects omit the `telegram` branch dispatched by [main.rs](./crates/trady-cli/src/main.rs). It exposes discovery, configuration checks and a monitoring loop that can read the broker, send Telegram replies/alerts and persist local queue state. Link the [Telegram command contract](./crates/trady-cli/src/telegram/README.md) and include those explicit command effects.

## crates/trady-cli/src/README.md — OpenCode/bootstrap follow-up

- The command contract omits `telegram discover`, `telegram check --config FILE` and `telegram run --config FILE [--once]`, implemented by [telegram::run](./crates/trady-cli/src/telegram/mod.rs). The same module resolves the private bot credential, optionally reads broker snapshots, sends replies/alerts and uses the durable local outbox. Add these commands, effects and the [Telegram child contract](./crates/trady-cli/src/telegram/README.md).

## crates/trady-cli/src/opencode/README.md — OpenCode/bootstrap follow-up

No drift detected.

## config/README.md — OpenCode/bootstrap follow-up

No drift detected.

## config/opencode/README.md — OpenCode/bootstrap follow-up

No drift detected.

## docs/README.md — OpenCode/bootstrap follow-up

- The reading map omits the implemented [Telegram monitoring guide](./docs/telegram.md), although it indexes the other operational provider guides. Add `./telegram.md` to this directory's guide list so discovery, owner configuration and monitoring operation are reachable from the documentation index.

## docs/opencode/README.md — OpenCode/bootstrap follow-up

No drift detected.

## migrations/README.md — OpenCode/bootstrap follow-up

No drift detected.

## scripts/README.md — OpenCode/bootstrap follow-up

No drift detected.
