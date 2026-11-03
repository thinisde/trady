# Trady complete codebase audit

**Repository:** `thinisde/trady`

**Audited ref:** `main` at `64fe00afad88628ac63ef719ea841f9917e4104f`

**Audit date:** 2026-11-03 (Europe/Berlin)

**Purpose:** classify the complete tracked repository against the requested
EUR/yfinance-rs/Trading212 day-trading bot and identify what is necessary,
refactor work, missing work, deprecated, or removable.

## Executive finding

The repository is a substantial Rust trading and research system, but its active
runtime authority is still the fixed 30-symbol IEX30 US-equity universe with
Alpaca IEX market data and Trading 212 demo execution. The requested authority is
an EUR-denominated, same-day strategy with yfinance-rs quotes, a pinned OpenCode
Go/DeepSeek analyst, mandatory daily stock selection, live news/X/RSS/browser
research, dynamic exits, Telegram lifecycle notifications, and daily/weekly
reconciliation.

The codebase has reusable components for almost every boundary, but they are not
wired into the requested end-to-end behavior. The biggest gaps are authority
migration, venue-compatible quote evidence, Trading 212 protection/capability
limits, autonomous LLM selection, same-day dynamic exit monitoring, and daily
reconciliation scheduling. The current repository should be treated as a strong
research/paper foundation, not as an implementation of the requested live bot.

The mandatory-selection clarification changes the selector contract: a successful
research run must rank and choose at least one eligible stock. The current code
can still return a blocked or no-action outcome when evidence or execution inputs
are unavailable. Split “selection contract” from “operational safety halt” so the
bot does not invent a trade merely to satisfy a non-empty result.

## Audit scope and inventory

The audit covered every tracked file in the repository, including Rust, Python,
SQL, JSON, TOML/YAML, shell, Markdown, Lean proofs, service units, schemas,
fixtures, tests, skills, and model artifacts.

Inventory at the audited ref:

| Area | Tracked paths | Classification basis |
|---|---:|---|
| Workspace crates | 260 | Rust runtime, adapters, storage, policy, application, CLI, and tests |
| Documentation | 45 | Operator, design, protocol, evidence, and historical docs |
| Skills | 56 | Research, source comparison, sector, entry, position, watchdog, and weekly skills |
| Lab/proofs | 56 | Python evaluation, Lean proofs, replay, parity, and fixtures |
| Configuration | 41 | Runtime, broker, market data, intelligence, credentials, and examples |
| Schemas | 23 | LLM/research contracts and fixtures |
| Migrations | 17 | PostgreSQL schema evolution and role setup |
| Services | 23 | FinBERT, inputs, runtime, and timer/service files |
| Models | 12 | Perpetual model and related metadata |
| Scripts | 18 | Validation, build, replay, evidence, credentials, and Postgres helpers |
| Top-level reports and files | 19 | README, license, historical audits, plans, and design notes |
| **Total** | **567** | All tracked files reviewed |

The repository also contains directory entries and generated/working metadata that
are not separate tracked files. No `AGENTS.md` was present at the root or in the
tracked tree.

## Classification legend

- **RETAIN:** necessary for the target authority or for retained replay,
  verification, or operator workflows.
- **REFACTOR / REWIRE:** valuable code whose current authority, interface, or
  integration conflicts with the target and must be changed.
- **MORE WORK:** useful foundation, but the requested capability is incomplete.
- **DEPRECATE / ARCHIVE:** keep temporarily for migration, then move out of the
  active path after replay and dependency checks.
- **REMOVE:** safe to delete after the stated replacement exists and no retained
  test/import depends on it.

## Workspace and top-level files

