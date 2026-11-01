# Simulated entry and deployment-input search — 2026-11-01

The simulated entry completed and was recorded locally. Broker submission was
blocked: **zero broker order API calls, zero broker order mutations and no fills**.
No reviewed deployment bundle was found in the accessible locations searched.
Real-data entry remains blocked by the existing deployment checks.

## Recorded simulation

The [native result and search evidence](./simulated-entry-2026-11-01.json) contain
the exact inputs, policy, calculation, order request, execution timestamp and
proposal digest. The command was executed with an empty environment:

```sh
env -i /thinisde/trady/target/debug/trady demo-entry --directory /tmp/trady-simulated-entry-20261101
```

| Field | Result |
| --- | --- |
| Classification | `SIMULATION_ONLY`; source `FIXTURE` |
| Simulation clock | `2026-10-30T14:00:00Z`, a fixed fixture clock |
| Actual execution time | `2026-11-01T00:34:12.928318660Z` |
| Synthetic account / instrument | `shadow_simulated_entry` / `SIMULATED_STOCK` |
| Proposed entry | BUY 48.95 units at a EUR 100.10 limit |
| Hypothetical stop / target | EUR 99.10 / EUR 103.11 |
| Acquisition cost | EUR 4,899.895 with synthetic zero costs |
| Submission result | `SUPPRESSED_BY_SHADOW_BROKER` |
| Network clients constructed | 0 |
| Broker order API calls / mutations | 0 / 0 |
| Fills / account changes | None |
| Entry authorized / production approved | `false` / `false` |

[entry_demo.rs](../crates/trady-cli/src/entry_demo.rs) invokes the real native
limit calculator with synthetic account, quote, volatility and cost inputs. It
constructs only `ShadowBroker`, calls its submit port and requires the
`Unsupported` response before reporting success. This receiver records proposals
without constructing a broker transport or producing an order acknowledgement.
No credentials or database are used. Synthetic confidence and formula parameters
establish no model, source, formula-release or live-readiness approval.

The private proposal is stored at
`/tmp/trady-simulated-entry-20261101/simulation-only/intent_simulation_entry_1.json`.
Its canonical SHA-256 is
`312cb91a0e8621135c333750afe0a5d96ac90e0323964dad72511a97df6d8f19`.
The containing directory has mode `0700`; its observation and proposal files
have mode `0600`. Identical reruns reuse the same intent and digest. The full
native result is retained in the JSON report if temporary files are removed.

## Deployment inputs located

Only example and pending configurations were found. The inspected runtime files
are [autonomous.example.json](../config/autonomous.example.json),
[autonomous.pending.json](../config/autonomous.pending.json),
[autonomous.demo-no-submit.json](../config/autonomous.demo-no-submit.json) and
`/tmp/trady-live-demo-20261031/shadow.json`.

| Requested input | Located evidence | Reviewed deployment status |
| --- | --- | --- |
| Inputs-service config / URL | [services/inputs/config.example.json](../services/inputs/config.example.json) binds `127.0.0.1:8787` with empty source/reviewer/account registries and `model=null`. Runtime `inputs.base_url` is `https://approved-inputs.example.invalid/v1/`. No inputs listener was found. | Not found; example and placeholder only |
| Approved native model | `models=[]` in each runtime config; the input-service example has `model=null`. | Not found; test fixtures and the separate FinBERT sentiment service do not supply native execution-model approval |
| Formula authority | `trading.limits_authority=null`; the release database has no candidates, evidence or ACTIVE release. | Not found; bundled formula/proof sources do not supply deployment-scoped approval |
| Approved skill hashes | `trading.approved_skills={}`. [Skill sources/catalog](../skills/README.md) are available. | Not found; no reviewed deployment pins |
| Owner instrument list | `inputs.screening.owner_allowlist=[]`. A [universe example](../crates/trady-adapters/src/screening/universe.example.json) exists. | Not found; the universe example does not establish owner approval |

[deployment.pending.json](../config/deployment.pending.json) records outstanding
requirements. It is an operator handoff manifest and supplies no approval evidence.
Credential values remain in the private `config/credentials.local.json` store
with mode `0600`; this report contains no secret values or DSNs.

## Search scope and registry evidence

The search inspected 1,027 JSON files across 25 roots: the repository, including
ignored files, and 24 existing `/tmp/trady*` directories. Nested structures and
multi-document JSON were included. A further repository filename scan found only
test/example candidates for model, manifest, authority and allowlist artifacts.

There is one Git worktree, `/thinisde/trady`. No deployed Trady system services
were found. The following deployment locations are absent:

```text
/etc/trady                      /opt/trady
/var/lib/trady                  /root/.config/trady
/thinisde/trady/var              /thinisde/trady/models
/thinisde/trady/artifacts        /run/trady-analyst
/var/lib/trady-lab               /var/lib/trady-analyst
/var/lib/trady-executor          /var/lib/trady-gateway
/var/lib/trady-evaluator
```

Read-only queries against database `trady` in the existing diagnostic container
`trady-live-demo-db-20261031` returned:

| Registry | Count |
| --- | --- |
| `release.candidates` | 0 |
| ACTIVE release candidates | 0 |
| `release.evidence` | 0 |
| `execution.autonomous_accounts` | 0 |
| `research.financial_sources` | 0 |
| `research.model_sessions` | 1 |

The model-session row holds conversation/provider/skills metadata; it is not a
fitted native-model registry. These findings cover accessible local locations
and registries; they do not establish absence on an unconfigured external system.

## Verification

- `cargo test --locked -p trady-cli`: 46 passed, 0 failed; 3 PostgreSQL integration
  tests skipped. The two new CLI tests verify native calculation, recorded digest,
  submission suppression, unchanged account, private permissions, identical
  reruns and rejection of runtime/broker options or relative paths.
- `cargo clippy --locked -p trady-cli --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- Saved proposal digest, fixture labels and `0700`/`0600` permissions: passed.
- `python3 scripts/verify-assets.py`: passed; 11 canonical JSON assets, 52 skill
  records, 198 Markdown files and 1,498 local links verified.
- `git diff --check` and whitespace checks for the new files: passed.

These are the current CLI-change checks. Earlier broader remediation results
remain in the [historical remediation report](./demo-remediation-2026-11-01.md).
See the [simulation operating guide](./autonomous.md#simulated-entry) for reuse.
