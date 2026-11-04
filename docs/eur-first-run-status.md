# EUR first-run implementation status

Recorded 4 November 2026 on branch `implement/eur-first-run`.
Authority implementation: `3219fb0`; durable SHADOW deployment: `88078cf`;
persistent database and systemd credential repair: `712f0fa`.

The first **real-input SHADOW run has completed** with PostgreSQL records and
an acknowledged Telegram summary. Dedicated analyst, executor and gateway
services are installed and running. The broker was reconciled flat and no
broker order was submitted. No complete paper-day replay or demo release has
passed.

A restart test caused the loss of the pre-existing tmpfs PostgreSQL database.
Only archived records could be recovered; prior unarchived history is lost.
The [incident record](./incident-2026-11-04-volatile-postgres.md) describes the
operator error and incomplete recovery. The rebuilt database uses a persistent
named volume and fixed loopback port. A full isolated restore and a guarded
restart subsequently preserved its new records and outbox hashes. Those checks
validate the rebuilt database, not recovery of the lost history.

## Authority

The requested base commit `6def5698c61151a9ce3cfe5a568db641031a3fe7` is absent
from this checkout. The migration branch starts at `823acb9`.
The [authority baseline](./evidence/eur-first-run-authority-2026-11-04.json)
records the actual code commit, file/policy/source hashes, demo account
`52211463`, EUR currency, model/prompt-source hashes, and missing reviews.
It is an unreviewed baseline, not release evidence or owner approval.

The owner subsequently [accepted the reviewed files and configuration](./evidence/eur-shadow-config-approval-2026-11-04.json)
at commit `6fbb864` for continued SHADOW implementation. This separate record
pins the sealed review manifest without changing its historical self-review
status. It does not grant a demo release approval, extend evidence validity,
or establish a provider entitlement.

The subsequent [delegation](./evidence/eur-demo-delegation-2026-11-04.json)
authorizes SHADOW deployment and work to resolve demo blockers. It does not
establish missing provider capabilities or approve an unidentified release.

The [EUR example](../config/eur-demo.example.json) is disabled SHADOW with
`live_enabled=false`, one entry slot, one open position, and rotation disabled.
The target validator accepts only REPLAY, SHADOW, DEMO and LIVE vocabulary;
LIVE is rejected. Legacy stage names remain readable only by the legacy
configuration parser. Legacy autonomous LIVE dispatch is also blocked.
The preparation helper produces a disabled `eur-shadow.json` and refuses
existing output files.

## Implemented boundaries

- The catalog reconciliation builder uses exact Trading 212 catalog ticker,
  currency and ISIN identities. It emits deterministic registry/catalog hashes,
  reviewed provider mappings, and explicit exclusions. A changed catalog ticker
  or ISIN invalidates the registry. Malformed broker tickers are retained as
  rejection rows. `universe snapshot` persists through the analyst PostgreSQL
  role and verifies readback; it requires a separate crosswalk review file.
- Yahoo observations preserve quality, source and crosswalk-hash metadata.
  Missing/unknown market state cannot authorize an executable quote; suffix/MIC
  mismatches and enabled mappings without broker bindings are rejected.
  Freshness defaults to ten seconds and configured quote age cannot exceed
  sixty seconds.
- The optional yfinance-rs `stream` feature is enabled and the SDK WebSocket
  path is captured separately as last-trade research. Its `QuoteUpdate` has no
  bid/ask fields. A 4 November direct SDK test received 25 updates; a separate
  browser-Origin wire test received 32 frames. The observed Amsterdam/Paris
  messages lacked bid/ask and were about 15 minutes delayed. Nasdaq `ASML` is
  a separate USD control and cannot replace the reviewed EUR listing.
  The [Yahoo assessment](./evidence/yahoo-eur-stream-assessment-2026-11-04.json)
  preserves the capture hashes.
- A separately pinned drager/tradingview 0.3.0 probe resolved both exact EUR
  ISINs and received 38 raw packets. Both listings reported
  `delayed_streaming_900`; no usable bid/ask was supplied. Its price projection
  also fabricates a receipt-time timestamp when `lp_time` is absent, which
  cannot establish quote freshness. The
  [TradingView assessment](./evidence/tradingview-eur-assessment-2026-11-04.json)
  records these limits and the standard terms' restriction on automated
  non-display use. TradingView was not added as a runtime provider.
- `gate explain --snapshot FILE` calls the same application gate as the target
  execution cycle. It checks exact universe-member equality, quote/session
  identity, spread, price/quantity precision, fractional support, buying power,
  risk and deadlines. Without a snapshot it reports missing snapshot authority.
- DEMO remains blocked by explicit missing release, owner approval and protection
  protocol blockers. Flipping `enabled` or capability flags cannot activate the
  target mutation path. No sequential stop-limit protocol was accepted or
  represented as atomic protection.
- Migration 16 replaces the target ledger's unscoped writer with scoped,
  content-hash-verified functions. Executor writes require database-time
  leases bound to the PostgreSQL connection and fencing generation.
  Acknowledgements load the exact reserved intent from PostgreSQL after restart
  instead of relying on an in-memory intent map.
