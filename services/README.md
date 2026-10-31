# Inference and runtime services blueprint

## System Overview

Optional FinBERT inference and reviewable runtime deployment artifacts. FinBERT
is called through Rust application ports; the runtime units separately launch
analyst, executor, Telegram gateway, weekly lab and sealed evaluator processes.

## Subdirectories & System Boundaries

- [FinBERT](./finbert/README.md): CPU English text classification, bounded HTTP
  contract, explicit Docker deployment and model-cache configuration.
- [Runtime deployment](./runtime/README.md): disabled configuration preparation,
  distinct service identities, private IPC, reviewed egress rules and units/timer.

## Interface

`[finbert/serve.py:Handler]` exposes health and sentiment HTTP endpoints.
The [v1 design](../docs/v1/finbert/README.md) binds these observations to Rust adapters
and conservative threshold constraints.
`[runtime/prepare.py:main]` prepares analyst/executor configurations;
`[runtime/trady-weekly.timer:OnCalendar]` schedules Saturday 08:00 Europe/Berlin
and `[runtime/trady-evaluator.service:ExecStart]` launches the sealed worker.

## Invariants

`[finbert/serve.py:__main__]` guards model/listener startup;
`[../config/finbert/config.example.json:enabled]` is false and
`[../services/finbert/compose.yaml:services.finbert.profiles]` makes the service opt-in.
Inference cannot mutate a trading plan or authorize an order.
`[runtime/prepare.py:main]` forces disabled configs and refuses overwrite;
`[runtime/prepare.py:unique]` rejects duplicate keys. Complete application
validation remains separate. `[runtime/firewall.nft:trady_runtime]` starts with
empty route sets; weekly/evaluator units allow only `AF_UNIX`
(`[runtime/trady-weekly.service:RestrictAddressFamilies]`,
`[runtime/trady-evaluator.service:RestrictAddressFamilies]`).

## Side Effects

Explicit `[finbert/serve.py:main]` startup can download model artifacts, bind an
HTTP listener and consume CPU/RAM. The service has not been built or launched by
this change.
`[runtime/prepare.py:main]` writes new disabled files sequentially and can leave
partial outputs. Explicit evaluator startup creates its exposure directory
(`[runtime/trady-evaluator.service:ExecStartPre]`). Deployment and enabled
executor operation have the separate effects described in the runtime contract.

## Verification

Run `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s services/finbert/tests -v`
and the configuration-only Compose checks in the [component contract](./finbert/README.md).
Runtime unit/firewall validation commands and deployment prerequisites are in
the [runtime contract](./runtime/README.md).
