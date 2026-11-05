# EUR SHADOW identity and source review

Review reference: `eur-shadow-identity-source-review-2026-11-04`.
Reviewed by Codex on 4 November 2026 at the user's request. This is an agent
self-review of primary evidence. It does not claim an independent reviewer or
owner release approval. Broker mutations: **zero**. The full SHADOW cycle and
paper-day replay have not passed.

Use [crosswalk.json](./crosswalk.json) for the captured identities and
[review.json](./review.json) for source decisions and remaining blockers.
[hashes.json](./hashes.json) pins these documents, configuration, capture tools,
and local observations. Raw responses and CLI readbacks are retained locally
under `var/eur-shadow-review/2026-11-04/`; they are not committed or sent to a
model as a review bundle. Failed and redirected requests remain visible.

## Crosswalk

The demo account identity read returned account `52211463`, currency EUR.
The catalog contained 17,143 instruments. The two reviewed research identities
are:

| Instrument | Broker ticker | ISIN | Listing | Yahoo | Alpha Vantage |
| --- | --- | --- | --- | --- | --- |
| Airbus SE | `AIRp_EQ` | `NL0000235190` | Euronext Paris / XPAR | `AIR.PA` | `AIR.PAR` |
| ASML Holding N.V. | `ASMLa_EQ` | `NL0010273215` | Euronext Amsterdam / XAMS | `ASML.AS` | `ASML.AMS` |

The broker's exchange/schedule pairs are `58/76` and `44/57`. On the capture
date Paris opens at 08:00:00 UTC and Amsterdam at 08:00:03 UTC; both close at
16:30:00 UTC. These are captured broker schedule observations, not a complete
reviewed exchange calendar or proof of the execution venue. Refresh them before
use. The identity bundle expires 24 hours after catalog capture.