| Path/group | Status | Audit result |
|---|---|---|
| `Cargo.toml`, `Cargo.lock` | RETAIN / REFACTOR | Workspace and pinned dependencies are necessary. The dependency graph already includes `yfinance-rs 0.9.1`; add only reviewed crates and keep lockfile reproducibility. |
| `README.md`, `docs/README.md`, `index.md` | REFACTOR | Current operator claims describe IEX30/Alpaca. Replace with the EUR/yfinance target and explicitly label paper/shadow limitations. |
| `LICENSE` | RETAIN | Repository licensing is independent of runtime scope. |
| `comprehension_report.md`, `consolidated_review_report.md`, `critic_report.md`, `design_draft.md`, `goldfish_review_report.md`, `implementation_readiness_report.md`, `intermediate_zoom_out_report.md`, `zoom_out_report.md` | DEPRECATE / ARCHIVE | Historical review artifacts are not runtime inputs. Move to `docs/archive` or delete after links are migrated. |
| `long_donchian_atr_v1_coding_agent_plan.md` | RETAIN / MERGE | Keep formula evidence while Donchian/ATR remains approved; merge operative parts into strategy and validation docs. |
| `.agents/skills/trady-weekly-optimization/SKILL.md` | RETAIN / REUSE | Useful for weekly offline reconciliation and proposals. It correctly forbids self-approval, live DB access, broker mutation, and automatic activation. |

## Crates

### `crates/trady-domain`

**Classification: RETAIN, with targeted extension.**

Typed currencies, account IDs, instrument IDs, quotes, orders, money, sessions,
capabilities, screening, and strategy/domain objects are the correct foundation.
Add venue, ISIN, broker instrument ID, Yahoo symbol, quote quality, evidence age,
selection ranking, and same-day deadline types. Do not allow arbitrary ticker
strings to become orderable instruments.

### `crates/trady-policy`

**Classification: RETAIN, REFACTOR policy defaults.**

The deterministic limits, cost/economic calculations, confidence inputs, formula
policies, intelligence policies, and validation rules are valuable. Change the
active policy from IEX30 five-session/10% target settings to an explicit EUR
intraday policy with 1–3% target objective, 1–2% default loss envelope, no
overnight carry, and monotonic dynamic exits. Resolve the 4–5% loss wording in a
separate approved revision; sentiment may not widen stops.

Remove or disable unused model-fusion/FinBERT authority if DeepSeek is the only
approved research model, but keep generic policy types needed by replay fixtures
until migration is complete.

### `crates/trady-application`

**Classification: REFACTOR / MORE WORK.**

This crate has reusable orchestration: autonomous execution, readiness, research,
screening, perpetual inference, watchdogs, X monitoring, and weekly optimization.
It currently assumes analyst packets, reviewed inputs, and IEX30 runtime paths.
Required changes:

- introduce EUR universe authority and candidate ranking;
- invoke OpenCode Go in autonomous research, not only owner-facing chat;
- enforce the at-least-one-candidate selection result;
- build explicit state machine and durable recovery;
- add yfinance quote polling and same-day exit monitoring;
- add monotonic dynamic exit updates and deadline flattening;
- connect daily reconciliation and Telegram lifecycle events;
- collapse duplicated preflight/readiness paths into one order gate.

Existing `autonomous/readiness.rs` is useful evidence validation but contains
assumptions around reviewed dossiers, model receipts, five-session holding, and
existing source types that need an intraday EUR adaptation.

### `crates/trady-storage`

**Classification: RETAIN / MORE WORK.**

PostgreSQL repositories, account state, evidence, research jobs, order intents,
verified holdings, weekly artifacts, and fail-closed storage are necessary. Add
append-only records for quote snapshots, instrument crosswalks, candidate ranks,
LLM runs/tool calls, strategy snapshots, dynamic exits, Telegram events, daily
reports, and immutable weekly proposals. Ensure report generation is reproducible
from stored snapshots and respect article/X retention and rights.

### `crates/trady-adapters`

**Classification: mixed; retain selected adapters, rewire authority.**

