# Central credentials

All Rust credential readers use
[`CredentialStore`](../crates/trady-adapters/src/credentials.rs): Trading 212,
Telegram, OpenCode Go, X/news, financial data and the database roles. Provider configuration
files contain settings and credential names; secret values belong here or in the
process environment.

The [credential setup READMEs](../config/credentials/README.md) explain how to
obtain every entry, with official provider links and separate demo/live T212 steps.

## Setup

From the repository root:

```sh
cargo run --locked --bin trady -- credentials init
```

This creates `config/credentials.local.json` from the
[blank template](../config/credentials.example.json). On Unix it creates the file
with mode `0600`. Initialization refuses to overwrite an existing file.
Edit the local file with your editor, filling only the credentials you need.
Leave unused entries empty. The local file and `config/credentials*.local.json`
variants are ignored by Git. The committed example must remain blank.

Check availability without printing values or contacting providers:

```sh
cargo run --locked --bin trady -- credentials status
```

Status lists each known or file-defined name, a `configured` boolean, and its
source (`environment`, `file`, or `missing`). It does not validate credentials
with a provider or report trading readiness.

## Names and resolution

Financial adapters register `TRADY_SEC_USER_AGENT`, `TRADY_FRED_API_KEY`,
`TRADY_TWELVE_DATA_API_KEY`, `TRADY_FINNHUB_API_KEY`, `TRADY_FMP_API_KEY`,
`TRADY_COINGECKO_API_KEY`, `TRADY_TIINGO_API_KEY` and
`TRADY_MASSIVE_API_KEY` in the central template/status
controller. SEC requires the operator's actual contact-bearing User-Agent.
GDELT and offline yfinance need no new secret; Alpha Vantage keeps its existing
entry. See [setup](../config/credentials/financial-data/README.md) and
[the integration guide](./financial-data.md). Existing files are preserved;
add required names to their `credentials` object instead of reinitializing them.

The template covers `OPENCODE_API_KEY`, `TRADY_TELEGRAM_BOT_TOKEN`,
`TRADY_TELEGRAM_WHITELIST_USER_ID`, separate Trading 212 demo/live API key/secret pairs,
`TRADY_TRADING_INPUTS_TOKEN`, `TRADY_X_BEARER_TOKEN`, `TRADY_X_CONSUMER_KEY`,
`TRADY_X_CONSUMER_SECRET`, `TRADY_ALPHA_VANTAGE_API_KEY`, and
`TRADY_ANALYST_DSN`, `TRADY_EXECUTOR_DSN`, `TRADY_GATEWAY_DSN`,
`TRADY_MIGRATOR_DSN`, `TRADY_LAB_DSN`, `TRADY_VERIFIER_DSN`, `TRADY_SHADOW_DSN`,
`TRADY_RELEASE_OWNER_DSN` and `TRADY_RELEASE_DSN`. The private input server uses
`TRADY_INPUTS_SERVICE_TOKEN`; its authorized client uses the same bearer value
under `TRADY_TRADING_INPUTS_TOKEN`. Keep each deployment identity's file limited
to its own secret set; the example lists names for setup, not shared authority.

`TRADY_TELEGRAM_WHITELIST_USER_ID` optionally stores one verified private
Telegram user ID as a decimal string. When set, Telegram configuration loading
uses that ID for both user/chat allowlists before validation. When absent or
empty, the configuration's explicit allowlists apply. Invalid configured IDs
fail without echoing their value. The
[Telegram setup README](../config/credentials/telegram/README.md) explains ID
discovery and access-list precedence.

Trading 212's `environment` field selects the endpoint and credential pair for
T212 clients, including snapshots and Telegram broker monitoring:

- `"demo"`: `TRADY_T212_DEMO_API_KEY` and `TRADY_T212_DEMO_API_SECRET`.
- `"live"`: `TRADY_T212_LIVE_API_KEY` and `TRADY_T212_LIVE_API_SECRET`.

These are the four T212 entries: one API key and one API secret per environment.
Both pairs can be stored in the same private file. The selected pair must be
complete; credentials from the other mode are never used as a fallback.
See the [T212 setup](./trading212.md) for selecting the account-bound demo/live
configuration.

The X app's consumer key and consumer secret are stored as
`TRADY_X_CONSUMER_KEY` and `TRADY_X_CONSUMER_SECRET`. The current X lookup and
timeline readers use `TRADY_X_BEARER_TOKEN`; the consumer pair is available in
the store for integrations that need it. See the
[X setup README](../config/credentials/x/README.md) for all three entries.

`TRADY_HASHING_SECRET_KEY` and `TRADY_SIGNING_SECRET_KEY` provide storage names
for future keyed hashing/signing consumers. They are optional and are not used
by today's unkeyed SHA-256 content fingerprints. Supply independently generated
keys when adding a consumer; this store does not generate keys or define a
cryptographic protocol.

Additional API keys, shared secrets, passwords and private keys can be added under
`credentials`. Names accept 1–96 uppercase ASCII letters, digits or underscores;
values must be JSON strings. Multiline keys use JSON newline escapes (`\n`).
An integration still needs to request the corresponding name: OpenCode's
`api_key_env` and research sources' `credential_env` retain their existing names
and can reference either an environment value or a file entry.

For each requested name, the process environment takes precedence over the file.
An explicitly empty or whitespace-only environment value disables that credential
and does not fall back to the file. Empty file values mean unconfigured.
Required credentials fail with their name; optional database bindings can remain
unconfigured. No code modifies the process environment.

By default, the file path is relative to the working directory. Set
`TRADY_CREDENTIALS_FILE` to an absolute path when starting Trady elsewhere, or to
select a different store. Both `init` and credential consumers honor it. A missing
default file allows environment-only deployment; a missing explicitly selected
file is an error. Store changes take effect on the next lookup or process start;
already constructed clients retain their current credentials.

## Deployment and protection

The local JSON file stores plaintext secrets. File permissions and the deployment's
filesystem protect it; it is not an encrypted vault. On Unix, reads reject any
group/other permission bits; use `chmod 600` after copying or editing a file if
your editor changed its permissions. On other systems, configure equivalent
filesystem access controls. Use environment injection from your secret manager
when that suits the deployment.

Keep production stores scoped to each process. For example, select
`config/credentials.analyst.local.json` for the analyst, with only its database
role and model/research keys. The executor, gateway and migrator should receive
their own files or injected environment values. A single development file does
not replace the [role boundaries](./v1/architecture.md).

The loader bounds files to 256 KiB and each value to 64 KiB. It rejects invalid
UTF-8, duplicate JSON keys, unknown top-level fields, unsupported versions,
invalid names, non-string values and NUL characters. Errors omit parser details
and credential values. `SecretValue` and `CredentialStore` redact `Debug`, and
secret values do not implement serialization. Provider-specific validation still
applies when constructing clients.

For rotation, replace the relevant file entry or injected environment value and
restart clients using it. Keep demo/live broker stores separate and preserve the
existing account bindings and API key permissions.

## Verification

```sh
cargo test -p trady-adapters --test credentials --locked
cargo test -p trady-cli --test credentials_cli --locked
```

These checks use temporary files and fixture values. They cover strict parsing,
redaction, permissions, initialization without overwrite, custom paths,
environment precedence and broker CLI lookup without provider requests.