[Airbus's share information](https://www.airbus.com/en/investors/share-price-and-information)
confirms its legal identity and ISIN. The old example's `FR0000120404` belongs
to Accor, also confirmed by the catalog. Both illustrative Airbus mappings have
been corrected. Airbus's Xetra ticker `AIRd_EQ` is a different listing and is
excluded from the Paris mapping.

[ASML's share table](https://investor.asml.com/share-information) distinguishes
the Amsterdam EUR listing from its NASDAQ USD registration. The table was
read through the web retrieval tool; direct capture returned a redirect and
then HTTP 403. Those failures were retained, not relabeled successful captures.

Yahoo chart metadata independently returned the selected symbol, EUR currency,
Paris/Amsterdam exchange name, timezone and issuer name. Alpha Vantage symbol
search returned the selected EUR regions. Plain `ASML` in Alpha Vantage is the
United States USD listing. Alpha's old Airbus display name and supplied UTC
offsets must not become a session calendar.

**Neither instrument is orderable from this bundle.** Minimum quantity, quantity
step, price tick and fractional support are explicitly unverified. The
[Trading 212 instrument API schema](https://docs.trading212.com/api/instruments/instruments)
does not provide them. A Yahoo display precision or broker maximum quantity
does not prove order precision. The crosswalk uses nulls rather than invented
rules and cannot be supplied as a `universe snapshot --review` approval file.
For the eventual first supervised session, keep the scope to Airbus alone
until the execution evidence is accepted.

## Yahoo observations

[yfinance.eur-shadow.reviewed.json](../../../config/yfinance.eur-shadow.reviewed.json)
contains exact read-only provider bindings for these two identities. `enabled`
in that provider file permits reads; it does not activate the runtime.

Both diagnostic quote requests succeeded as observations. Both had missing bid
and ask, were older than ten seconds, and returned `tradable=false` and
`quote_quality=STALE`. They cannot authorize entry. Listing identity review does
not resolve the known Trading 212 OTC execution compatibility blocker.

## Alpha Vantage

Two authenticated `SYMBOL_SEARCH` reads succeeded using
`TRADY_ALPHA_VANTAGE_API_KEY`. The previous registry referenced a different,
unconfigured credential name. No credential value appears in this bundle.

[The published free limit](https://www.alphavantage.co/support/) is 25 requests
per day. The registry now caps its optional source at 20 and leaves it disabled.
The spare five requests are not a global reservation: other consumers still
need one shared daily budget. No paid entitlement was inferred from a valid key.

The intended role is historical research or crosscheck, never executable
quotes. Symbol-search support does not establish `NEWS_SENTIMENT` coverage.
The current news adapter cannot implement that historical role, so its source
remains disabled rather than querying guessed tickers.
[The terms](https://www.alphavantage.co/terms_of_service/) license private,
individual research and distinguish commercial use. Commercial deployment and
third-party content processing rights were not established. The captured terms
are a PDF, preserved in base64 with the original byte hash.

## RSS

The [official ECB feed](https://www.ecb.europa.eu/rss/press.html) was captured,
parsed by the existing collector and persisted through the analyst PostgreSQL
role: 15 items, with five checked through a separate model-facing readback.
The collector explicitly reports that the feed snapshot is not complete history.

[ECB usage conditions](https://www.ecb.europa.eu/services/using-our-site/disclaimer/html/index.en.html)
allow use with accurate attribution and additional conditions for modified or
resold information. The enabled source covers direct feed titles and supplied
summaries, with ECB attribution and 72-hour retention. It does not fetch full
linked papers or third-party content. Author-named papers have separate rights.

[Airbus's RSS directory](https://www.airbus.com/en/rss-feeds) links to its
[current press feed](https://www.airbus.com/en/rss-press-releases-feeds).
The feed is reachable, but the runtime parser rejects its timezone-free,
nonstandard dates. PostgreSQL records `UNAVAILABLE` / `rss_parser_unhealthy`,
with zero collected items. No timezone was guessed. The source is disabled.
[Airbus's terms](https://www.airbus.com/en/terms-of-use) also do not establish
content extraction or model-processing permission; both content flags are false.
The unreviewed Reuters and Google News examples were removed from the EUR
registry rather than kept as approved publishers.

## X

Official-site ownership links and the authenticated X API agree on:

| Owner | Official handle | Stable user ID | Ownership reference |
| --- | --- | --- | --- |
| Airbus | `@Airbus` | `15425377` | [Issuer website](https://www.airbus.com/en/investors/share-price-and-information) |
| European Central Bank | `@ecb` | `83466368` | [ECB official channels](https://www.ecb.europa.eu/services/socialmedia/html/index.en.html) |

Both IDs were resolved through `trady news resolve-x` and the existing
PostgreSQL X cost ledger. Two user reads cost an estimated USD 0.02; the shared
budget reports USD 1.88 remaining. The historical billing-scope name
`iex30_x_balance` identifies the existing provider budget, not market authority.
No second budget or new owner budget approval was created.

The EUR registry now records both identities and their actual resolution times.
Post access and recent activity were not tested; no posts were requested.
Collection and content/model processing remain disabled. Identity resolution
does not prove a usable post entitlement or affordable polling coverage.
[X's developer guidance](https://docs.x.com/developer-guidelines) and
[license terms](https://legal.x.com/data-terms/us/mla.html) impose data-use
restrictions; the agreement applicable to this account and its permission for
the proposed LLM processing were not established. A generic license page or
working bearer token is not an account-specific processing approval.

## Validation and next use

From the repository root:

```sh
python3 scripts/verify-eur-shadow-review.py
trady news status --config config/research.eur.json
trady yfinance quote --config config/yfinance.eur-shadow.reviewed.json --instrument AIR_PA_EQ
```

The offline verifier checks hashes, exact catalog/ISIN/schedule joins, provider
regions, X stable IDs and ownership links, the shared budget, and the restrictive
source decisions. `--integrity-only` permits historical inspection after expiry;
its output still reports whether the bundle is current.

The disabled runtime example remains disabled, with LIVE blocked. Its full
universe example remains illustrative apart from the corrected Airbus ISIN.
This bundle supplies factual identity review; it does not manufacture the
missing order precision, source entitlement, coordinator/gateway integration,
paper-day evidence, protection capability or owner release approval. See
[the runtime status](../../eur-first-run-status.md) for the remaining work.
