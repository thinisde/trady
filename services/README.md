# Inference and runtime services blueprint

## System Overview

Optional FinBERT inference and reviewable runtime deployment artifacts. FinBERT
is called through Rust application ports; the runtime units separately launch
analyst, executor, Telegram gateway, weekly lab and sealed evaluator processes.

## Subdirectories & System Boundaries

- [Research and market inputs](./inputs/README.md): authenticated, loopback-only
  reads from immutable, independently reviewed source imports, calendars,
  market observations and the offline model-lab bundle. `[inputs-service]
  check/import/review/inspect/export-universe/deadline/prune` are local operator
  commands; `[inputs-service/http.rs:InputsService::respond]` exposes only the
  bounded read routes described by the component contract.
- [FinBERT](./finbert/README.md): CPU English text classification with a bounded
  HTTP contract, explicit Docker deployment and model-cache configuration.
  `[finbert/serve.py:Handler.do_GET]` exposes
  `GET /health` and `[finbert/serve.py:Handler.do_POST]` exposes
  `POST /v1/sentiment`. The current development server binds `0.0.0.0:8000`;
  that bind is not acceptable for supervised private deployment and remains an
  explicit blocker until the process is changed to loopback/private ACL binding.
  Compose publishes the development container at `127.0.0.1:8091`
  (`[finbert/serve.py:main]`,
  `[finbert/compose.yaml:services.finbert]`). The [v1 design](../docs/v1/finbert/README.md)
  binds these observations to Rust adapters and conservative threshold constraints.
- [Runtime deployment](./runtime/README.md): disabled configuration preparation,
  distinct service identities, private IPC, reviewed egress rules and units/timer.
  `[runtime/prepare.py:main]` exposes `--source FILE --destination DIRECTORY` to
  prepare analyst/executor configurations. `[runtime/trady-weekly.timer:OnCalendar]`
  schedules Saturday 08:00 Europe/Berlin and
  `[runtime/trady-evaluator.service:ExecStart]` launches the sealed worker; the
  timer declares `Persistent=yes` and `AccuracySec=1min`.

## Global Invariants

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
The evaluator unit also denies `connect` and `io_uring`
(`[runtime/trady-evaluator.service:SystemCallFilter]`); the worker installs its
mandatory additional filter (`[../lab/evaluator/isolation.py:isolate_worker]`).
The FinBERT offline override requires cached pinned artifacts and supplies
application offline flags; network isolation remains a deployment boundary
(`[finbert/compose.offline.yaml:services.finbert.environment]`).
The inputs service requires nonempty source and reviewer registries and a
loopback listener (`[../crates/trady-adapters/src/inputs_service/mod.rs:InputsServiceConfig::validate]`);
its bearer-token HTTP boundary exposes reads only and cannot import, review or
approve models (`[../crates/trady-adapters/src/inputs_service/http.rs:InputsService::respond]`).

## Global Side Effects

Explicit `[finbert/serve.py:main]` startup can download model artifacts, bind an
HTTP listener and consume CPU/RAM. The
[31 October local diagnostic](../docs/live-demo-run-2026-10-31.md#actual-integrations)
records a separately built and running instance; shipped client configuration
remains disabled.
`[runtime/prepare.py:main]` writes new disabled files sequentially and can leave
partial outputs. Explicit evaluator startup creates its exposure directory
(`[runtime/trady-evaluator.service:ExecStartPre]`). Deployment and enabled
executor operation have the separate effects described in the runtime contract.
Inputs operator commands write the synchronized import store, while explicit
`inputs-service serve` starts bounded loopback workers; neither operation fetches
live provider data or submits orders.

## System Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s services/finbert/tests -v`
and the configuration-only Compose checks in the [component contract](./finbert/README.md).
Run the inputs/model-lab checks listed in the [inputs contract](./inputs/README.md).
Runtime unit/firewall validation commands and deployment prerequisites are in
the [runtime contract](./runtime/README.md).