| Adapter area | Status | Finding |
|---|---|---|
| `yfinance.rs` | MORE WORK / REFACTOR | Already validates EUR/Euronext mappings and requires fresh two-sided quotes, but is read-only one-shot polling, not wired into autonomous execution, has a narrow suffix allowlist, lacks venue crosswalk evidence, and needs market-state cleanup/tests. |
| `trading212/` | RETAIN / MORE WORK | Correct only-broker foundation with typed credentials, account/instrument/history/submit/cancel boundaries. Current evidence says executable quotes and native attached protection are unsupported; live must stay blocked until tested capability exists. |
| `news/`, `rss`, `x` | RETAIN / REWIRE | Strong source, budget, rights, and provenance pieces. Connect feeds to autonomous selection, add yfinance news, RSS/browser page reads, and user URLs. |
| `opencode/` | RETAIN / MORE WORK | Pinned `deepseek-v4.1-flash`, bounded tool loop, and JSON contracts exist. Autonomous selection/holding workers do not yet use it as active analyst. |
| `telegram/` | RETAIN / REWIRE | Transport/spool and gateway pieces exist, but lifecycle notifications are not connected to the autonomous state machine. |
| `api_health` | RETAIN / SIMPLIFY | Keep checks protecting current quote, broker, credentials, and source freshness. Remove broad optional-provider probes from order readiness. |
| `alpaca.rs` and IEX-specific clients | DEPRECATE / REMOVE FROM ACTIVE PATH | They implement the current IEX30/USD authority, conflicting with requested yfinance/EUR authority. Retain fixtures only while migration tests need them. |
| FinBERT/intelligence providers | DEPRECATE FROM ACTIVE AUTHORITY | Keep only if a separately approved policy consumes them. DeepSeek is the requested research model. |
| AlphaVantage/Finnhub/GDELT/other optional sources | DEPRECATE / CONFIGURE OFF | Keep contract tests or historical fixtures only if needed. Target sources are yfinance, X, RSS, browser/page, and user URLs. |

### `crates/trady-cli`

**Classification: REFACTOR.**

The CLI is valuable for diagnostics and controlled execution, but current
commands expose IEX30/Alpaca/autonomous-preflight assumptions. Keep read-only
status, yfinance quote, Trading 212 account/instrument/history, research,
Telegram, replay, daily/weekly report, and explicit paper/shadow commands. Add:

- `universe snapshot/check` for EUR crosswalks;
- `research select --session ...` that always returns ranked candidates and one
  selected candidate or a typed provider failure;
- `strategy evaluate` and `risk plan` previews;
- `autonomous run` using the new coordinator;
- `reports daily` and `reports weekly`;
- `gate explain` for the single order-readiness gate.

Archive IEX-only stream-plan/watchdog commands after the yfinance monitor replaces
them. A diagnostic command may remain, but duplicated check logic belongs in the
application gate.

## Configuration, schemas, migrations, services, labs, and skills

### `config/`

**Classification: REFACTOR / SPLIT authority.**

Retain credentials, Telegram, research, formula, policy, and OpenCode examples.
Create one explicit EUR target profile and keep credentials out of committed files.
Migrate `autonomous.iex30.demo.json`, Alpaca config, IEX universe, and IEX watchdog
profiles out of the active path. Expand `yfinance.euronext.example.json` into a
reviewed target example with crosswalk, session, liquidity, quote, and risk fields.
Keep Trading 212 demo/live examples but do not imply demo proves live readiness.

### `schemas/`

**Classification: RETAIN / EXTEND.**

`llm-contracts` and `research-contracts` are the correct protocol basis. Add
candidate ranking, mandatory selected instrument, quote quality, dynamic exit,
user URL, daily report, weekly proposal, and broker capability schemas. Preserve
unknown-field rejection, stable IDs, source references, timestamps, and hashes.

### `migrations/`

**Classification: RETAIN / MORE WORK.**

Existing roles, model sessions, research sources, autonomous execution, X
monitoring, weekly lab, jobs, holdings, financial data, intelligence, fail-closed
storage, order dispatch, daily limits, and watchdog migrations show a strong
foundation. Add EUR crosswalk, quote snapshots, selection runs, article/post/page
evidence, LLM tool calls, dynamic exit events, Telegram events, daily
reconciliation, and report versions. Retire IEX-specific tables only after
migration queries and retained tests no longer use them.

### `services/`

**Classification: REFACTOR.**

