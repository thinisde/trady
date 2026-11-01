# Getting credentials

These READMEs explain how to obtain every value in
[credentials.example.json](../credentials.example.json). Provider setup steps and
official links were checked on 2026-10-31.

## Start here

From the repository root, initialize the private store:

```sh
cargo run --locked --bin trady -- credentials init
```

If `config/credentials.local.json` already exists, edit it directly. Put values
inside its `credentials` object under the exact names below. Keep unused entries
empty and keep the committed example blank. The private file is ignored by Git
and uses owner-only permissions on Unix. Environment variables override file
entries; see the [storage guide](../../docs/credentials.md) for alternate paths.

## Interface

`[../../crates/trady-adapters/src/credentials.rs:CredentialStore]` exposes
`load`, `optional`, `required` and metadata-only `status`. The
[blank template](../credentials.example.json) lists the centrally recognized names.

### Guides for each credential

- [Financial data providers](./financial-data/README.md): SEC contact User-Agent,
  FRED, Twelve Data, Finnhub, FMP, CoinGecko, Tiingo and Massive.
  All use the same central private credential file.

- [OpenCode Go](./opencode/README.md): `OPENCODE_API_KEY`.
- [Telegram](./telegram/README.md): `TRADY_TELEGRAM_BOT_TOKEN`,
  `TRADY_TELEGRAM_WHITELIST_USER_ID`.
- [Trading 212](./trading212/README.md): `TRADY_T212_DEMO_API_KEY`,
  `TRADY_T212_DEMO_API_SECRET`, `TRADY_T212_LIVE_API_KEY`,
  `TRADY_T212_LIVE_API_SECRET`. Exactly one key/secret pair per environment.
- [Trading inputs service](./trading-inputs/README.md): client
  `TRADY_TRADING_INPUTS_TOKEN` and server `TRADY_INPUTS_SERVICE_TOKEN`.
- [X](./x/README.md): `TRADY_X_BEARER_TOKEN`, `TRADY_X_CONSUMER_KEY`,
  `TRADY_X_CONSUMER_SECRET`.
- [Alpha Vantage](./alpha-vantage/README.md): `TRADY_ALPHA_VANTAGE_API_KEY`.
- [Alpaca Basic market data](./financial-data/README.md): `TRADY_ALPACA_API_KEY`,
  `TRADY_ALPACA_SECRET_KEY`.
- [PostgreSQL](./postgresql/README.md): `TRADY_ANALYST_DSN`,
  `TRADY_EXECUTOR_DSN`, `TRADY_GATEWAY_DSN`, `TRADY_MIGRATOR_DSN`, and the
  separate lab, verifier, shadow, owner-approval and release DSNs.
- [Locally generated secrets](./local-secrets/README.md):
  `TRADY_HASHING_SECRET_KEY`, `TRADY_SIGNING_SECRET_KEY`.

Provider keys come from the provider's account console. Database connection
strings come from your database administrator. The trading-inputs token comes
from that service's operator. Hashing/signing secrets are generated locally and
currently have no active consumer.

## Check storage

```sh
cargo run --locked --bin trady -- credentials status
```

This reports names, availability and source without printing values or contacting
providers. A configured value does not confirm provider access. Each provider's
feature configuration and account bindings still apply.

## Invariants

- `[../../crates/trady-adapters/src/credentials.rs:CredentialStore::resolve]`
  gives environment entries precedence, including explicitly empty values.
- `[../../crates/trady-adapters/src/credentials.rs:SecretValue]` redacts debug
  output and exposes values only through explicit access methods.
- Provider credentials, database connection strings and service tokens belong in
  the private central file or environment, with no values in committed examples.

## Side Effects

Credential loading reads the selected file and environment. Explicit
`[../../crates/trady-adapters/src/credentials.rs:initialize]` creates a private
template file and refuses overwrite. Status commands perform no provider requests.

## Verification

```sh
cargo test --locked -p trady-adapters --test credentials
cargo test --locked -p trady-cli --test credentials_cli
```
