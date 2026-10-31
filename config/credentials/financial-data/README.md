# Financial provider credentials

## Overview

Financial adapters resolve fixed names through the existing
[central store](../../../docs/credentials.md). Keep values in the ignored private
`config/credentials.local.json` or existing environment overrides. This directory
contains setup documentation only.

## Interface

| Central credential name | Obtain/configure |
|---|---|
| `TRADY_SEC_USER_AGENT` | Your truthful application/operator identity and monitored contact email, as required by [SEC fair access](https://www.sec.gov/search-filings/edgar-search-assistance/accessing-edgar-data) |
| `TRADY_FRED_API_KEY` | Your own key from [FRED](https://fred.stlouisfed.org/docs/api/api_key.html) |
| `TRADY_TWELVE_DATA_API_KEY` | Your [Twelve Data](https://twelvedata.com/) dashboard key after verifying the requested plan/venue |
| `TRADY_FINNHUB_API_KEY` | Your [Finnhub](https://finnhub.io/) account key for company news |
| `TRADY_FMP_API_KEY` | Your [Financial Modeling Prep](https://site.financialmodelingprep.com/) account key for the stable income-statement endpoint |
| `TRADY_COINGECKO_API_KEY` | Your [CoinGecko Demo API](https://docs.coingecko.com/demo/reference/introduction) key; no Pro hostname fallback |
| `TRADY_TIINGO_API_KEY` | Your [Tiingo](https://www.tiingo.com/) account token for the reviewed EOD endpoint |
| `TRADY_MASSIVE_API_KEY` | Your [Massive](https://massive.com/) account key for the reviewed daily aggregate route |

Alpha Vantage keeps its [existing credential entry](../alpha-vantage/README.md).
GDELT's implemented public route and offline yfinance imports use no API key.
`[../../../crates/trady-domain/src/financial.rs:Provider::credential_name]`
defines the exact mapping; adapters accept no configurable secret-name override.

For a new installation use `trady credentials init`. For an existing private
file, add the needed names from the [blank template](../../credentials.example.json)
without replacing existing entries. Fill values in your editor. Check metadata
with `trady credentials status` and `trady financial status`.

## Invariants

- `[../../../crates/trady-adapters/src/credentials.rs:CredentialStore::resolve]`
  preserves environment-over-file precedence, including explicit empty overrides.
- `[../../../crates/trady-adapters/src/credentials.rs:SecretValue]` has redacted
  debug output and no serialization implementation. Status exposes availability only.
- Provider credentials never belong in research registries, LLM tools or committed
  documentation. SEC's contact is configured by the operator; the example stays blank.
- A credential establishes neither endpoint entitlement nor consumer rights.

## Side Effects

Credential resolution reads environment/private files. Explicit initialization
creates a private file and refuses overwrite. These instructions and status commands
perform no provider request. Reviewed collection uses the [financial data path](../../../docs/financial-data.md).

## Verification

```sh
cargo test --locked -p trady-adapters --test credentials
cargo test --locked -p trady-cli --test financial_cli
```
