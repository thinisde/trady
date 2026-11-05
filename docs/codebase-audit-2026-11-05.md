# Codebase audit and fixes, 5 November 2026

This audit reviewed the current working tree, including the in-progress legacy
runtime retirement. Three independent `gpt-6.1-terra` reviewers used high reasoning
effort to examine execution/recovery, PostgreSQL persistence, and strategy/offline
research. The parent verified the findings, implemented fixes and reviewed the
operator documentation and deployment/release boundaries. Existing uncommitted
retirement work was preserved. This is an engineering audit, not broker or
release acceptance, and does not claim every possible defect has been excluded.

## Confirmed issues and changes

| Issue | Trigger and impact | Fix and regression evidence |
| --- | --- | --- |
| Interrupted cancellation had no recovery path | Death after the cancellation marker but before DELETE, or a lost response, left an unresolved entry or STOP permanently pending. | [Driver](../crates/trady-application/src/eur_protocol/driver.rs) reports an explicit operator action. A reviewed retry binds the original leg/ID and permits one further cancellation only after fresh reconciliation. Tests cover both interrupted dispatch and transport ambiguity, overdue acknowledgement, incomplete history and a late terminal result. |
| Protocol journals exhausted their lifetime | Routine observations reached the 10,000-event limit in both replay and PostgreSQL, preventing later recovery/exit writes. | [Migration 21](../migrations/0021_eur_protocol_lifetime.sql) removes the arbitrary ceiling while retaining positive bigint revisions, fencing and hash checks. Unchanged protected observations retain raw evidence without duplicate reducer events. Tests recover and flatten a chain beyond 10,000 records and append/reload a similarly long PostgreSQL journal. |
| Old ticker history blocked protection/liquidation | More than eight history pages made all later scans incomplete even when current-intent orders were present. | [Broker observer](../crates/trady-adapters/src/trading212/eur_protocol.rs) proves current exact-order coverage through distinct native fill totals equal to cumulative filled quantity. It stops without scanning unrelated old history. Missing coverage requires review; conflicting executions reject the observation. Tests include continuation pages, split/duplicate/missing fills, and preservation of a working STOP. This does not certify account-wide economic history. |
| Risk plans could claim false economics | Prices, quantities, cost, loss and reward claims could disagree while the readiness gate accepted their independent bounds. | [Intraday plans](../crates/trady-policy/src/intraday.rs) retain calculator cost inputs and recompute acquisition, stop loss, target return and reward-to-risk; currency and policy revision must match. Entry and paper replay bind those terms to retained/release costs. The gate also enforces the calculator's 98% cash ceiling. Tests mutate each economic field, remove cost inputs and check the allocation boundary. |
| Replay inverted opening-target chronology | A bar opened above target and later crossed the stop; stop-first intrabar ordering incorrectly reported a stop. | [Rust replay](../crates/trady-policy/src/limits/donchian.rs) and [Python evaluator](../scripts/evaluate-long-donchian-atr.py) resolve opening gaps through either threshold before ambiguous intrabar touches. Unit tests and the Rust bridge cover target-at-open, gap stop and stop-first ambiguous bars. |
| Stop updates mutated on failure | Revision overflow returned an error after changing the stop. | The next revision and updated economics are calculated before assigning any fields. A regression compares the entire plan after overflow; successful tightening remains economically valid. |
| A stale worker could fail a reclaimed job | An unlocked lease check raced with expiry/reclaim, followed by an update using only job ID. | [Research jobs](../crates/trady-storage/src/research_jobs.rs) lock the scoped worker/generation row through completion. Terminal updates also compare generation/status and require one affected row. A two-connection regression publishes a replacement generation while the old failure waits on the row lock. Policy revocation still permits the current worker to record failure. |
| Exhausted jobs stayed RUNNING without notification | Cleanup was rolled back when no queued successor existed; revoked-policy cleanup bypassed notification creation. | Cleanup and its deduplicated notification now commit together, including the empty-queue path. PostgreSQL tests reconnect to verify persisted failure and exactly one notification, plus queued and running policy-revocation cases. |
| Concurrent notification drains duplicated sends | Two gateways read the same unclaimed row; death after send but before acknowledgement replayed it. | [Migration 22](../migrations/0022_research_gateway_delivery_claims.sql) and the [gateway](../crates/trady-cli/src/telegram/gateway.rs) commit an exclusive claim before transport and require its token for completion. Concurrent/restarted gateways cannot reclaim `SENDING`/`UNKNOWN` rows. Tests cover simultaneous claims, stale tokens, repeated completion, restart, suppression and revoked direct acknowledgement privileges. |

## Documentation reconciled

The [implementation entry point](../index.md) now directs work to the EUR runtime
and labels the remaining v1 design as historical. Storage documentation describes
all 22 migrations; the script guide no longer advertises retired test targets.
The [execution guide](./eur-execution-and-release.md) documents cancellation
review, scoped history completeness, reproducible plan economics and old-plan
compatibility. Gateway docs describe claim-before-send and operator review of
ambiguous deliveries rather than promising replay after an uncertain send.

The [historical daily strategy guide](./formulas/long-donchian-atr-v1.md) labels
the old evaluation results as predating the exit correction. Frozen authority,
owner approvals and evidence were not rewritten to approve changed behavior.
Positive integrity tests use a clearly marked in-memory synthetic binding;
another regression verifies that the historical registration rejects the changed
implementation.

## Validation

All workspace, offline and disposable-database checks below passed. The final
workspace run passed **667 tests**; its 43 ignored cases comprise 42 database/CLI
cases that passed in the isolated PostgreSQL run and one supervised native DEMO
capability probe that was not run. Python suites passed **59 script, 178
evaluator, 7 training and 6 FinBERT tests**, and all **12 Rust bridge boundary
cases** passed. Formatting, check, Clippy with warnings denied, canonical assets,
local Markdown links and diff whitespace checks passed.

Commands run from the repository root:

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`
- `python3 scripts/test-postgres.py`, using a new disposable PostgreSQL container
  with restricted roles, including the normally ignored concurrency/CLI tests.
- Python unittest discovery for `scripts/tests`, `lab/tests`, `lab/training` and
  `services/finbert/tests`.
- Rust bridge synthetic Donchian boundary cases, including opening-target precedence.
- `python3 scripts/verify-assets.py` and `git diff --check`.

The historical `verify-long-donchian-integrity.py` check is expected to return
`BLOCKED` for the changed implementation hash. Its previous evidence is preserved;
this is not a current release approval.

## Upgrade and acceptance boundaries

Apply migrations 21–22 through the normal migrator before deploying the changed
binary. No services were activated and no external broker order or Telegram
message was sent during this audit.
Migration 22 marks pre-existing undelivered research notifications `UNKNOWN`
because the old sender did not retain a dispatch marker. An operator must inspect
the native chat before any explicitly authorized redelivery. A new interrupted
send likewise remains `SENDING` or `UNKNOWN`; automatic retry is intentionally
unavailable.

Legacy risk-plan JSON remains readable, but a new entry needs a newly calculated
plan with reproducible cost inputs. Already persisted protocol contexts can still
recover and perform risk-reducing exits. Changed implementation bytes require new
exact-hash release evidence; existing bundles and historical strategy metrics do
not approve these changes. Native partial-entry evidence, a genuine positive
strategy paper lifecycle, independent acceptance and exact-hash owner approval
remain separate release requirements. LIVE remains rejected.
