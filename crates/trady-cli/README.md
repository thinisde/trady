# Trady command line

The `trady` binary composes the EUR workflow, read-only research tools and
offline validation. `autonomous check|run` accepts only EUR configurations.
The retired IEX30 runner, private analyst child, account/cold-start CLI,
Alpaca/watchdog commands and synthetic `demo-entry` command are removed.

- [Source contract](./src/README.md): current command routing and effects.
- [EUR workflow](../../docs/eur-execution-and-release.md): durable research,
  exact release verification, DEMO protection/recovery and real-input paper.
- [Retirement record](../../docs/eur-legacy-retirement-2026-11-05.md): removed
  runtime paths and the shared replay/database contracts retained.
- [OpenCode](./src/opencode/README.md), [Telegram](./src/telegram/README.md),
  [input/model service](../../services/inputs/README.md) and
  [weekly lab](../../lab/README.md): separate research and offline tools.

`eur capture` uses analyst credentials; `autonomous run` coordinates SHADOW
with executor credentials; `eur gateway` delivers the private-owner outbox.
SHADOW never submits orders. `eur demo-run` and `eur protocol-drive` require
the exact accepted bundle and root-owned owner authority. LIVE remains blocked.

The shared `analyst|executor|gateway --once`, `demo` and `demo-postgres`
commands remain foundation/database diagnostics, without a broker mutation
client. `t212 snapshot --config FILE` is a read-only, explicitly scoped account
check. `validate-json` checks shape without granting entry authority.

Research commands include `news`, `research-report`, `financial`,
`intelligence`, `sentiment`, `yfinance`, `chat`, `sessions`,
`skill-manifest` and `api-health`. The EUR executable book remains IBKR
SMART live type-1; research commands cannot provide a quote fallback.

`inputs-service`, `model-lab`, `weekly` and `release` retain reviewed imports,
offline model/formula evaluation and separate research-release identities.
An old research release cannot authorize an EUR order or bypass its verifier.

Run `cargo test -p trady-cli --locked` and
`python3 scripts/test-postgres.py` from the workspace root. Ordinary tests
exclude the explicitly authorized native DEMO capability experiment.
