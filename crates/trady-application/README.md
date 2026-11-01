# Trady application

## Overview

Application ports, paper reconciliation and strict versioned JSON boundaries.
The crate also coordinates bounded model sessions through provider, reviewed-tool
and persistence ports, plus read-only owner monitoring. Separate autonomous
execution coordinates readiness, durable order attempts, protection and exits;
Perpetual and universe screening supply scoped advisory/research observations.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): configuration, provider/repository ports,
  broker transport, research collection, JSON contracts and model-session use cases.
  Its [Telegram child contract](./src/telegram/README.md) documents authorization,
  observation caching and unsent command replies.
  [Autonomous](./src/autonomous/README.md) and
  [optimization](./src/optimization/README.md) describe execution/readiness and
  approved formula/weekly experiment boundaries.
  [Model lab](./src/model_lab/README.md) covers offline fitting, sealed evaluation
  and approved inference; [sentiment](./src/sentiment/README.md) covers text
  projection/correlation.

## Interface

- `[src/lib.rs:FoundationConfig]`, `ExecutionRepository`, `reconcile`,
  `reconcile_account`: paper configuration, information ports and reconciliation.
- `[src/broker.rs:BrokerExecutionPort]`, `BrokerOrderRequest`:
  typed execution transport with caller-established authority.
- `[src/contracts.rs:Contracts]`, `ValidatedDocument`, `parse_json`,
  `parse_json_bounded`: strict embedded-schema and duplicate-free JSON boundaries.
- `[src/intelligence.rs:IntelligenceConfig]`, `IntelligenceRepository`, `ingest`,
  `read`, `assess_ephemeral`: reviewed rights/provenance, immutable vintages,
  retention and erasure boundaries for research intelligence.
- `[src/stock_report.rs:generate]`: offline evidence-linked US/EU reports.
  Financial period/unit consistency and source-content truth remain caller
  obligations; report generation has no provider/database effects.
- `[src/financial.rs:FinancialConfig]`, `FinancialProvider`, `FinancialRepository`,
  `collect`, `snapshot`, `signals`, `model_features`: bounded research-only data
  collection and rights-aware numerical projections.
- `[src/research_jobs.rs:ResearchJobRepository]`, `VerifiedResearchPacket`,
  `score_sentiment_job`, `readiness_reference`: scoped jobs, supplied dependency
  validation and correlated sentiment readiness.
- `[src/x_monitoring.rs:XRule]`, `XBudgetPolicy`, `XCostRepository`,
  `measure_x_counts`: versioned queries, billing ports and resumable count preflight.
- `[src/lib.rs:model_lab]`: supplied-data fitting/evaluation, immutable holdout
  journaling and model-approval/inference contracts; see the child contract above.

- `[src/sentiment/mod.rs:SentimentProvider]`, `assess_sentiment`: rights-aware source
  projection and exact prediction correlation; [design](../../docs/v1/finbert/README.md).

- `[src/research.rs:NewsSource]`, `ResearchRepository`, `ResearchConfig`,
  `collect_source`: bounded registry-driven research collection and durable progress.
- `[src/harness.rs:AgentHarness]`, `RunInput`, `LlmProvider`, `ToolExecutor`,
  `SessionRepository`: synchronous generation/tool orchestration with durable
  sessions or required ephemeral storage.
- `[src/perpetual.rs:PerpetualRepository]`, `PerpetualAdvisor`, `assess`,
  `validate_record`, `validate_review_for_scenario`, `record_review`:
  scenario/model-bound advice and reviews; repositories enforce immutable bindings.
- `[src/screening.rs:run_screening]`, `UniverseSource`, `ScreeningDataPort`,
  `ScreeningRepository`: source-backed research screens and persisted attempts.
- `[src/autonomous/mod.rs:AutonomousTrader]`, `TradingConfig`, `ExecutionJournal`,
  `TradingInputs`, `ExecutionBroker`, `recover_order`: fenced autonomous execution.
- `[src/optimization/mod.rs:FormulaAuthority]`, `FormulaProposal`,
  `calculate_authoritative_limits`, `WeeklyStore`, `WeeklyLab`, `run_weekly`:
  approved reproducible limits and bounded weekly research decisions.
