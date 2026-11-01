# Selected setup for the no-submit demo

The owner delegated the instrument and setup choices. The demo now allows
Microsoft (`MSFT_US_EQ`), JPMorgan (`JPM_US_EQ`), Johnson & Johnson (`JNJ_US_EQ`),
ExxonMobil (`XOM_US_EQ`) and Procter & Gamble (`PG_US_EQ`). This is a small research
set spanning different businesses, chosen to exercise the existing five-candidate
limit. It is not a portfolio allocation or an expected-return ranking.

The [runtime configuration](../config/autonomous.demo-no-submit.json) contains
the selection. The [deployment selection record](../config/deployment.demo-selection.json)
records provider choices, access results and outstanding requirements. That record
is an operator planning artifact, not a runtime approval.

## Instrument verification

A single authenticated GET to the Trading 212 demo instruments endpoint returned
all five as USD-denominated `STOCK` instruments at 2026-11-01 02:00:50 UTC. The
selection record retains their broker IDs and ISINs. This verifies metadata
availability, not fresh quotes, S&P membership, GICS classification, fractional
orders or protective-order capability. The account is in EUR, so current USD/EUR
conversion and cost evidence remain necessary. The endpoint and fields are
documented in the [official broker API](https://docs.trading212.com/api/instruments/instruments).

ExxonMobil's returned ISIN contains CUSIP `30233Q108`, consistent with its
[current issuer FAQ](https://investor.exxonmobil.com/company-information/faq).
This avoids carrying forward the old security identity after its 2026
reorganization.

## Chosen data and service stack

- **Trading 212 demo:** account and instrument identity through read-only requests.
- **Twelve Data:** historical bars for offline research. The existing key returned
  two MSFT daily OHLCV records successfully. These records are not an approved
  training dataset and cannot establish intraday fills, costs or executable spread.
  Its [coverage documentation](https://support.twelvedata.com/en/articles/9935903-us-equities-market-data)
  distinguishes the default real-time venue coverage from consolidated historical
  coverage. Intended-use and retention rights still need verification.
- **Financial Modeling Prep:** current and historical S&P 500 constituent data,
  normalized and reviewed through Trady's input service. Its
  [constituent endpoint](https://site.financialmodelingprep.com/developer/docs/stable/sp-500)
  supplies company, sector and subsector fields. The existing key received
  **HTTP 402** on the current-constituents request. Access, taxonomy mapping and
  storage/processing rights must be resolved before enabling this source.
- **Massive:** consolidated U.S. bid/ask data, because the runtime checks spreads
  and quote freshness. Its [last NBBO endpoint](https://massive.com/docs/rest/stocks/trades-quotes/last-quote)
  provides bid/ask observations. The existing key received **HTTP 403** for MSFT.
  Massive distinguishes free end-of-day data from real-time plans on its
  [stock-data page](https://massive.com/stocks). A working key does not itself prove
  the required entitlement, rights or broker execution capability.
- **SEC EDGAR and issuer investor relations:** primary research, supplemented by
  genuinely independent reporting under the existing source-comparison rules.
  Actual source imports and reviewer identities are still required.
- **Existing Rust input service:** loopback deployment behind a trusted HTTPS
  proxy, with separate import/review authority and read-only runtime access. A
  working source registry, calendar, account bindings, approved formula and model
  are needed before creating a deployable service configuration. A raw vendor
  response cannot be used directly as Trady's universe or research packet.

No new subscription was purchased, and automatic paid fallback remains disabled.
The provider choices are made; the HTTP rejections are concrete access blockers.

## Model, formula and skill choices

Use the repository's pinned native Perpetual binary classifier for scenario
advice and its separate four-outcome `model-lab` baseline for complete outcome
probabilities and conditional net payoffs. These are distinct artifacts. Genuine
point-in-time training data, reconciled costs/outcomes, sealed holdout evaluation,
independent approval and prospective shadow evidence must be produced; the
repository does not contain approved fitted artifacts to select today.

Use the existing bounded native TREND family with an ordinary 1% net target as
the initial research scenario. No formula authority or policy change was installed.
The parameter/envelope candidate still needs compatible prediction bindings,
proof, parity, evaluation and prospective shadow evidence before activation.

The native `skill-manifest` command validated bundle 2.1.0 and generated
[a candidate manifest](../config/skills.demo.candidate.json) for all 52 files.
Each file's SHA-256 was checked against the generated manifest. Full content
review remains necessary before deploying the hashes as approved runtime skills;
hash generation alone does not establish approval.

## Verification and current state

At 2026-11-01 02:04:58 UTC, the revised configuration passed parsing and retained
`order_submission_policy=record_only`. The allowlist blocker is resolved; the
remaining five checks are model, input service, universe, approved skills and
formula authority. A bounded run returned `shadow_blocked`, **zero market cycles
and zero order mutations**.

The run had an empty inherited environment and an absent credential-store path.
Its configuration failure occurred before credential loading or construction of
network clients. Separate provider probes were GET requests only. No entry-order
API calls were made; network traffic was not independently captured.

The existing missing-prerequisite regression test now explicitly empties its own
allowlist fixture, so real owner selections cannot silently weaken that test. The
test passed. The five configured IDs match the selection record, and all 52
candidate skill hashes match current file contents.

Config SHA-256: `14b95284d6ad8f73192b4a8023d8894f9ce42e415624695a8094325353f415cb`.

The immutable audit was verified against its filename SHA-256:
`/tmp/trady-demo-no-submit-20261101/shadow-journal/cycle_bd0fd13a5f2aa432093728b0df9c7e9250d66b7a77f128901aca10db9d6dbd18.json`.

The live-data demo is not operational yet. Next comes provider access and reviewed
data ingestion, followed by real model/formula validation and deployment review.
Instrument choices and provider selection no longer need to be supplied by the owner.
