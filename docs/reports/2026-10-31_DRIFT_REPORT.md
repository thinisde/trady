This report records the earlier README review. The subsequent
[implementation record](./2026-10-31_IMPLEMENTATION.md) supersedes its two open
implementation findings; deployment evidence and full financial/proof acceptance
remain separate requirements.

## README.md

No drift detected.

## config/README.md

No drift detected.

## crates/README.md

No drift detected.

## crates/trady-adapters/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-adapters/src/autonomous/README.md

No drift detected.

## crates/trady-adapters/src/finbert/README.md

No drift detected.

## crates/trady-adapters/src/news/README.md

No drift detected.

## crates/trady-adapters/src/opencode/README.md

No drift detected.

## crates/trady-adapters/src/perpetual/README.md

No drift detected.

## crates/trady-adapters/src/screening/README.md

No drift detected.

## crates/trady-adapters/src/telegram/README.md

No drift detected.

## crates/trady-adapters/src/trading212/README.md

No drift detected.

## crates/trady-application/README.md

No drift detected.

## crates/trady-application/src/README.md

No drift detected.

## crates/trady-application/src/autonomous/README.md

No drift detected.

## crates/trady-application/src/optimization/README.md

No drift detected.

## crates/trady-application/src/sentiment/README.md

No drift detected.

## crates/trady-application/src/telegram/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## crates/trady-cli/src/README.md

- Resolved in implementation: [separate service identities and restrictions](../../services/runtime/README.md) and [serve_external](../../crates/trady-cli/src/autonomous_worker.rs) supply the private analyst socket path. Live requires that path. Installation and verification on the deployment host remain operational evidence.

## crates/trady-cli/src/finbert/README.md

No drift detected.

## crates/trady-cli/src/opencode/README.md

No drift detected.

## crates/trady-cli/src/telegram/README.md

No drift detected.

## crates/trady-domain/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-domain/src/sentiment/README.md

No drift detected.

## crates/trady-policy/README.md

No drift detected.

## crates/trady-policy/src/README.md

No drift detected.

## crates/trady-policy/src/limits/README.md

No drift detected.

## crates/trady-policy/src/prediction/README.md

No drift detected.

## crates/trady-policy/src/sentiment/README.md

No drift detected.

## crates/trady-storage/README.md

No drift detected.

## crates/trady-storage/src/README.md

- Resolved in implementation: [VerifiedHolding](../../crates/trady-storage/src/holding_research.rs) joins actual executor-observed quantity/costs and active thresholds. Position-bound native reviews are persisted and queued; [activate_reviewed_holding_update](../../crates/trady-application/src/autonomous/updates.rs) rechecks actual economics, fresh research and the old exit plan before activation.

## docs/README.md

No drift detected.

## docs/reports/README.md

No drift detected.

## docs/v1/README.md

No drift detected.

## docs/v1/finbert/README.md

No drift detected.

## migrations/README.md

No drift detected.

## schemas/README.md

No drift detected.

## scripts/README.md

No drift detected.

## skills/README.md

No drift detected.


# Follow-up audit of modified contracts

This follow-up records the user-requested audit of the 61 modified README
contracts. Eight documentation gaps across six contracts were corrected against
current implementation; the bullets retain the discrepancies found before those
corrections. All 61 modified contracts have been audited, including the final
evaluator and test contracts after their independent implementation review.

## lab/evaluator/README.md

No drift detected.

## lab/tests/README.md

No drift detected.

## config/credentials/alpha-vantage/README.md

No drift detected.

## config/credentials/financial-data/README.md

No drift detected.

## config/credentials/local-secrets/README.md

No drift detected.

## config/credentials/opencode/README.md

No drift detected.

## config/credentials/postgresql/README.md

No drift detected.

## config/credentials/README.md

No drift detected.

## config/credentials/telegram/README.md

No drift detected.

## config/credentials/trading-inputs/README.md

No drift detected.

## config/credentials/trading212/README.md

No drift detected.

## config/credentials/x/README.md

No drift detected.

## config/README.md

No drift detected.

## crates/README.md

No drift detected.

## crates/trady-adapters/README.md

No drift detected.

## crates/trady-adapters/src/autonomous/README.md

