# Trady v1 deployment, database reuse and operations

Status: deployment design and runbook. This documentation has not inspected, modified or provisioned `paperless-db-1`, installed Trady, or enabled a brokerage account.

## Operating modes and deployment unit

Use one Rust release with three isolated services: analyst, executor and Telegram gateway. Give each a distinct OS/container identity, database role and secret set. The executor needs no arbitrary web-fetch or model capability. Use a service manager/container restart policy, bounded memory/CPU, structured logs, a persistent analyst memory volume and a read-only approved runtime-skill bundle.

| Mode | Data and execution behavior |
|---|---|
| Read-only | Real permitted research/account snapshots and reports; no broker trading credential |
| Paper | Same policy and lifecycle with fake broker/fill adapters and simulated cash/deposits |
| Shadow | Real data and recorded proposals/intents compared with live quotes; no actual order submission |
| Live | Explicit account-scoped deployment with verified trading permissions and protection capabilities |

Use a distinct account/mode namespace for every record and queue. Separate paper and live credential references and deployment configuration. A mode change is a restart/deployment operation that requires a verified flat or explicitly reconciled holding state; never silently convert simulated cash, plans or orders into live state.

The service invocation shapes are `trady analyst`, `trady executor` and
`trady gateway`. The binary and bounded foundation probes exist; use the current
implementation records and CLI help for supported orchestration. A documented
shape is not an installed or approved deployment. Pin the Rust toolchain and
dependencies, and include schema compatibility, configuration fingerprint and
build identity in every deployment audit event.

## FinBERT sidecar preparation

Offline [weekly optimization](./weekly-optimization.md) and
[prediction fusion](./prediction-fusion.md) require a separate lab identity,
rights-approved frozen data, cumulative resource/spend budgets and protected
evaluators. The owner configures any weekend invocation locally; no timer is
installed by the included skill. Keep LEAN offline, Lean 4 in its pinned proof
project, model preparation outside ordinary experiment runs, and broker/release
credentials outside the coding workspace. Missing tools, data, budgets or proofs
produce `BLOCKED`; keep the compatible champion or pause new entries while
protection continues. Apply the stated lab CPU/memory caps without starving the
executor. Activation/rollback is atomic and deployment-owned, at a safe next-session
boundary, for new plans only. Model/fusion/formula changes require refreshed
compatibility, independent evidence, shadow outcomes and owner approval.

