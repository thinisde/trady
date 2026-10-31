# Isolated Trady runtime deployment

## Overview

These are reviewable deployment artifacts. Nothing here installs units/users,
changes a firewall, starts a listener, installs a timer or enables trading.

## Interface

- `[trady-analyst.service:ExecStart]` runs `trady autonomous analyst-service --config
  /etc/trady/runtime/analyst.json` as UID 13001, group `trady-ipc`.
- `[trady-executor.service:ExecStart]` runs `trady autonomous run --config
  /etc/trady/runtime/executor.json` as UID 13002, with supplemental IPC group.
- `[trady-gateway.service:ExecStart]` runs the separately configured Telegram gateway as UID
  13003. Configure the expanded gateway to keep analysis out of this identity.
- `[trady-weekly.service:ExecStart]` and `[trady-weekly.timer:OnCalendar]` run the offline lab as UID 13004 on Saturday
  08:00 Europe/Berlin. A timer trigger reviews a week; it cannot approve a release.
- `[trady-evaluator.service:ExecStart]` runs the concrete sealed evaluator as UID 13005,
  without any secret, database or provider route. The lab's fixed helper client
  uses `/run/trady-evaluator/evaluator.sock` and expected worker UID 13005.
- `[sysusers.conf:trady-analyst]` describes distinct identities. `[firewall.nft:trady_runtime]` contains explicit
  per-UID IPv4/IPv6 database/provider allowlists, initially empty.
- `[prepare.py:main]` exposes `prepare.py --source REVIEWED_CONFIG --destination NEW_DIRECTORY` and produces full
  analyst/executor app configs with `enabled=false` and the private socket path.
  It refuses overwrite. Validate the resulting complete files through the CLI.

| Identity | Required secret set | Allowed routes |
| --- | --- | --- |
| analyst | `TRADY_ANALYST_DSN`, approved model/input source token if required | analyst input/universe/model providers, database, private socket |
| executor | `TRADY_EXECUTOR_DSN`, one explicitly scoped broker credential pair, `TRADY_TRADING_INPUTS_TOKEN` when required | approved broker and input-service endpoints, database, analyst socket |
| gateway | `TRADY_GATEWAY_DSN`, `TRADY_TELEGRAM_BOT_TOKEN` | Telegram endpoint, database |
| lab | `TRADY_LAB_DSN` | Unix database socket; no internet |
| evaluator | none | private evaluator socket; no database or internet |

Verification, owner approval and release commands use separate credentials outside
these service secret sets. The lab cannot read a verifier, owner, releaser,
broker, executor or gateway secret. The executor does not need analyst credentials.
Its `MarketInputs` still fetches session, quote and exit-cost data from
`config.inputs.base_url` with the approved hosts and optional input token, even
when candidate analysis uses the external analyst socket. For HTTPS on port 443,
include the approved input-service IPs alongside broker IPs in `broker_https4`
and `broker_https6`; those executor sets admit both required destinations.
Other reviewed endpoint ports need corresponding explicit rules before deployment.
Verifier attestations for verification, calendar and readiness must explicitly use
`evidence_class:"PRODUCTION"`; TEST/DEMO evidence is rejected, even with the verifier role.

## Invariants

- Root installs reviewed immutable binaries under `/opt/trady/bin`, configuration
  under `/etc/trady/runtime`, approved model files under `/var/lib/trady/models`
  and credentials under root-only `/etc/trady/secrets`. Analyst, executor,
  gateway and weekly units load their respective credential JSON through
  `LoadCredential`; paths use `%d`. The evaluator has no `LoadCredential`
  directive (`[trady-analyst.service:LoadCredential]`,
  `[trady-executor.service:LoadCredential]`, `[trady-gateway.service:LoadCredential]`,
  `[trady-weekly.service:LoadCredential]`, `[trady-evaluator.service:Service]`).
- `[prepare.py:main]` initially bounds source size to 4 MiB and checks required
  top-level keys; `[prepare.py:unique]` rejects duplicate JSON keys. These checks
  do not validate the complete application schema or establish absence of secret
  values. The helper forces `enabled=false` and the private analyst socket path.
- Both complete autonomous configs set
  `analyst_socket:"/run/trady-analyst/analyst.sock"`. The analyst's
  `RuntimeDirectory` is owned by analyst/IPC group, mode `0750`; the server socket
  is `0660`. The executor shares the IPC group. This uses the real persistent
  analyst service and does not spawn an analyst under the executor's UID.
