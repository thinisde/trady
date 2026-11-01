# Live demo with entry-order API calls disabled

Attempted the existing shadow demo on 1 November 2026, 01:04:04–01:04:21 UTC
(02:04 CET). Authenticated Trading 212 demo-account reads succeeded. The
autonomous runtime returned `shadow_blocked` before starting a market cycle.
Entry-order API calls: **0**. Broker order mutations: **0**.

The account was unchanged between the two observations: **EUR 5,000.00 available
cash, zero positions and zero pending orders**. No candidate was evaluated and no
entry proposal was generated. No continuous demo process was started.

## Execution and submission guard

Built the current working tree with `cargo build --locked --bin trady`, then ran:

```sh
target/debug/trady autonomous check --config config/autonomous.demo-no-submit.json
target/debug/trady autonomous run --config config/autonomous.demo-no-submit.json --once
```

The existing [demo configuration](../config/autonomous.demo-no-submit.json) has
`enabled=true`, `trading.mode=shadow`, `broker.environment=demo`,
`controls.pause_entries=false` and `controls.close_position=false`.
The check confirmed `order_submission_policy=record_only`.

The [runtime dispatch](../crates/trady-cli/src/autonomous.rs) selects the
[shadow runner](../crates/trady-cli/src/autonomous_shadow.rs), which checks readiness
before loading credentials or constructing network clients. This run also used an
empty inherited environment and a deliberately absent credential-store path.

Separate native `t212 snapshot --config /dev/stdin` commands received the exact
`broker` object from the demo configuration on standard input. They used the
configured demo credentials through
[Trading212Reader](../crates/trady-adapters/src/trading212/mod.rs), whose snapshot
path issues only GET requests to account summary, positions and orders.
No broker executor was constructed on the exercised paths. Zero entry calls are
supported by these paths and the preflight result; HTTP traffic was not separately
captured.

## Readiness blockers

The runtime requires all six prerequisites before a real-data shadow cycle can run:

- `approved_model_registry_empty`: a current independently approved native model manifest and its exact fitted artifact.
- `approved_input_service_unconfigured`: an approved input service with reviewed sources and fresh account-bound market and research inputs.
- `universe_source_disabled`: an enabled approved constituent provider with mappings, rights and current classifications.
- `owner_allowlist_empty`: the owner's explicit instrument allowlist.
- `approved_skill_hashes_missing`: exact reviewed runtime skill hashes.
- `approved_formula_unavailable`: active compatible formula authority with matching approval evidence.

These requirements were preserved. The runtime is not operational for autonomous
candidate evaluation until they are supplied.

## Verification and evidence

- Build passed.
- `incomplete_shadow_deployment_reports_actions_without_credentials_or_network`: passed.
- `shadow_only_records_intents_across_restart_and_never_acknowledges_orders`: passed.
- Before snapshot: `2026-11-01T01:04:04.979512795Z`.
- After snapshot: `2026-11-01T01:04:21.465657444Z`.
- Runtime result: `shadow_blocked`, `cycles=0`, `order_mutations=0`.
- Immutable runtime audit: `/tmp/trady-demo-no-submit-20261101/shadow-journal/cycle_72d6104cadcade10df6cbfb28cb541cf4af693ddb8885eda0329847d72c2b54b.json`, mode `0600`; its filename hash matches its bytes.
- Demo configuration bytes were unchanged.

[Machine-readable evidence](./live-demo-no-entry-orders-2026-11-01T010404Z.json)
contains command results, both account snapshots, the audit record, timestamps,
configuration and executable SHA-256 values, and the checkout HEAD identifier.
The checkout contains pre-existing uncommitted work; the executable hash identifies
the binary actually used. No functional source, deployment configuration or
credential-store changes were made by this run.