`services/inputs` is a useful bounded producer/import surface but is not itself a
live news/browser provider. `services/runtime` and the weekly timer provide an
operations pattern; add a daily session timer and same-day monitor. FinBERT
services should be disabled or archived if DeepSeek is the sole sentiment
authority. Service units must not contain credentials or bypass the coordinator.

### `lab/`, `models/`, and `scripts/`

**Classification: RETAIN selectively.**

Lean proofs, Python evaluator/replay, parity, sealed fixtures, formula builders,
ledger verification, news-packet verification, and asset checks are valuable for
reproducibility. Extend them for EUR/intraday/yfinance quote replay and same-day
exits. Keep approved Donchian evidence while it remains active. Remove scripts
that only build obsolete IEX/Alpaca or FinBERT artifacts after migration.

### `skills/`

**Classification: RETAIN / CONSOLIDATE.**

Existing market research, news analysis, search planning, source comparison,
sector/subsector, company profile, influential people, entry readiness, position
review, holding watchdog, uncertainty, memory/review, and perpetual review skills
already cover most requested research behavior. Pin and reuse them. Do not add a
second set of duplicate skills. Update contracts for EUR selection and keep weekly
optimization offline and non-self-approving.

## Preflight audit and replacement

Preflight logic appears in `trady-cli/src/autonomous.rs`,
`autonomous_shadow.rs`, research/X monitoring, CLI tests, and related docs. Some is
safety-critical; some is duplicate operator ceremony.

### Retain in one order-readiness gate

The gate immediately before an order must validate account identity, exact broker
instrument, EUR currency, session open, fresh two-sided venue-compatible quote,
spread/volatility/liquidity bounds, buying power, quantity precision, maximum
loss/notional/daily limits, protection/recovery capability, idempotency, policy and
evidence validity, and flatten deadline.

### Remove or merge

Remove repository-state checks, unused-daemon checks, unrelated optional-provider
checks, repeated broker capability probes, repeated shadow/readiness calculations,
and manual preflight commands that block research or reporting when no order is
pending. Retain `gate explain` as a read-only diagnostic, but have it call the
same application gate used by execution.

This simplifies the control plane without removing money-protection invariants.
Every removed check must map to a retained invariant in the migration commit.

## Major findings by requested feature

| Requested feature | Current status | Required action |
|---|---|---|
| Daily LLM selection | Partial | Wire autonomous worker to OpenCode; add ranking and mandatory selected-candidate contract. |
| At least one stock | Missing as explicit contract | Add fallback search and typed operational-failure result; never fabricate. |
| EUR stocks | Partial | Replace IEX30/USD authority with EUR instrument/currency/venue crosswalk. |
| yfinance-rs live bid/ask | Partial | Extend polling, freshness, history, quote quality, crosswalk, and persistence; prove execution compatibility. |
| yfinance news | Partial/missing wiring | Normalize into evidence store and selection loop. |
| X API monitored posts | Partial | Existing monitoring/budget/rights components need autonomous wiring and attribution. |
| Google RSS/browser/user URLs | Missing in autonomous path | Add allowlisted collectors/page reader and untrusted URL queue. |
| Donchian/perpetual/other algorithms | Partial | Retain deterministic implementations, add registry, intraday parameters, parity, and risk integration. |
| LLM entry price | Partial | LLM may propose bounded context; Rust computes executable entry from quote/strategy/costs. |
| LLM exit price | Partial | Add validated proposal and dynamic update queue; Rust owns hard stop/target/deadline. |
| Live monitoring under 30-minute news age | Missing | Add evidence freshness and holding monitor with bounded LLM reviews. |
| Same-day exit | Missing in active policy | Add exchange-local deadline and deterministic flatten/reconciliation. |
| Telegram trade/status/news facts | Partial | Connect gateway to coordinator events and source-linked reports. |
| Daily reconciliation | Missing scheduler | Add daily timer, metrics, evidence windows, and publisher. |
| Weekly reconciliation/self-improvement | Partial | Reuse weekly skill; add daily metrics and non-self-approving Codex proposals. |
| Stock-type-specific strategy | Partial | Add classifications, profiles, and attribution without policy bypass. |
| Trading 212 only | Partial | Keep only Trading 212 mutation path; disable/remove other active broker authorities. |