- Configs contain reviewed policy/model/source references, without secret values.
  The preparation helper disables them; a later explicit deployment review owns
  enabling. Live mode additionally requires a matching database `ACTIVE` release,
  fresh reconciled broker state and all runtime readiness/protection gates.
- The executor resolves ACTIVE authority with its own database session before
  analysis and immediately before dispatching a new live entry. The immutable
  database candidate must exactly match `RuntimeConfig.release_candidate`, the
  approved formula/policy/owner core and every locally loaded native model hash.
  Entry validity is bounded by the actual resolved owner approval expiry.
  Gate references use this mapping:

  | Runtime gate | Resolved source |
  | --- | --- |
  | `owner_approval` | `evidence.owner` |
  | `shadow_acceptance` | `shadow_<subject_hash>` |
  | `model_validation` | `evidence.verification` |
  | Remaining five runtime gate names | `evidence.readiness` |

  Existing holdings retain their original authority and bypass new-entry release
  resolution so an expired or unavailable release cannot disable risk-reducing
  recovery/protection. This does not authorize a new entry.
- All services have read-only filesystems, no ambient capabilities, no-new-privilege
  settings, dedicated bounded writable state, process limits, CPU quotas and
  memory limits. Weekly tools also have per-process/pipeline bounds.
- Apply the firewall only after inspecting local UID conflicts and approved routes.
  Empty route sets intentionally block traffic. DNS is not generally permitted:
  deploy approved hostname-to-IP mappings or a separately reviewed resolver route.
  These nftables rules are for host systemd users; container traffic needs its own
  corresponding namespace/forwarding policy. Do not assume UID rules cover NAT.
- The lab uses a Unix PostgreSQL socket because its unit permits only `AF_UNIX`.
  Prepare engine/toolchain/data before the weekly run; grant no Docker socket or
  download route. Store writable tool builds/artifacts outside this checkout.
- Evaluator IPC uses a separate `trady-evaluation-ipc` group, with only the lab
  and evaluator as members. The runtime parent is evaluator-owned 0750 and socket
  0660. Root provisions sealed data at `/var/lib/trady/sealed` and the lock at
  `/etc/trady/evaluator`, root-owned and readable only by the evaluator's private
  group; do not use the shared IPC group for sealed file access. The lab unit
  explicitly makes these paths and `/var/lib/trady-evaluator` inaccessible.
  Worker state is 0700 and includes a durable one-candidate-per-source-dataset
  exposure latch. Even a changed lock cannot re-expose that dataset to another
  candidate. See the complete [input schema and gates](../../lab/evaluator/README.md).
  Prepare these files before explicitly deploying the unit; none is bundled as
  production evidence. The evaluator remains independent of candidate code and
  accepts only the exact request frozen by the administrator.

## Side Effects

Explicit installation of `[sysusers.conf:trady-analyst]` and the service artifacts
by an administrator would provision users/units/secrets and
apply the reviewed firewall. Running the analyst/executor/gateway would access
their authorized providers and database. Running an enabled executor can place
orders; this implementation request does not authorize starting it. A timer
installation is also a separate deployment action. `[prepare.py:main]` writes
new disabled analyst/executor files sequentially using exclusive creation and
file fsync. Failure may leave partial output; there is no rollback. Requested
directory/file modes are 0755/0644, subject to the process umask.

## Verification

Validate units with `systemd-analyze verify services/runtime/*.service
services/runtime/*.timer` on the target system; missing installed binaries or
users are deployment prerequisites. Check firewall syntax with `nft --check -f
services/runtime/firewall.nft` in an authorized isolated network namespace.
Do not apply either as part of a code/test check.

Run `cargo test -p trady-adapters --test shadow_recording --test weekly_helpers`,
`cargo test -p trady-cli --test weekly_cli`, and `python3 -B -m unittest discover
-s lab/tests -v`. The root's disposable PostgreSQL runner executes
`crates/trady-storage/tests/weekly_durable.rs` for role isolation, durable trial
budgets, restart/concurrency, fenced checkpoints, consumed holdouts and tampering.

Provision `migrations/0000_lab_roles.sql` once with the cluster administrator before
running schema migration 0006. It creates roles only; grant local authentication
and scoped secrets through the deployment process. Migration 0006 grants CONNECT
and narrowly selected functions; none receives table mutation grants.