[FinBERT deployment and activation](./finbert/README.md#deployment-files-and-later-activation)
defines the opt-in CPU service, local port, pinned revision, non-root cache and
offline overlay. Compose validation is read-only and has been run; no image build,
container startup or model download is part of this change. The shipped client
configuration has `enabled=false`. Health is ready only after weights load; monitor
inference latency, errors/busy responses, model identity and cache/resource capacity.

For later deployment, isolate this service from executor resources and keep its
endpoint local/private. Inference failure, changed model identity, expired evidence
or a cold offline cache blocks required entries/target increases and triggers a
research-health report. Retain active protection; never move inference into the
executor's quote loop. Durable assessment storage and automatic readiness binding
must be completed before claiming the full trading integration.

## Reusing the PostgreSQL server in `paperless-db-1`

Reuse the PostgreSQL server, not the Paperless application's database. Create a separate database named `trady` and dedicated roles. Keep Paperless volumes, tables, database name, owner credentials and startup configuration intact. A shared database server still shares CPU, I/O, connections, backups and downtime; it is not failure isolation.

Before provisioning, record:

1. The actual PostgreSQL server version, container/service identity, persistent storage health and backup/restore procedure.
2. The Paperless database name and current workload, so Trady grants and connection limits can be checked without changing Paperless permissions.
3. The approved administrative connection and whether the deployment is container-to-container or host-to-container.
4. A stable internal network name/alias and authentication/TLS arrangement; `paperless-db-1` is a suggested container name, not proof it resolves from another service.
5. Available disk, connection capacity and an initial per-role connection cap. Start with at most five pooled connections per Trady service, within the server's available headroom.

Inspect configuration without printing environment secrets. Do not recreate the PostgreSQL container, attach a second server to its data directory, alter its image version or expose its port publicly just to connect Trady. If the current network is unsuitable, use an explicitly configured private network path or database proxy with narrow authentication.

### Reviewed provisioning outline

The SQL below is an illustrative administrator runbook, not an executed migration or a complete permission script. Review it against the installed PostgreSQL version and existing role names. Run database creation outside a transaction, as required by [PostgreSQL](https://www.postgresql.org/docs/current/sql-createdatabase.html). Set role passwords through a private secret mechanism; no password literal is included here.

```sql
CREATE ROLE trady_owner NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE trady_migrator LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT trady_owner TO trady_migrator;

CREATE ROLE trady_analyst LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE trady_executor LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE trady_gateway LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;

CREATE DATABASE trady OWNER trady_owner TEMPLATE template0;
REVOKE ALL ON DATABASE trady FROM PUBLIC;
GRANT CONNECT ON DATABASE trady
  TO trady_migrator, trady_analyst, trady_executor, trady_gateway;
```

Connect to `trady` through the approved administrative session, then establish object ownership and schema boundaries:

```sql
REVOKE ALL ON SCHEMA public FROM PUBLIC;
SET ROLE trady_owner;
CREATE SCHEMA policy AUTHORIZATION trady_owner;
CREATE SCHEMA research AUTHORIZATION trady_owner;
CREATE SCHEMA execution AUTHORIZATION trady_owner;
CREATE SCHEMA control AUTHORIZATION trady_owner;
CREATE SCHEMA audit AUTHORIZATION trady_owner;

ALTER DEFAULT PRIVILEGES FOR ROLE trady_owner
  REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;
RESET ROLE;
```

The migrator must create objects as `trady_owner` so default grants and ownership are predictable. Subsequent reviewed migrations grant `USAGE`, exact table/view privileges and narrow queue/audit function execution according to [memory and data](./memory-and-data.md). No runtime role receives owner membership, schema `CREATE`, financial-record deletion or broad rights over another process's queue. Do not grant all privileges on every table for convenience.

PostgreSQL roles exist at the server level. Creating a separate database and revoking `PUBLIC` access on `trady` does not, by itself, prevent a Trady login from connecting to another database that permits `PUBLIC CONNECT`. Use a narrowly ordered database/user-specific authentication or proxy policy that permits Trady runtime roles only to `trady`, while leaving Paperless's existing connection path working. Runtime roles get no Paperless memberships or object grants. Verify rejected cross-database login/access as well as successful Trady connections; do not revoke global permissions from Paperless to achieve this.

Record role/schema grants and object owners in deployment validation. Check that analyst writes to execution tables, gateway policy writes and runtime DDL are denied. The owner/migrator credential is mounted only during migrations. Runtime DSNs come from individual secret references and never enter Telegram messages or LLM context.

## Migrations and backups

Use ordered, checksum-tracked migrations and a migration lock. Apply additive changes first; handle data backfills separately, then deploy compatible processes. Destructive schema changes require a dedicated reviewed maintenance operation. Pause entries before incompatible changes, preserve/verify native protection and reconcile before restarting execution.

Back up `trady` daily with an approved single-database logical backup process and separately preserve the required role/grant definitions. `pg_dump` covers one database; roles are server-level objects, as described in the [official reference](https://www.postgresql.org/docs/current/app-pgdump.html). Use compatible backup tools and encrypt backups. Keep initial daily backups for 14 days and weekly backups for eight weeks, adjusted for source-retention obligations.

Exclude short-lived licensed content from longer-lived archives, or keep it in separately expiring encrypted backup sets when the applicable terms permit that arrangement. Raw source retention applies to backup copies too; deletion tombstones on restore alone do not justify retaining disallowed content indefinitely.

A nightly snapshot alone can lose a day's new records; for live operation adopt continuous WAL/PITR backup or another tested approach with a proposed recovery-point objective of five minutes. The initial recovery-time objective is 60 minutes. Verify what the shared server actually supports rather than enabling instance-wide replication settings blindly. Backup cost and resource use must not disrupt Paperless.

Restore at least monthly into a separately named test database/server with outbound broker access disabled. Recreate reviewed roles/grants, verify checksums and financial totals, replay deletion tombstones, reconstruct memory projections, and exercise startup reconciliation against a fake broker. A successful dump is not evidence that restoration works.

Never replay restored order/outbox jobs as if they were unsent. A broker may have filled orders after the backup point. In production recovery, suppress execution, reconcile the external account and all outstanding intents, rebuild missing fills/cash events with deduplication, then explicitly clear recovery state. Notification replay may duplicate a message; order replay may duplicate a trade and is forbidden without established outcome.

## Startup, shutdown and recovery

Startup order is database connectivity/schema check, configuration validation, gateway/analyst read readiness, then executor lease acquisition and reconciliation. Readiness for entries is stricter than process liveness.

Validate the skill catalog/hashes and 11/25 taxonomy map, effective S&P 500 data access, permitted source rights and the owner's five-person listener capability records. Pending-verification handles and unsupported Truth Social collection remain disabled with visible gaps. Rebuild holding watchdog bindings from actual positions after reconciliation; reference-count shared subscriptions and resume cursors without falsely marking an outage interval complete.

Load only a deployment-approved immutable Perpetual model manifest with compatible Rust/feature/label/calibration versions, hash and validation report. Record its training cutoff, supported universe/scenarios and inference CPU/memory/time caps. No trained model is bundled here; required-entry ML readiness stays blocked until a suitable model is trained/evaluated/approved. Offline candidate training and model promotion must not starve Paperless or executor safety capacity. The live LLM cannot trigger training or promotion.

Keep research queues separate by holding/global/opportunity scope, initially 80/10/10, and reserve execution capacity independently. Startup rejects rotation policies beyond one bounded replacement or two total attempts. Two-attempt rotation, source/account configuration and model promotion are reviewed deployment-policy changes, not conversational tool permissions.

Executor recovery must:

1. Acquire/renew a fenced account lease using database time.
2. Read prior intents, entry-slot reservations, active plans and exit latches.
3. Synchronize positions, open orders, fills, spendable cash, cash events and instrument changes from the broker.
4. Resolve unknown submissions and possible predecessor requests; do not blindly retry them.
5. Verify actual held quantity and corresponding native protection before declaring a position protected.
6. Rebuild trailing peaks/floors and evaluate the active plan using fresh usable quotes/FX.
7. Resume eligible entries only if policy, sources, costs, quote quality and account state all pass.

On graceful shutdown, stop claiming entry/research jobs, finish recording current effects, reconcile pending work where possible, and leave verified native protection in place. Never cancel protective orders as a generic cleanup step. An interrupted model call can be marked incomplete and retried within its job policy; an interrupted order call remains unknown until reconciled.

Manual trades, unexpected holdings, a split/spin-off, broker instrument remapping or a mismatched cash balance enter recovery. Preserve known protective orders and request reconciliation; do not improvise a liquidation of an unknown instrument. A reconciled split may adjust quantities and price thresholds economically without changing the permitted downside/target policy.

## Incident responses

| Incident | Required response |
|---|---|
| Model timeout or invalid schema | Record failure, retain current plan, no new proposal-derived order |
| Tool round/call/deadline exhausted | Finalize an incomplete/no-action result with recorded gaps; no entry from unfinished research |
| Required skill missing, changed or incomplete | Block new entries; report bundle/artifact validation failure; retain existing protection |
| Broker information tool unavailable | Report stale/unavailable display data; never replace it with model guesses or promote the request into an order |
| Paid research cap reached | Stop paid generation/collection; report reduced coverage; keep execution protection |
| RSS/news parser failure | Mark the source degraded; use only verified remaining evidence; block entries missing required evidence |
| X listener disconnected, unsupported event type or exhausted credits | Persist coverage gap, backfill only supported history, alert affected watchdog; do not report no news or bypass rate/access restrictions |
| Company/economic-link correction or source deletion | Invalidate dependent profile/graph/features/dossiers and refresh/rebuild permitted projections |
| Perpetual unavailable, stale, wrong hash/features or unsupported regime | Block required new entry/raised target; retain plan/protection and supported risk tightening; quarantine artifact if integrity fails |
| High-priority meme/reaction flood | Bound/deduplicate triage within its issuer scope; holding and execution safety retain priority |
| Stale/missing quotes or FX | Block new software price decisions and entries; retain native protection; alert and restore the feed |
| Broker timeout after submission | Mark unknown and reconcile; freeze conflicting submissions |
| Partial entry fill | Protect the actual quantity, reconcile/cancel unfilled remainder, retain consumed entry slot |
| Unprotected holding | Highest-priority incident; execute only the tested protective/recovery path and alert; block entries |
| PostgreSQL outage or lost executor lease | Stop new submissions; rely on verified native protection and recover/reconcile before resuming |
| Memory filesystem failure | Continue database-backed research when possible; retry projections; never reconstruct balances from files |
| Telegram outage | Continue deterministic holding management; queue notifications and retain local service logs |
| Daily loss guard | Latch the session entry pause; keep exits and reconciliation active |
| Market halt/closed session | Wait for supported execution conditions; disclose pending exit and gap/overnight exposure |

An incident acknowledges the actual protection state, not merely the desired state. If no tested safe automated recovery exists, retain recovery mode and give the owner exact broker/order identifiers for inspection. Avoid competing manual and automated exits; resynchronize any owner intervention before the bot continues.

## Metrics, costs and schedules

Track executor heartbeat/lease generation, quote/account/FX age, unresolved order age, protected versus held quantity, queue lag, stale proposal rejections, source completion gaps, model latency/token/cost usage, projection lag, Telegram backlog and reconciliation differences. Alert immediately on an unprotected live holding, ambiguous order or lost execution authority; warn on source and budget degradation with bounded repeated alerts.

Also track model turns/tool rounds per run, denied/failed tools, call/result mismatches, finalization time, pinned skill hashes, incomplete skill artifacts and readiness rejection reasons. Model-originated T212 information refreshes share broker quotas and use lower priority than protective/order reconciliation; cached results retain their actual ages.

Set owner-approved daily/monthly monetary caps before enabling paid LLM or news access. Track measured usage, configured price/version, reserved in-flight cost and later actual charges per provider. A missing price or exhausted cap blocks new paid work. Changing a spending cap is a versioned owner command, not an LLM decision. Existing positions must not depend on another paid generation to exit.

Reserve cost atomically before each tool/model call, including parallel calls, so a batch cannot overrun a shared cap. Count every tool-assisted generation round and uncertain charge. The per-run turn/deadline limits and finalization reserve are in [LLM conversations and tools](./llm-and-tools.md#application-loop-and-limits). Runtime skills and a high score do not authorize exceeding those limits.

Use actual exchange calendars, early closes and holidays, storing times in UTC and deriving `America/New_York` sessions. Reports display `Europe/Berlin` by default. The [NYSE calendar](https://www.nyse.com/markets/hours-calendars) is a primary schedule reference, but deployment must supply a calendar for every approved instrument/venue. Never assume the US and German clocks change on the same weekend.

The initial cadences, quote-age envelope and holding checks are specified in [architecture](./architecture.md). Configure provider-aware polling; a five-second quote/pending-order cadence is allowed only when endpoint quotas and observed latency support it. A documented capability test can choose a different interval, but it must still satisfy the live policy freshness limit.

## Performance and records

Calculate results in Rust from reconciled data, then ask the LLM to explain them. For any report interval:

```text
broker_account_PnL = ending_equity - starting_equity - deposits + withdrawals
project_PnL = broker_account_PnL - operating_costs_paid_outside_the_account
```

Do not subtract broker costs twice when they are already reflected in cash/equity. Separate dividends, realized gains, unrealized marks, fees, FX and actual withholding. Record opening/closing marks and their freshness; an unavailable valuation yields an incomplete result rather than a fabricated return.

Deposits are not profit. To compare percentage performance across top-ups, prefer time-weighted returns from valuations immediately around external flows. If those valuations are unavailable, label a Modified Dietz estimate and record its assumptions:

```text
estimated_return = account_PnL / (opening_equity + sum(weight_i × net_flow_i))
weight_i = fraction_of_report_interval_remaining_after_flow_i
```

Treat deposits as positive external flows and withdrawals as negative. A nonpositive denominator yields no valid percentage. This reporting calculation differs from the explicit operational loss-guard denominator in [trading policy](./strategy.md).

Weekly reports include complete round-trip win rate, gain/loss sizes, no-trade days, holding times, sampled drawdown, entry/exit slippage, partial fills, failed/stale research, source completeness, policy violations, external costs and a preselected cash/buy-and-hold comparison. Align benchmark cash flows/timestamps with actual deposits before comparing performance. An unrealized gain is not a completed winning trade.

Store actual broker withholding and statements for the owner's accounting. Applicable taxes depend on account entity and personal circumstances; the LLM does not calculate a definitive liability. The original v1.1 fee/tax examples are historical inputs and must be rechecked against the actual account rather than copied into executable pricing logic.

## Live capability record

Record evidence and a checked date for each capability before selecting a live broker profile:

| Area | Required evidence |
|---|---|
| Broker/account | Correct entity, Invest/cash account, currency, demo/live separation and API permissions |
| Instruments | Exact IDs, fractional increments/minimums, tick sizes, venue and trading calendar |
| Orders | Entry limits, native protective stops, expiry, cancellation, partial fills and history lookup |
| Exit coordination | Actual linked/reduce-only semantics or an equally verified no-oversell protection sequence |
| Idempotency/recovery | Documented client-key behavior or demonstrated deterministic reconciliation; ambiguous outcomes block |
| Market data | Authorized bid/ask and FX, coverage, latency, halts and sufficient polling/stream quotas |
| Research | X access when enabled, non-X news/primary evidence, Google RSS, source rights and incomplete-coverage reporting |
| LLM | Provider/model, multi-round call/result replay, JSON schemas/refusals, usage accounting, context/deadline limits and allowed data processing |
| Runtime skills/readiness | Approved bundle hashes, completed required artifacts, independent comparison, no material conflicts and current score at least 90 |
| Database | Installed PostgreSQL version, stable private connectivity, per-process grants, backups and tested restore |
| Owner control | Numeric allowlists, confirmations, entry pause/close flow, incident delivery and independent broker access |

Trading 212 remains a candidate, not an assumed fit. Its official API currently documents account-currency restrictions and several order types; those pages do not establish that every required linked/fractional protection behavior is available on this owner's account. Remain in paper/shadow mode if a required capability is unproven. [References](./references.md) records the documentation checks and open questions.

## Verification

Current verification is documentation review only. Future deployment checks must prove isolated database access without disturbing Paperless, recover unknown orders without duplication, restore memory/ledger state and keep protective exits independent of model/Telegram availability. The phased exit criteria are in [implementation and acceptance](./implementation.md).