- Target lifecycle publication commits journal records and outbox deliveries in
  one transaction. Delivery identity includes deployment, event, kind and chat.
  Acknowledged deliveries cannot be claimed again. A crash during dispatch
  leaves SENDING/UNKNOWN for operator review. This avoids blind resend; it does
  not claim exactly-once delivery across Telegram's ambiguous network boundary.
- Dedicated EUR analyst, executor and gateway commands use PostgreSQL role
  credentials through systemd `LoadCredential`. The executor imports the
  read-only Trading 212 reader. Durable daily checkpoints resume without
  duplicating selection or acknowledged Telegram messages, while broker
  reconciliation is repeated before returning a completed checkpoint.
- Oversized broker catalogs are stored as lossless, content-hashed chunks and
  checked again during reconstruction. Referenced source configurations bind
  the daily configuration hash. The compiled SHADOW source hash is recorded
  in each result.
- Legacy holding review now receives research availability from a current
  validated pending research update. Existing deterministic exits still win and
  every proposed update retains the full activation checks.
- `replay manifest-check` verifies all required recorded input paths and hashes
  offline, rejects directory escapes/tampering, and explicitly returns
  `paper_day_passed=false`. `replay paper-day` fails rather than returning a
  fabricated acceptance result.

## Verification

The [check record](./evidence/eur-first-run-checks-2026-11-04.json) binds the
implementation commit to archived log hashes.

| Check | Result |
| --- | --- |
| Formatting and strict workspace Clippy | Passed |
| Workspace tests, all targets | Latest SDK build: 636 passed, zero failed; 36 integration tests ignored by this command |
| `python3 scripts/test-postgres.py` | Passed against disposable PostgreSQL 18.3 |
| New EUR PostgreSQL boundary test | Passed: fencing, role isolation, restart acknowledgements, outbox idempotency and ambiguous delivery |
| OpenCode/autonomous/Telegram ignored CLI suites | Passed |
| Credential rotation and post-rotation analyst/executor/gateway logins | Passed |
| Preparation helper | Disabled SHADOW output and overwrite rejection verified |
| Repository asset/link validation and diff whitespace | Passed |
| Installed systemd service/timer verification | Passed after installing `/opt/trady/bin/trady` |
| Real SHADOW service restart | Passed: broker reconciliation repeated; acknowledged outbox delivery not resent |
| Rebuilt PostgreSQL restore and guarded restart | Passed against the persistent named volume; archived in `/var/lib/trady-eur-backups/` |
| Yahoo SDK and independent wire streaming | Connected; EUR observations stale and without a two-sided book |

Full logs are local deployment artifacts under `var/eur-first-run-checks/`:
`postgres-test.log`, `workspace-test.log`, `clippy.log` and
`systemd-verify.log`. They are retained on this host rather than committed as
release evidence; the check record preserves their SHA-256 hashes.

## Remaining dependency-ordered work

1. Complete the orderable registry from the
   [identity/source self-review](./evidence/eur-shadow-review-2026-11-04/README.md).
   Real catalog reads now establish Airbus `AIRp_EQ` and ASML `ASMLa_EQ`, their
   EUR ISINs, listing schedules, Yahoo mappings and Alpha Vantage search symbols.
   Official Airbus/ECB X ownership and stable API IDs are verified through the
   shared PostgreSQL budget. The crosswalk leaves order precision null and
   remains research-only; it is not an independent or owner release approval.
2. Verify the expanded real-input capture on the deployment. Yahoo SDK streaming,
   quote/bar/news capture, shared-budget Alpha Vantage historical cross-checks,
   ECB RSS and bounded X metadata collection are implemented separately.
   X content storage and model processing remain unapproved; metadata access
   must not be described as full X research coverage. Preserve typed provider
   failures and source rights in every capture.
   The deployed cycle persisted 11 SDK updates as STALE, both completed-bar
   observations, ECB RSS and four X metadata items. The acknowledged summary
   was sent once and the broker reconciled flat. An Alpha Vantage per-second
   burst notice was fixed with request pacing; a separate durable diagnostic
   then captured 100 daily bars for each reviewed instrument. Yahoo news
   remains UNAVAILABLE. These are provider observations, not paper-day parity.
3. Extend the working durable SHADOW path into the execution lifecycle only
   after its broker protection and recovery invariants are proven. Blocked
   selections currently record `NO_EXECUTABLE_EUR_CANDIDATE`, with no invented
   model strategy or risk plan.
4. Complete and independently validate the full fill/protection/recovery
   implementation. Existing capability and quote evidence remains
   UNSUPPORTED/pending review. Native protection or separately accepted bounded
   sequential protection is required before any DEMO entry.
5. Capture a real-input session and implement the complete offline paper-day
   execution/restart/failure scenarios. Manifest integrity alone is insufficient.
6. Add the daily/session scheduler and sanitized Codex handoff once the commands
   they invoke actually execute those workflows. No successful-looking daily
   service or empty review bundle was added.
7. Build an immutable release bundle and separate owner activation process bound
   to exact hashes, account, limits and expiry. Keep preparation disabled.
   Archive IEX30/Alpaca and redundant preflight paths only after EUR parity and
   acceptance evidence pass.

The PostgreSQL suite now passes on this host, so the original audit's missing
Cargo/Docker limitation no longer applies. Passing that suite does not establish
the missing paper-day, quote-compatibility or protection acceptance gates.
