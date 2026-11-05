# EUR first-run implementation status

Latest continuation: [bounded DEMO month preparation](./eur-demo-month-2026-11.md).
Month operation is prepared for 6 November–6 December, with LIVE disabled and
no automatic model promotion. The final evidence record, rather than the
historical expiry statements below, identifies current policies and release
blockers. Native partial-entry proof and a genuine positive paper lifecycle
still prevent activation.

The [5 November follow-up](./eur-blocker-resolution-2026-11-05.md) supersedes
the implementation gaps and identity-expiry statement below. Decision/risk
handoffs, an evidence-gated broker protocol command and the activation verifier
are now implemented; SHADOW has been redeployed. Broker-backed acceptance and
a successful real-input paper day remain incomplete. The renewed identity
review expires **5 November 2026 at 19:08:04 UTC (20:08 CET)**.

The remaining sections preserve the earlier implementation baseline and evidence
history; they are not a claim that the later acceptance gates have passed.

Recorded 4 November 2026 on branch `implement/eur-first-run`.
Authority implementation: `3219fb0`; durable SHADOW deployment: `88078cf`;
persistent database and systemd credential repair: `712f0fa`.
IBKR, durable protocol and recorded replay: `51939a3`; delivery and reporting
follow-up: `c9f85db`.

The first **real-input SHADOW run has completed** with PostgreSQL records and
an acknowledged Telegram summary. Dedicated analyst, executor and gateway
services are installed and running. The broker was reconciled flat and no
broker order was submitted. No complete paper-day replay or demo release has
passed.

The [initial run evidence](./evidence/eur-real-input-shadow-2026-11-04.json)
binds implementation `f521286` to its deployed binary, configuration, policy,
15 provider observations and acknowledged Telegram event. The result is an
operationally completed SHADOW cycle with a blocked selection.

The [IBKR SHADOW run](./evidence/eur-ibkr-shadow-run-2026-11-04.json) subsequently
stored live SMART callbacks for both EUR identities and 16 provider observations.
ASML's captured book met the freshness bound; Airbus's older book remained
STALE. Both approved primary sessions were closed, and entry stayed blocked.
The broker reconciled flat, both distinct deployment-attempt notifications were
acknowledged once, and a service restart reused the decision without resending.
Offline replay reproduced the recorded decision and quote normalization, and
all 15 synthetic protocol scenarios passed. Positive paper-day acceptance is
still blocked. Daily/weekly timers and the sanitized pending-review workers
are installed and their one-shot probes passed.

The current identity review expires on **5 November 2026 at 12:51:52 UTC**.
Selection and source capture require a current review; Telegram can continue
delivering journaled events after entry evidence expires. No review was silently
extended by adding the IBKR source.

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
  This tested the public unauthenticated mode. It does not establish what an
  authenticated account with real-time Euronext entitlement can receive.
- A subsequent password-authenticated TradingView test succeeded, but the two
  listings still reported `delayed_streaming_900` with no usable bid/ask.
  The [authenticated assessment](./evidence/tradingview-authenticated-eur-assessment-2026-11-04.json)
  preserves that narrower result. The market was closed, so last-trade age was
  not treated as an independent latency measurement.
- The existing read-only IB Gateway returned **live SMART data** for both exact
  EUR ISINs. Its `marketDataType=1` callbacks and timestamped `BidAsk` events are
  recorded in the [IBKR assessment](./evidence/ibkr-smart-eur-assessment-2026-11-04.json).
  The [SHADOW adapter](./ibkr-shadow-quotes.md) pins conIds and primary exchanges,
  rejects delayed/frozen or untimed books, and uses provider time for freshness.
  SMART composite data has not yet been accepted as Trading 212 executable
  price authority. Yahoo remains a separate research/bar source.