- **Resolved:** The `ResearchDraft` wire-contract description at README lines 17–19 omits `sentiment_reference`. The actual public field is `Option<SentimentReadinessRef>` with a deserialization default ([mod.rs:86](../../crates/trady-adapters/src/autonomous/mod.rs)), and candidate construction forwards it unchanged ([mod.rs:459](../../crates/trady-adapters/src/autonomous/mod.rs)). `EntryCandidate::validate_dossier` rejects its absence in live mode and validates its scope, hashes, model revision and freshness ([readiness.rs:329](../../crates/trady-application/src/autonomous/readiness.rs), lines 329–377). A producer following the documented draft interface can emit a parseable packet that can never satisfy live readiness.

## crates/trady-adapters/src/financial/README.md

No drift detected.

## crates/trady-adapters/src/finbert/README.md

No drift detected.

## crates/trady-adapters/src/inputs_service/README.md

No drift detected.

## crates/trady-adapters/src/news/README.md

No drift detected.

## crates/trady-adapters/src/opencode/README.md

No drift detected.

## crates/trady-adapters/src/perpetual/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-adapters/src/research_jobs/README.md

- **Resolved:** The implemented-tool catalog at README lines 14–18 omits five unconditional public tools: `financial.snapshot`, `fundamentals.snapshot`, `filings.search`, `macro.snapshot` and `prices.features`. `research_tool_specs` registers all five with required `source_id` and `limit` (1–3) arguments ([mod.rs:85](../../crates/trady-adapters/src/research_jobs/mod.rs), lines 85–94); `ResearchTools::execute` also propagates their backend financial status into `ToolOutput.status` ([mod.rs:190](../../crates/trady-adapters/src/research_jobs/mod.rs), lines 190–204). The actual `PgResearchJobs::information` backend reads stored financial sources with source/instrument/type and external-model rights checks ([research_jobs.rs:1254](../../crates/trady-storage/src/research_jobs.rs), lines 1254–1337). Consumers cannot discover these implemented financial-context tools or their status behavior from this catalog.

## crates/trady-adapters/src/screening/README.md

No drift detected.

## crates/trady-adapters/src/telegram/README.md

No drift detected.

## crates/trady-adapters/src/trading212/README.md

No drift detected.

## crates/trady-adapters/src/weekly_lab/README.md

No drift detected.

## crates/trady-application/README.md

No drift detected.

## crates/trady-application/src/autonomous/README.md

No drift detected.

## crates/trady-application/src/model_lab/README.md

No drift detected.

## crates/trady-application/src/optimization/README.md

No drift detected.

## crates/trady-application/src/README.md

No drift detected.

## crates/trady-application/src/sentiment/README.md

No drift detected.

## crates/trady-application/src/telegram/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## crates/trady-cli/src/finbert/README.md

No drift detected.

## crates/trady-cli/src/opencode/README.md

No drift detected.

## crates/trady-cli/src/README.md

- **Resolved:** The `autonomous run` requirements/effects at README lines 160–166 and 220–231 are stated without a mode qualification: executor DSN, executor lease and executor journal writes. `autonomous::run` dispatches shadow mode before loading `TRADY_EXECUTOR_DSN` or constructing `PgTradingJournal` ([autonomous.rs:241](../../crates/trady-cli/src/autonomous.rs), lines 241–252). `shadow::run` constructs a read-only broker reader, uses the analyst DSN only for the child compatibility path, and records immutable local cycle/proposal files without an executor lease ([autonomous_shadow.rs:61](../../crates/trady-cli/src/autonomous_shadow.rs), lines 61–103 and 164–234). The brief shadow mention at README lines 167–168 does not qualify these requirements; operators are told to supply an unnecessary executor credential and to expect execution-journal/fencing effects that shadow mode does not provide.
- **Resolved:** README lines 233–235 say `--once` propagates cycle errors. In shadow mode, `shadow::run` converts a cycle error into a persisted `{"status":"blocked"}` result ([autonomous_shadow.rs:227](../../crates/trady-cli/src/autonomous_shadow.rs)), breaks after the cycle, and returns `Ok` with `status="shadow_stopped"` ([autonomous_shadow.rs:238](../../crates/trady-cli/src/autonomous_shadow.rs), lines 238–248). The CLI dispatch and `main` therefore return success when recording/output succeeds ([main.rs:77](../../crates/trady-cli/src/main.rs), [main.rs:42](../../crates/trady-cli/src/main.rs)). Automation following the documented exit-status contract misses shadow-cycle failures unless it inspects the cycle JSON.
- **Resolved:** The interface lists `holding` and `review-holding` at README lines 61–62 but omits their request/read/write contract. `HoldingReviewRequest` requires `context`, `proposed`, `research`, `screening` and `review_reason`, rejecting unknown fields ([autonomous.rs:62](../../crates/trady-cli/src/autonomous.rs), lines 62–70). `holding_advice` requires `TRADY_ANALYST_DSN` and reads an executor-attested holding; review then persists screening, dossier and context, performs/persists native assessment and review, and queues a `HoldingUpdate` for subsequent executor checks ([autonomous.rs:344](../../crates/trady-cli/src/autonomous.rs), lines 344–460). The command list alone leaves callers unable to construct the required request or account for durable analyst writes and the queued plan-update effect.

