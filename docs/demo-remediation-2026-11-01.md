# Demo repairs — 1 November 2026

The runtime and local database defects found during the demo are repaired and
verified. Entry controls remain enabled, and broker order API writes remain **0**.
A real entry still cannot be generated: six required deployment inputs are absent.

## Repairs

- [Read-only account observations](../crates/trady-cli/src/broker_read.rs) recover
  from known broker rate limits with at most three attempts and thirty total
  seconds of backoff. Shutdown interrupts the wait. Other failures and unknown
  outcomes return directly. Order mutation clients do not use this helper.
- The [shared configuration check](../crates/trady-cli/src/autonomous.rs) now
  supplies readiness, entry-control state, submission policy and an action for
  every blocker. It also identifies an empty owner instrument allowlist.
- The [shadow runner](../crates/trady-cli/src/autonomous_shadow.rs) applies that
  check before loading credentials or constructing network clients. Missing inputs
  return `shadow_blocked`, zero market cycles and a private immutable audit instead
  of making unnecessary requests and reporting a generic provider error.
- The [database helper](../scripts/rotate-postgres-credentials.py) now supports
  `--sync-endpoints` and explicit `--roles`. It authenticates each selected role
  against the named container, verifies SCRAM and server identity, and atomically
  updates only DSN ports under file/server locks. Passwords remain unchanged;
  exception and connection-string details are suppressed. Eight local role DSNs
  now use `127.0.0.1:32811`. The lab DSN uses a different host and was excluded.
- [Foundation probe messages](../crates/trady-application/src/lib.rs) now describe
  their verification limits instead of calling implemented autonomous code absent.

## Verification

- Workspace Rust suite: **530 passed**; its **34** opt-in tests subsequently
  **passed** in the disposable PostgreSQL suite.
- Credential/endpoint Python suite: **29 passed**, covering private atomic writes,
  authentication, scoped updates, concurrent edits, redaction and preserved secrets.
- Clippy with warnings denied, Rust formatting, asset/document checks and
  `git diff --check`: passed.
- Native analyst, executor and gateway probes verify their restricted roles with
  the saved credential store, without temporary DSN overrides.
- Back-to-back native broker observations succeeded without manual spacing.
  The account stayed at **EUR 5,000.00 cash, zero positions and zero pending orders**
  between 00:06:10 UTC and 00:06:16 UTC.
- The enabled [demo configuration](../config/autonomous.demo-no-submit.json)
  returned the specific preflight blocker report, with zero order mutations.

## Remaining deployment inputs

These requirements need real deployment material before entry generation can run:

1. A reviewed HTTPS trading-input service, source permissions/reviewer identities,
   account bindings and fresh research/market imports.
2. A current independently approved native model manifest and exact fitted artifact.
3. An approved, enabled universe provider with current membership/classification,
   instrument mappings and storage rights.
4. The owner's explicit instrument allowlist.
5. Exact approved runtime skill hashes.
6. An active formula authority with matching prediction authority and genuine
   proof, parity, independent evaluation and prospective shadow evidence.

The source registry, model/formula artifacts and approvals have not been invented.
The inputs' locations or service configuration are still needed to finish setup.
Preflight completion also does not replace runtime checks for a verified open
exchange session and fresh executable market data.

```sh
target/debug/trady autonomous check --config config/autonomous.demo-no-submit.json
target/debug/trady autonomous run --config config/autonomous.demo-no-submit.json --once
```

[Machine-readable verification](./demo-remediation-2026-11-01.json) records the
native results, remaining actions, endpoint repair and source/executable hashes.
