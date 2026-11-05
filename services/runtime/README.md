# Isolated Trady runtime deployment

These are reviewable templates. They do not install units, enable a timer or
grant trading authority. The [bounded EUR month](../../docs/eur-demo-month-2026-11.md)
still requires complete evidence and exact-hash owner approval before DEMO.

## EUR services

- [trady-eur-analyst.service](./trady-eur-analyst.service): provider capture and
  model work as the analyst identity, with its own restricted credentials.
- [trady-eur-executor.service](./trady-eur-executor.service): `autonomous run`
  with the EUR SHADOW config as executor. This route cannot select a legacy
  runtime or submit orders.
- [trady-eur-gateway.service](./trady-eur-gateway.service): durable private-owner
  outbox delivery as gateway.
- [trady-eur-demo-month.service](./trady-eur-demo-month.service) and
  [release environment example](./eur-demo-month-release.env.example): disabled
  preparation for the verified finite DEMO supervisor. The root helper issues
  an executor-readable receipt only after complete evidence and owner approval.
- [trady-daily.timer](./trady-daily.timer) and
  [EUR weekly timer](./trady-eur-weekly-report.timer): existing calendar/research
  and review handoffs. A timer cannot approve a release.
- [Database backup timer](./trady-postgres-backup.timer): private persistent-volume
  backups; recovery verification remains separate from activation.

[prepare.py](./prepare.py) accepts only EUR configuration, forces
`enabled=false` and `execution_stage=SHADOW`, and exclusively creates
`eur-shadow.json`. Existing output is never overwritten. The separate
[SHADOW deployment helper](../../scripts/prepare-eur-shadow.py) validates its
recorded source/owner delegation before the explicit SHADOW install path.
Neither helper activates DEMO orders.

The old IEX30 worker/executor unit templates are
[archived as text](../../docs/archive/legacy-runtime/README.md). Their CLI and
config-preparation paths were retired; these archived units must not be installed.

## Shared offline and research services

The generic Telegram gateway/worker templates retain scoped research dialogue,
not the removed private trading analyst child. The weekly lab and evaluator
retain separate identities, immutable helper pins and private IPC. The
evaluator has no credential load and denies new/outbound sockets and io_uring;
its accepted worker socket and client UID remain independently checked.

[sysusers.conf](./sysusers.conf), [firewall.nft](./firewall.nft) and
[PostgreSQL policy](./postgres/pg_hba.conf) define separate identity/network/DB
boundaries. Empty allowlists grant no provider access. A service credential
contains only that identity's required values; no unit embeds secret text.
The lab/evaluator cannot read broker or activation-owner credentials.

## Sources and recovery

The EUR coordinator uses exact native broker metadata/calendar/account checks,
verified current IBKR candles and separately timestamped live type-1 bid/ask.
Yahoo/Alpha remain historical/news research; X stays metadata-only. Raw failures
are retained and cannot become a fallback quote or a successful paper day.

An operator can capture read-only diagnostic inputs with
`trady eur diagnose-inputs --config /etc/trady/runtime/eur-shadow.json`.
Unknown submissions and interrupted deliveries remain ambiguous until exact
reconciliation; a service restart never grants permission to retry them.

Source cleanup preserves migrations, immutable evidence and recovery artifacts.
Any rebuilt deployment must be tested, reviewed and bound to a new exact bundle;
no old activation receipt approves changed bytes.