## crates/trady-cli/src/telegram/README.md

No drift detected.

## crates/trady-domain/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-domain/src/sentiment/README.md

No drift detected.

## crates/trady-policy/README.md

No drift detected.

## crates/trady-policy/src/limits/README.md

No drift detected.

## crates/trady-policy/src/README.md

No drift detected.

## crates/trady-policy/src/sentiment/README.md

No drift detected.

## crates/trady-storage/README.md

No drift detected.

## crates/trady-storage/src/README.md

No drift detected.

## docs/README.md

No drift detected.

## docs/reports/README.md

No drift detected.

## docs/v1/finbert/README.md

- **Resolved:** The generic holding-update description omitted native activation's stronger broker-stop constraint. The guide now distinguishes `checked_update` (a nondecreasing stop) from `activate_reviewed_holding_update` (an unchanged broker stop).

## lab/lean/README.md

No drift detected.

## lab/proofs/README.md

No drift detected.

## lab/proofs/TradyProof/README.md

No drift detected.

## lab/README.md

No drift detected.

## migrations/README.md

No drift detected.

## README.md

No drift detected.

## scripts/README.md

No drift detected.

## services/inputs/README.md

- **Resolved:** README lines 157–162 say `model-lab bundle` reports the integration IDs, including `ApprovedModel::calibrator_id()`. The bundle branch reports `model_id`, `calibration_report_id`, `payoff_estimation_report_id` and `cost_model_hash`, but no calibrator ID ([model_lab.rs:98](../../crates/trady-cli/src/model_lab.rs), lines 98–102). `ApprovedModel::calibrator_id()` is a distinct `temperature_<candidate hash>` value ([mod.rs:471](../../crates/trady-application/src/model_lab/mod.rs)). Operators cannot obtain the stated exact fusion calibrator binding from the command's reported IDs.

## services/runtime/README.md

- **Resolved:** The executor dependency row omits its input-service route and optional credential: `services/runtime/README.md:28` lists only the broker credential pair and broker/database/analyst-socket routes. The runtime loads `TRADY_TRADING_INPUTS_TOKEN` at `crates/trady-cli/src/autonomous.rs:276–281`; even with the external analyst socket, `IsolatedInputs::new_external` calls `IsolatedInputs::new` (`crates/trady-cli/src/autonomous_worker.rs:425–437`), which constructs `MarketInputs` from `config.inputs` at line 411. Its `TradingInputs` implementation delegates session, quote, and exit-cost requests to that market client at lines 440–452. `InputService::request` sends HTTP requests to `config.base_url` with `allowed_hosts` and the optional bearer token (`crates/trady-adapters/src/autonomous/market.rs:86–119`); `session`, `quote`, and `costs` use the corresponding routes at lines 141–167. The executor firewall permits only database and `broker_https4`/`broker_https6` destinations (`services/runtime/firewall.nft:33–37`). Following the documented broker-only routing therefore blocks required inputs when their service uses a separate destination, preventing normal trading evaluation. Document the executor input-service route, `TRADY_TRADING_INPUTS_TOKEN` when required, and its approved firewall destinations.


