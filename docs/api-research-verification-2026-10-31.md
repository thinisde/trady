# API and research verification — 31 October 2026

Scope: read-only tests of the other configured APIs, current US/EU stock research for a moderate-risk 6–12 month horizon, and two new CLI features. **No broker orders, cancellations, Telegram messages or source activations were performed.** The existing `deepseek-v4.1-flash` pin was retained.

## Live API results

The first run at **19:38:41 UTC** made 13 logical diagnostic attempts: **12 APIs returned HTTP 200 with the expected payload shape; GDELT returned no HTTP response**. Full metadata is in [the saved JSON](./api-health-2026-10-31.json).

| API | HTTP | Result | Response records |
| --- | ---: | --- | ---: |
| SEC | 200 | healthy | 1,000 recent filing entries |
| FRED | 200 | healthy | 1 observation |
| Twelve Data | 200 | healthy | 1 price bar |
| Finnhub | 200 | healthy | 1 quote |
| Alpha Vantage | 200 | healthy | 1 quote |
| FMP | 200 | healthy | 1 financial statement |
| CoinGecko | 200 | healthy | 1 market record |
| Tiingo | 200 | healthy | 5 price bars |
| Massive | 200 | healthy | 1 price bar |
| GDELT | No response | transport_failed | — |
| Google News RSS | 200 | healthy | 38 entries |
| BBC business RSS | 200 | healthy | 50 entries |
| Telegram getMe | 200 | healthy | 1 bot identity |
| X | Not requested | requires_existing_cost_gate | — |
| Yfinance | Not requested | offline_import_only | — |

GDELT's first failure took 5,003 ms. Independent DNS resolution subsequently succeeded, but the targeted second GET also failed after 5,018 ms with safe code `http_transport_failed`. Its [separate retest output](./api-health-gdelt-2026-10-31.json) preserves that result. This establishes an unresolved HTTPS transport failure, not a confirmed API authentication or data-parser defect. Total diagnostic attempts including that retest: **14**. No additional retries or alternative paid endpoints were used.

The offline FRED smoke check returned `credential_present`, `http_status: null`, `network_requests: 0`, `collection_settings_changed: false` and `entry_authorized: false`.

Successful endpoint checks do not approve all provider endpoints, data rights or deployment capabilities. The production financial/news registries remain disabled. Responses were validated in memory; saved diagnostics retain status, counts and timing rather than payloads, private identities, credential values or request URLs.

## Implemented features

- **API diagnostics:** `trady api-health [--online] [--provider NAME]`. The default makes no requests; online checks use fixed bounded GET routes, distinguish HTTP/payload failures, retain safe transport codes and preserve X's cost gate. [Guide](./api-health.md), [adapter](../crates/trady-adapters/src/api_health.rs), [CLI](../crates/trady-cli/src/api_health.rs).
- **Stock research reports:** `trady research-report FILE [--as-of RFC3339]`. The offline generator validates strict dated inputs, computes period-matched growth/margins and optional trailing P/E, and keeps missing evidence visible. [Guide](./stock-reports.md), [application logic](../crates/trady-application/src/stock_report.rs), [CLI](../crates/trady-cli/src/stock_report.rs).

Both commands are wired into help and documented in the source README contracts. No dependency or database schema was added for these features.

## Stock research and calculations

The [sourced research](./market-research-2026-10-31.md) prioritizes **Visa and SAP for business-quality review**, followed by **Microsoft with a capital-spending condition**. Visa's issuer closing quote implies a premium 33.5706× trailing P/E. Schneider Electric and Sanofi remain follow-up candidates with explicit missing financial inputs. This is a focused research shortlist rather than a full-market ranking or a buy recommendation.

The generator successfully processed the [dated input](./research-us-eu-2026-10-31.input.json); [actual JSON output](./research-us-eu-2026-10-31.json) records `as_of: 2026-10-31T19:38:05Z`. An independent Python Decimal computation matched every supplied growth and margin calculation.

Example returned values:

```json
{
  "symbol": "V",
  "fundamentals": "SUPPORTED",
  "revenue_growth_pct": "14.7575",
  "operating_margin_pct": "61.2378",
  "free_cash_flow_margin_pct": "48.0513",
  "pe_ttm": "33.5706",
  "evidence_gaps": [],
  "valuation_review_required": true,
  "entry_authorized": false
}
```

Visa's verified issuer closing quote is $413.59 on 30 October; $413.59 / $12.32 annual diluted EPS gives 33.5706× trailing P/E, independently checked with Decimal. The other four candidates have missing quote/earnings inputs. No expected returns or target prices were generated. The report retains supplied source references; it does not turn them into runtime verification attestations. The research record distinguishes reported and constant-currency growth, annual and quarterly periods, issuer FCF measures and cash-capex calculations.

## Verification

| Check | Latest result |
| --- | --- |
| `cargo build --bin trady --locked` | Passed; both new commands executed successfully |
| `cargo test --workspace --locked --no-fail-fast` | **498 passed, 0 failed; 20 isolated-PostgreSQL tests ignored** |
| New feature tests, included above | **10 passed**: six API diagnostics tests and four stock-report tests |
| Independent Decimal checks | All supplied report ratios matched |
| Formatting of the new feature files and CLI wiring | Passed |
| Full workspace formatting | Pending differences in concurrent intelligence source/tests and storage wiring |
| Strict workspace Clippy | Failed on concurrent intelligence lints; not represented as a clean lint pass |
| Warning-level Clippy sweep across all targets | Completed with **no diagnostics in the four added feature modules**; concurrent warnings remained |
| Repository asset validation and `git diff --check` | Passed |

The first broader test run saw three failures in the concurrently changing intelligence target. That target then passed all eight tests, and the subsequent complete workspace run passed all 498. No intelligence source or tests were changed here to resolve those failures.

Outstanding strict lints include large intelligence enum variants, nested conditionals, a many-argument intelligence read function and an intelligence repository type-complexity warning. Formatting differences are likewise confined to that concurrent work. It remains visible in the working tree and is preserved.

The 20 database tests were not rerun through a disposable PostgreSQL instance during this extension; the earlier [demo verification](./demo-verification-2026-10-31.md) records its separate database checkpoint. The new commands require no database and performed no database writes. The older 537-test total is a historical checkpoint, not the count of the current Rust run.

## Reproduce

```sh
cargo build --bin trady --locked
target/debug/trady api-health
target/debug/trady api-health --online --provider fred
target/debug/trady research-report docs/research-us-eu-2026-10-31.input.json --as-of 2026-10-31T19:38:05Z
```

Online diagnostics consume the selected API account's ordinary request allowance; they do not enable a background collector or make a paid fallback. Stock report generation is offline.

