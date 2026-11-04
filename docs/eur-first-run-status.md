# EUR first-run implementation status

Recorded 4 November 2026 on branch `implement/eur-first-run`.
Implementation commit: `3219fb0`.

The authority and durable-boundary milestone is implemented and tested. The
bot is **not ready for its first real-input SHADOW run**, and no complete
paper-day replay has passed. No service was installed or started, no real
Telegram message was sent, and no broker order was submitted.

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
| Workspace tests, all targets | 632 passed, zero failed; 36 integration tests ignored by this command |
| `python3 scripts/test-postgres.py` | Passed against disposable PostgreSQL 18.3 |
| New EUR PostgreSQL boundary test | Passed: fencing, role isolation, restart acknowledgements, outbox idempotency and ambiguous delivery |
| OpenCode/autonomous/Telegram ignored CLI suites | Passed |
| Credential rotation and post-rotation analyst/executor/gateway logins | Passed |
| Preparation helper | Disabled SHADOW output and overwrite rejection verified |
| Repository asset/link validation and diff whitespace | Passed |
| Existing systemd service/timer verification | Blocked by missing `/opt/trady/bin/trady`; no installation performed |

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
2. Wire real-input capture for Yahoo quotes/bars/news, Alpha Vantage,
   allowlisted RSS and reviewed X through separately metered source roles.
   Persist complete response provenance, typed failures and replay references.
   The dated review records stale Yahoo observations without bid/ask, two
   successful Alpha symbol searches, and 15 ECB RSS items persisted and read
   back from PostgreSQL. Airbus RSS parser failure is recorded as UNAVAILABLE.
   X post access/activity, processing rights and collection coverage are still
   unverified; these sources remain disabled in the EUR registry.
3. Connect the tested target lease/journal/outbox primitives to long-running
   EUR executor and Telegram services. Persist every coordinator transition,
   selection, strategy, holding and recovery state with configuration hashes
   and actor identities. The current EUR `autonomous run` refuses to claim a
   cycle occurred; its check command reports these missing integrations.
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
the missing real-input SHADOW, paper-day, quote-compatibility or protection
acceptance gates.