- [Cargo target](./Cargo.toml): `trady_application`.

## Invariants

`[src/lib.rs:FoundationConfig::validate]` accepts version 1 paper configuration only.
`[src/lib.rs:BrokerInformationPort]` and `BrokerPort` expose no order submission or
cancellation. Concrete PostgreSQL/provider implementations remain outside this
crate. `[src/model_lab/journal.rs:EvaluationJournal]` and `write_immutable` provide
local artifact and holdout persistence within the crate.

The separate `[src/broker.rs:BrokerExecutionPort]` exposes typed submission and
cancellation for an executor. Requests confer no authority: durable intent/fencing,
policy/readiness and protection coordination are caller responsibilities. Transport
acknowledgments do not establish fills or protected positions.

`[src/autonomous/readiness.rs:EntryCandidate::validate]` requires account/snapshot/
policy-bound current dossier/skills/screening/advice and approved formula evidence.
`[src/autonomous/mod.rs:AutonomousTrader]` persists attempts before mutation,
never automatically replays uncertain submissions and requires configured live
release gates; shadow blocks entries. Foundation blockers describe the separate
paper reconciliation path, not a global absence of autonomous execution.
Weekly decisions keep entry/live activation flags false.

`[src/harness.rs:RunInput]` binds scope, session ID, optional input and cancellation.
`[src/harness.rs:AgentHarness::respond]` acquires a 45-second session lease;
`[src/harness.rs:SessionLease]` carries its TTL. Provider timeouts are bounded to
30 seconds in configured limits; provider implementations enforce timeouts
(`[src/harness.rs:LlmProvider::complete]`). Session writes require atomic revision/worker/generation/expiry
checks from the repository. Stored session history, tool results and run audit
metadata do not authorize trading. Unfinished tools may execute again on resume.

`[src/financial.rs:FinancialConfig::validate]` requires research-only operation and
disabled automatic paid fallback. `[src/financial.rs:FinancialRepository]` and
`[src/x_monitoring.rs:XCostRepository]` require implementations to enforce durable
reservations. `[src/research_jobs.rs:check_dependencies]` checks supplied metadata;
repositories/callers resolve current source rights and provenance.

`[src/intelligence.rs:ingest]` rejects tax before repository access;
`[src/intelligence.rs:assess_ephemeral]` keeps tax owner-only and ephemeral.
`[src/harness.rs:SessionRepository::is_ephemeral]` and
`[src/harness.rs:ToolExecutor::requires_ephemeral_session]` enforce the
intelligence conversation storage boundary, while `ToolExecutor::validate_context`
rechecks retained research before every model request.

## Side Effects

`[src/lib.rs:reconcile_account]` invokes supplied broker, repository and clock ports.
`[src/harness.rs:AgentHarness]` invokes supplied provider, tool and session-repository
ports, streams generated text and records context/run/tool state through the
selected repository; persistence depends on that repository.
`[src/intelligence.rs:ingest]` registers sources and stores observations/corrections;
`[src/intelligence.rs:read]` purges expired content and stores assessments only
when the referenced sources permit derived storage. `assess_ephemeral` and
`[src/stock_report.rs:generate]` compute without repository/provider access.
`[src/financial.rs:collect]`, `[src/research.rs:collect_source]`,
`[src/research_jobs.rs:score_sentiment_job]` and
`[src/x_monitoring.rs:measure_x_counts]` invoke supplied collection/inference and
repository ports. `[src/model_lab/journal.rs:EvaluationJournal]` and
`write_immutable` create durable local files; fitting and inference compute in memory.
`[src/perpetual.rs:assess]` and `[src/screening.rs:run_screening]` persist scoped
advice/screens through repositories; `[src/autonomous/mod.rs:AutonomousTrader]`
can submit/cancel through broker ports and persist execution state. Concrete
PostgreSQL connections and provider/broker HTTP calls belong to adapters.

## Verification

Run `cargo test -p trady-application --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification and
`python3 scripts/test-postgres.py` for isolated PostgreSQL checks.
