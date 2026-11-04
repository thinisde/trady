# Volatile demo PostgreSQL incident — 4 November 2026

During the requested restart testing, Codex restarted the existing Trady demo
PostgreSQL container before checking its data mount. The container used tmpfs
for `/var/lib/postgresql` and an automatically assigned host port. Restarting it
reinitialized the cluster and changed the host port. This was an operator error;
the mount should have been checked and a backup taken before the test.

All EUR services were stopped when the loss was identified. No database backup
was found in the Trady workspace or the checked backup directory. Prior
unarchived database history could not be recovered and must not be represented
as restored. The separate Trading 212 account was last reconciled flat and no
broker mutation was submitted by this implementation.

Recovery replaced the volatile container with the same PostgreSQL image on the
named volume `trady-eur-postgres18-data`, pinned to `127.0.0.1:32811`. The retired
container was retained for incident inspection. The other database containers
and separately hosted lab were not modified. Application role passwords were
reinstated from the existing private credential store without printing them.
The new network authentication policy requires SCRAM and explicit Trady roles.

The archived first SHADOW selection and daily report were recovered, along with
the single archived SENT Telegram acknowledgement and the two known X identity
charges totalling USD 0.02. The old event remains SENT rather than being queued
again. A recovery record explicitly marks the missing provider snapshots and
unarchived history. No completed runtime checkpoint or full replay approval was
invented from this partial archive.

Fresh database and role backups were created under the root-only directory
`/var/lib/trady-eur-shadow/recovery-20261104/`. The recovery implementation is
[`scripts/recover-eur-postgres.py`](../scripts/recover-eur-postgres.py).
The SHADOW deployment receives a new identity, `eur_shadow_recovery_v2`, so it
must capture and reconcile fresh inputs instead of trusting the incomplete
pre-incident checkpoint.

Further restart tests must first verify the exact data mount, published endpoint
and backup readability. Deployment acceptance now requires an actual database
restart with the named volume and unchanged endpoint, followed by report and
outbox comparison. This incident remains part of the release evidence even if
the subsequent deployment checks pass.