## File-level coverage appendix

Every tracked path was read and classified. The following package-level map is the
compact form of that file-by-file decision; files below a row inherit the stated
status unless a named exception is listed.

| Tracked path family | Status | Decision |
|---|---|---|
| `crates/trady-domain/src/**` and tests | RETAIN / REFACTOR | Keep typed domain and extend identity/session/quote/selection types. |
| `crates/trady-policy/src/**` and tests | RETAIN / REFACTOR | Keep deterministic risk/economics; replace active IEX30/5-session defaults. |
| `crates/trady-application/src/**` and tests | REFACTOR / MORE WORK | Rewire runtime authority, LLM selection, coordinator, dynamic exit, reports. |
| `crates/trady-storage/src/**` and tests | RETAIN / MORE WORK | Keep repositories; add target event/report tables and migrations. |
| `crates/trady-adapters/src/trading212/**` and tests | RETAIN / MORE WORK | Keep only broker; replace hard-coded IEX submission guard with crosswalk. |
| `crates/trady-adapters/src/yfinance.rs` and tests | RETAIN / MORE WORK | Make authoritative quote/history poller after venue evidence. |
| `crates/trady-adapters/src/opencode/**` and tests | RETAIN / MORE WORK | Pin provider/model; wire autonomous research and holding jobs. |
| `crates/trady-adapters/src/news/**`, `rss*`, `x*` and tests | RETAIN / REWIRE | Keep provenance/budget logic; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/telegram/**` and tests | RETAIN / REWIRE | Keep outbox/auth; connect rich lifecycle events. |
| `crates/trady-adapters/src/alpaca.rs`, IEX clients, and IEX tests | DEPRECATE | Remove from active authority after EUR replay parity. |
| `crates/trady-adapters/src/financial/**`, FinBERT, optional providers | DEPRECATE / CONFIGURE OFF | Retain only if an approved benchmark or source policy still uses them. |
| `crates/trady-cli/src/**` and tests | REFACTOR | Replace IEX-focused commands; centralize order gate and add daily/report commands. |
| `config/credentials*`, credential loader docs | RETAIN / CHECK | Keep examples and loader; remove tracked generated lock if unused and fix ignore pattern. |
| `config/autonomous.iex30*`, `alpaca*`, `iex30*`, `watchdog.iex30*` | DEPRECATE | Keep migration fixtures, then archive/remove active references. |
| `config/yfinance.euronext.example.json` | RETAIN / EXTEND | Turn into target EUR example with broker crosswalk and session/risk fields. |
| `config/opencode/**`, Telegram, research, formula/policy examples | RETAIN / REFACTOR | Pin target authority and source scope. |
| `schemas/**` and fixtures | RETAIN / EXTEND | Add selection, exit, quote, URL, report, and capability contracts. |
| `migrations/**` | RETAIN / EXTEND | Add target event/report tables; retire IEX tables after dependency proof. |
| `services/inputs/**` | RETAIN / REFACTOR | Keep bounded producer/import pattern; add verified URL/RSS/page collectors. |
| `services/runtime/**` | RETAIN / REFACTOR | Add daily/session monitor; retain weekly timer as offline workflow. |
| `services/finbert/**` | DEPRECATE / ARCHIVE | Keep only as an explicit offline benchmark if DeepSeek replaces it. |
| `lab/**` proof/replay/evaluator files | RETAIN / EXTEND | Add EUR/quote/dynamic-exit replay and retain formula evidence. |
| `models/perpetual/**` | RETAIN / REVIEW | Keep if approved for EUR/intraday horizon; otherwise archive as historical. |
| `scripts/**` | RETAIN selectively | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `skills/**` | RETAIN / CONSOLIDATE | Reuse pinned skills; update contracts and avoid duplicates. |
| `docs/v1/**`, `docs/designs/**`, `docs/evidence/**` | RETAIN / RECONCILE | Existing implementation/LLM acceptance specs are useful; mark IEX conflicts and link target docs. |
| `docs/iex30*`, IEX operator/config docs | DEPRECATE / ARCHIVE | Replace active claims with EUR target authority. |
| top-level generated review reports | DEPRECATE / ARCHIVE | Move to `docs/archive` or delete after link migration. |
| `.gitignore`, root metadata, README files | REFACTOR | Ignore generated credential rotation state and update operator scope. |

The classifications are intentionally conservative. A family marked deprecate is
not deleted until `rg` import checks, workspace tests, replay tests, and migration
queries prove that the active target does not depend on it.

## Concrete blockers found in source

These findings explain why the existing components cannot simply be enabled by changing JSON:

- `crates/trady-adapters/src/trading212/mod.rs` still routes `Trading212Executor::submit_bound_before` through the IEX symbol helper. That rejects non-IEX30 instruments even though the reader can enumerate EUR stocks. Replace the guard with the reviewed EUR broker crosswalk.
- `crates/trady-application/src/autonomous/mod.rs` repeats the IEX symbol validation at dispatch. Both application and adapter guards must use one crosswalk-backed instrument identity.
- The active policy validators and holding-plan logic allow a target up to 10% and a maximum hold of five sessions. Changing only the example JSON would leave the requested 1–3% same-day policy unenforced.
- The runtime holding manager sets `HoldingContext.research_available` to false, so the queued holding-update/LLM review path is not an active dynamic-exit loop.
- `crates/trady-adapters/src/opencode/runtime_tools.rs` currently exposes only the local skill/memory/session tools. Market, news, X, RSS, yfinance, browser, user-URL, and broker-read tools need reviewed registry entries and schemas.
- Source lookup validation currently accepts a bounded external ID rather than an arbitrary user article URL, so user-supplied URLs need a separate canonical URL queue and provenance path.
- Existing analyst/executor/gateway CLI workers require `--once`; the weekly timer exists, but there is no complete daily reconciliation and same-day holding scheduler.
- `services/runtime/trady-weekly.timer` runs only Saturday at 08:00 Europe/Berlin and the service is an offline worker. It cannot itself gather live data or spawn a credentialed external Codex session. Add a daily timer and an immutable queued handoff to a separately authorized reviewer/Codex runner.
- Telegram has authenticated status/report transport and a durable outbox, but autonomous transitions do not yet publish the requested selected-stock thesis, news/CEO facts, target updates, fill details, and exit reasons.
- Current Trading 212 capability evidence documents no public executable bid/ask endpoint and no atomic attached protection/OCO. The combination of yfinance quote + application-level protection therefore remains a paper/shadow capability until race, cancel/replace, and restart recovery are proven.

## Tracked-file decision matrix

The full per-file matrix is generated at [trady-codebase-file-matrix.md](./trady-codebase-file-matrix.md). It contains one row for every tracked path, with extension, line count, status, and next action. The matrix was generated from `git ls-files` at the audited commit; package-level rationale and dependency rules are in this document.

## Verification constraints

The current container does not provide `cargo` on `PATH`, so a full Rust build/test
run could not be executed during this audit. The repository's documented checks
remain:

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
git diff --check
```

Implementation work must run those checks in a Rust-equipped environment, plus
replay, property, contract, broker-capability, stale-data, partial-fill, restart,
and flatten-deadline tests. The audit found no TODO/FIXME markers in production
code beyond explicit capability/feature messages and test stubs; those messages
are evidence of missing behavior, not proof of completion.

## Recommended execution order

1. Land the target authority and contract docs.
2. Build the EUR Trading 212/yfinance instrument crosswalk and quote evidence.
3. Add evidence ingestion and autonomous OpenCode selection with mandatory
   candidate output.
4. Adapt strategies/risk to same-day 1–3% objectives and hard loss limits.
5. Complete Trading 212 capability/reconciliation evidence.
6. Add coordinator, dynamic exit monitor, Telegram, daily reports, and weekly
   Codex proposals.
7. Remove or archive conflicting IEX/Alpaca/FinBERT/duplicate-preflight paths only
   after replay and dependency checks.
8. Run full Rust validation, shadow trading, and an explicit reviewed activation.