- A read-only [demo-history comparison](./evidence/trading212-ibkr-history-compatibility-2026-11-04.json)
  found four OTC fills and one cancelled order. Both AAPL USD fills exactly
  matched the side-appropriate historical SMART book one second earlier.
  Nokia EUR history was unavailable with IBKR error 10187 for both SMART and
  explicit Xetra requests. There were no Airbus/ASML fills. This establishes
  limited historical USD price alignment; EUR quote compatibility remains
  unverified and no entry gate changed.
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
  `paper_day_passed=false`. `eur export` writes a private, immutable capsule with
  source observations, lossless catalog chunks, configurations and hashes.
  `replay paper-day` reconstructs decisions and IBKR normalized quotes offline,
  writes the full artifact set and persists a separate REPLAY report. Missing
  executable candidates, costs or strategy evidence produce a nonzero exit and
  `PAPER_DAY_BLOCKED`, preserving the actual captured failure.
- A sequential order-protocol reducer persists UNKNOWN before dispatch, checks
  exact IDs, reconciles cancellation races and protects only final filled
  quantity. Fifteen synthetic scenarios cover restart, ambiguous submission,
  partial entry/exit, rejected protection, disconnect, stop widening and
  exposure timing. Migration 18 adds fenced protocol journals and atomic daily
  entry-slot reservation. These regressions do not prove broker capability;
  the reducer is not yet connected to an enabled DEMO broker driver.
- Migration 19 adds immutable `PENDING_REVIEW` artifacts. Daily analyst cycles
  create sanitized handoffs from completed reports; the dedicated reviewer
  service receives only an analyst DSN. The daily timer starts the calendar
  workflow, and the EUR weekly timer aggregates recorded facts. Unknown
  economic reconciliation remains unavailable rather than becoming zero P&L.
  Neither worker can approve or activate its own changes. The existing sealed
  weekly research worker is retained separately.
- Pending Telegram delivery is independent of entry enablement and provider
  review expiry. Disabling notifications still stops the gateway. The executor
  uses `/var/lib/trady-executor/eur-shadow` for IBKR captures, matching its own
  systemd state directory rather than the root-owned evidence archive.

## Verification

The [check record](./evidence/eur-first-run-checks-2026-11-04.json) binds the
implementation commit to archived log hashes.

| Check | Result |
| --- | --- |
| Formatting and strict workspace Clippy | Passed |
| Workspace tests, all targets | IBKR/protocol build: 650 passed, zero failed; 38 integration tests ignored by this command |
| `python3 scripts/test-postgres.py` | All 38 ignored tests passed against disposable PostgreSQL 18.3, including migrations 18–19 |
| New EUR PostgreSQL boundary test | Passed: fencing, role isolation, restart acknowledgements, outbox idempotency and ambiguous delivery |
| OpenCode/autonomous/Telegram ignored CLI suites | Passed |
| Credential rotation and post-rotation analyst/executor/gateway logins | Passed |
| Preparation helper | Disabled SHADOW output and overwrite rejection verified |
| Repository asset/link validation and diff whitespace | Passed |
| Installed systemd service/timer verification | Passed after installing `/opt/trady/bin/trady` |
| Real SHADOW service restart | Passed: broker reconciliation repeated; acknowledged outbox delivery not resent |
| Rebuilt PostgreSQL restore and guarded restart | Passed against the persistent named volume; archived in `/var/lib/trady-eur-backups/` |
| Yahoo SDK and independent wire streaming | Connected; EUR observations stale and without a two-sided book |
| Authenticated TradingView | Connected; both observed EUR feeds explicitly delayed 900 seconds |
| IBKR SMART, read-only | Live data type confirmed; exact EUR contracts and timestamped bid/ask received |

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
5. Capture and pass a complete real-input paper day. The recorded-decision
   replay and synthetic protocol suite are implemented; neither substitutes
   for a successful session with actual selection, risk, cost and recovery
   evidence.
6. Consume the installed workers' sanitized `PENDING_REVIEW` handoffs through
   the separate review process. The service probes passed, but an external
   Codex review has not been started and no owner activation approval exists.
7. Build an immutable release bundle and separate owner activation process bound
   to exact hashes, account, limits and expiry. Keep preparation disabled.
   Archive IEX30/Alpaca and redundant preflight paths only after EUR parity and
   acceptance evidence pass.

The PostgreSQL suite now passes on this host, so the original audit's missing
Cargo/Docker limitation no longer applies. Passing that suite does not establish
the missing paper-day, quote-compatibility or protection acceptance gates.
