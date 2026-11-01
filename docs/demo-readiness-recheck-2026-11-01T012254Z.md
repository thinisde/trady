# No-submit demo readiness recheck

Verified at 2026-11-01 01:22:54 UTC.

The demo remains blocked before its first market cycle. The run used `shadow`
mode with `record_only` submission, an empty inherited environment and an absent
credential-store path. It returned **zero market cycles and zero order mutations**.
The exercised configuration branch exits before credential loading or construction
of network clients, so it made no entry-order API calls. Network traffic was not
separately captured. No broker account snapshot was requested during this recheck.

## Verification

- `cargo build --locked --bin trady`: passed.
- `cargo test --locked -p trady-cli --test autonomous_cli incomplete_shadow_deployment_reports_actions_without_credentials_or_network -- --exact`: one passed.
- `cargo test --locked -p trady-adapters --test shadow_recording`: one passed.
- `trady autonomous check --config config/autonomous.demo-no-submit.json`: `configuration_ready=false`, `order_submission_policy=record_only`.
- `trady autonomous run --config config/autonomous.demo-no-submit.json --once`: `shadow_blocked`, `cycles=0`, `order_mutations=0`.
- The run's preflight matched the separate check; the configuration remained unchanged.
- The immutable audit's SHA-256 matched its filename, and its phase was `configuration`.

Audit: `/tmp/trady-demo-no-submit-20261101/shadow-journal/cycle_420ec401655483cd921657d5386f063252c4aec809fb536bf12d98d9ac4d57c8.json`.

Config SHA-256: `d471409ae4fcc3d576bfdd954e456ecb6688f52b1b5e5899dbcf9e31e6f3eabe`.

Executable SHA-256: `863ad68db7b1d4d53130ace5c4a30f13363a8e1afd72a34de56d6214f1f7714a`.

## Material needed to continue

The demo and pending runtime configurations both lack the following material.
The example input-service registry also has no sources, reviewers, accounts or
model. Repository file discovery found no separate approved model manifest or
deployment approval bundle to connect.

| Blocker | Required deployment material |
| --- | --- |
| `approved_model_registry_empty` | Current independently approved native model manifest and its exact fitted artifact. |
| `approved_input_service_unconfigured` | Reviewed HTTPS input-service configuration, source permissions and reviewer identities, account bindings, and fresh market/research imports. |
| `universe_source_disabled` | Approved constituent provider with current membership/classification, broker instrument mappings and storage rights. |
| `owner_allowlist_empty` | Owner-selected instrument IDs for `inputs.screening.owner_allowlist`. |
| `approved_skill_hashes_missing` | Reviewed runtime skill versions and their exact approved hashes. |
| `approved_formula_unavailable` | Active compatible formula/prediction authority with genuine proof, parity, independent evaluation and prospective shadow evidence. |

The next setup step requires the owner's instrument selection and locations of
the deployment materials above. No configuration or approval was fabricated to
clear these checks. Once supplied, the materials can be validated and connected
before repeating the record-only run. Runtime session, freshness and capability
checks still apply after configuration readiness succeeds.

Earlier completed implementation and database repairs are documented in
[the remediation report](./demo-remediation-2026-11-01.md).
