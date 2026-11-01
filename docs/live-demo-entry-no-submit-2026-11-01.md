# Entry-enabled demo with order submission suppressed

Ran the actual Trading 212 demo account with the runtime enabled, entries unpaused
and order submissions suppressed by the existing shadow runner. The verified cycle
ran on 1 November 2026, 00:47 CET (31 October, 23:47 UTC), then stopped. Broker order
API writes: **0**.

The cycle returned `blocked: trading input provider unavailable`. No candidate was
selected and no entry order was generated. The candidate-to-order path therefore
remains unexercised with live inputs. The native configuration check also reports
missing approved inputs, native models, universe source, skill hashes and formula
authority. Entry controls are enabled; those missing prerequisites still apply.

The before/after broker comparison passed: **EUR 5,000.00 cash, zero positions and
zero pending orders** at both 23:47:31 UTC and 23:47:48 UTC. An earlier attempt hit
HTTP 429 on a broker read; the verified attempt spaced reads eight seconds apart.

## Configuration and submission guard

[autonomous.demo-no-submit.json](../config/autonomous.demo-no-submit.json) uses
`enabled=true`, `trading.mode=shadow`, `broker.environment=demo`,
`controls.pause_entries=false` and `controls.close_position=false`. It keeps a
separate `shadow_demo_no_submit_20261101` account namespace and private journal at
`/tmp/trady-demo-no-submit-20261101/shadow-journal/`.

The [runtime dispatch](../crates/trady-cli/src/autonomous.rs) selects the existing
[shadow runner](../crates/trady-cli/src/autonomous_shadow.rs), which constructs
`Trading212Reader` and no executable broker order client. A validated entry request
is saved locally using
[`ShadowBroker::record_proposal`](../crates/trady-adapters/src/shadow.rs);
it is never forwarded to the broker order API. No functional source changes were
needed.

The isolated analyst connection used the existing diagnostic PostgreSQL container
at `127.0.0.1:32811` through a process-local DSN override. Saved DSN endpoints were
preserved. During inspection, two database passwords were exposed in tool output;
both were rotated in that container and the private credential store. Verification
confirmed that the new passwords work and the old passwords are rejected.

## Verification

- `cargo build --locked --bin trady`: passed.
- `cargo test --locked -p trady-adapters --test shadow_recording`: 1 passed.
- `trady autonomous check --config config/autonomous.demo-no-submit.json`: parsed
  successfully and reported the five missing prerequisites above.
- `trady autonomous run --config config/autonomous.demo-no-submit.json --once`:
  one verified cycle, zero order mutations, stopped at unavailable trading inputs.
- Fresh native `t212 snapshot` observations before and after matched.

[Machine-readable run evidence](./live-demo-entry-no-submit-2026-11-01.json)
includes timestamps, cycle hashes, executable hash and account observations.
