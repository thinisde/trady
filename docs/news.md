# X and news reading implementation

This slice implements read-only X timeline polling and post lookup, Google News
RSS searches, publisher RSS/Atom and optional Alpha Vantage news. It stores bounded
observations, cursors, collection events and correction invalidations in PostgreSQL.
The [five-account pilot](./v1/x-api-monitoring.md) adds a separate disabled
recent-search configuration, shared resource-cost ledger, preflight counts and
local evidence export, described below.
The [research specification](./v1/research.md) and [X requirements](./v1/watchdogs-and-x.md)
remain authoritative for the complete research system.

## Commands and configuration

Finnhub company news and GDELT discovery are implemented as disabled sources in
the existing registry. They produce ordinary `SourceItem` records with the same
rights, lineage, corrections and verified-packet/FinBERT boundaries. GDELT's seen
time does not become publication time; saturated windows remain incomplete.
Finnhub uses the central `TRADY_FINNHUB_API_KEY`; GDELT needs no key. Alpha Vantage,
Finnhub and GDELT share migration 9's account credit ledger across sources.
See [financial data](./financial-data.md) for synchronized account limits and reviews.

```sh
cargo run --locked --bin trady -- news status
cargo run --locked --bin trady -- news parse-feed crates/trady-cli/tests/fixtures/news.rss --source google_market
cargo run --locked --bin trady -- migrate
cargo run --locked --bin trady -- news collect --once --config config/research.json
cargo run --locked --bin trady -- news read --limit 20 --config config/research.json
cargo run --locked --bin trady -- news read --for-model --limit 20 --config config/research.json
```

`status` and `parse-feed` work without a database or credentials. `parse-feed`
does not store its result. `migrate` uses `TRADY_MIGRATOR_DSN`; collection, lookup,
identity resolution and database reading use the restricted `TRADY_ANALYST_DSN`.
Database transport still requires an explicit loopback/Unix development connection.

The [registry](../config/research.json) ships with every source disabled, content
storage/model processing denied, and the five X user IDs unset. It preserves Trump,
Musk, Gill, Ackman and Icahn in the owner's order and scoped priorities. No candidate
identity or provider entitlement has been activated or claimed as verified.

An operator enables a source only after reviewing its access, billing, refresh
interval and content rights. Set `rights.terms_reference` to that review, choose
`store_content`, `llm_processing` and `retention_hours`, and set a conservative
`daily_request_limit`. Host allowlists are exact; RSS redirect destinations require
their own allowed host. Google `query` values are encoded as one query parameter.
Alpha Vantage's optional `query` is its documented ticker filter, not a company-ID
resolution or trading-universe permission.

Credentials resolve through the [central store](./credentials.md) or environment
under the name in `credential_env`, normally
`TRADY_X_BEARER_TOKEN` or `TRADY_ALPHA_VANTAGE_API_KEY`. No command accepts tokens
as arguments, writes them to the registry or returns provider error bodies.

Enable/review `x_lookup` before resolving candidates:

```sh
cargo run --locked --bin trady -- news resolve-x realDonaldTrump --source x_lookup --config config/research.json
cargo run --locked --bin trady -- news x https://x.com/i/status/123 --source x_lookup --config config/research.json
cargo run --locked --bin trady -- news collect --once --source x_trump --config config/research.json
```

The post URL above is illustrative, not an actual verified post. Identity lookup
returns the current ID/username without approving ownership or editing the registry.
Before enabling a timeline, verify original ownership cross-links, role and recent
activity; set its `user_id`, `ownership_reference` and UTC `verified_at`. Reviews
expire after seven days. Each timeline page checks that current handle resolution
still matches the approved stable ID; a mismatch blocks that fetch.

## X pilot and resource budget

The [pilot example](../config/x-pilot.example.json) contains only
Ackman/Musk/Bessent/Burry/Altman. It does not replace or combine with the owner's
original registry. Sources, billing, content storage and model processing ship
disabled; identities and approval references are unset. Use one reviewed registry
for overlapping author coverage. Configuration rejects duplicate primary handles
or stable IDs within that registry.

Inspect the proposal offline with `news status --config config/x-pilot.example.json`.
After migration and explicit operator review, these bounded commands are available:

```sh
cargo run --locked --bin trady -- news counts-x --source x_pilot_ackman --config config/x-pilot.example.json
cargo run --locked --bin trady -- news collect --once --source x_pilot_ackman --config config/x-pilot.example.json --force
cargo run --locked --bin trady -- news budget-x --config config/x-pilot.example.json
cargo run --locked --bin trady -- news reconcile-x /path/to/provider-usage.json --config config/x-pilot.example.json
cargo run --locked --bin trady -- news export-x --as-of 2026-11-02T12:00:00Z --config config/x-pilot.example.json
```

Do not enable the example unchanged. Verify current pricing/entitlement and the
provider spending limit, set the actual UTC billing-cycle boundaries, and record
the owner's watchlist/coverage decision in `x_budget.approval_reference` and each
rule's `approval_reference`/`approved_at`. Review rights and stable identities as
above. `entitlement_reference` and `provider_limit_reference` record deployment
reviews; software does not set the provider's limit or auto-recharge policy.
Both source and budget require explicit enablement. Existing timeline, lookup
and handle-resolution commands also require a reviewed `x_budget`; the original
shipped registry has none and therefore authorizes no paid X reads.

Amounts use integer USD microdollars: `10000000` is $10, split into at most $8
ordinary posts and $2 support resources. Rates are versioned configuration.
`MeteredHttp` reserves each request's conservative post, user and Recent Counts
resource bounds before HTTP. Identity, context/edit/user resources, retries and
repeated results count. Successful responses record observed resources; failures,
malformed responses and crashes retain the upper bound. A page-persistence failure
cannot refund network cost. Resource overruns block reads pending reconciliation.

PostgreSQL serializes reservations for a shared `billing_scope`; use the same
scope for all consumers of one provider billing account. Policy/rate changes in
a used cycle and overlapping cycle resets are rejected. Query changes do not
reset spending. `budget-x` reports resources, estimated charges and uncertain
requests per source/rule. A provider usage file has this shape:

```json
{
  "provider_total_microusd": 125000,
  "as_of": "2026-11-02T12:00:00Z",
  "evidence_reference": "reviewed Developer Console usage report"
}
```

Reconciliation records a monotonic provider total/timestamp. Effective debit is
the greater of the local ledger and provider usage plus later, in-flight or
uncertain reads. Unknown charges remain conservative; this cannot promise an
exact invoice. Exhaustion records a durable gap and stops optional X reads,
independently of quote monitoring, reconciliation and protected broker exits.

`XRule` builds one `from:handle` query with literal OR terms, quoted phrases and
an optional explicitly approved `-is:retweet`. Replies remain eligible. A changed
approved rule requires a higher version and starts a coverage revision with fresh
counts. Every page re-resolves the stable identity. Search expands authors only;
it does not routinely fetch referenced post/edit bodies or media. Missing action
metadata stays `UNKNOWN`; missing context/edit lineage remains a recorded gap.

Preflight uses daily buckets over a recorded interval within the prior seven
days, with bounded resumable pagination. Missing, gapped, repeated or partial
counts remain unknown. Pending pages can resume before normal cadence, preserving
provider backoff and request/resource limits. Collection requires complete counts
for the current rule, at most seven days old, whose projected cycle post cost with
burst margin fits the ordinary allocation. This projection is per rule: sum all
rules and separately assess support costs before enabling the pilot. Counts are
planning evidence, not a volume forecast or a completeness guarantee.

Search requires page sizes 10–100 and UTC second-aligned intervals ending at
least ten seconds before request time (the collector uses thirty seconds).
Observation and counts pagination retain independent original intervals.
Unsupported history outside seven days stays a gap; the collector does not skip
to newer history or start another transport. Daily search is a pilot trade-off.

`export-x` makes zero X requests. It selects current versions ingested/fetched by
`--as-of`, intersects current source rights, and excludes expired, storage/model-denied,
unavailable and superseded observations. PostgreSQL records ingestion and the
original registry/coverage revision. Availability is recorded after the content
transaction commits; a crash before this marker leaves unknown availability and
excludes the item. Content changes clear the marker until recorded again. These
exports select current versions, and cannot reconstruct a historical dataset.
Later edits cannot appear in earlier exports.
Verification and feature-ready times remain unknown, and `training_permitted` is
false. Inference permission does not authorize model fitting. Trusted offline
evaluation still must resolve eligible issuers, deduplicate lineage, preserve
no-action samples and establish feature availability before held-out comparisons.
No profitability evidence, semantic clustering, promotion, scheduler, stream or
webhook is installed.

## Collection and reading contracts

- `collect --once` performs bounded work. Use an operator scheduler for recurring
  invocation. The source cadence is enforced; `--force` skips cadence only, retaining
  provider retry deadlines and durable daily request caps.
- Request reservations commit before network access and remain spent after a crash.
  Reserve two requests per X timeline/search/counts page, one per lookup/resolve/API page and four
  per RSS fetch (including three possible redirects). These are conservative request
  caps, not a monetary estimate or proof of current provider entitlements.
- Source windows and continuation tokens survive bounded runs. Every page commits
  items, source revision, cursor, status and collection audit atomically. Concurrent
  writers must reread the durable revision. Failed fetches never advance the successful
  watermark. Page caps and repeated tokens remain partial.
- Alpha Vantage caps subdivide the original interval into persisted, minute-aligned
  work. Saturated minutes preserve pending sibling windows and remain explicitly
  incomplete. RSS reports
  snapshot coverage; it cannot promise to recover articles already removed from a feed.
- Items retain source/external IDs, canonical URLs, content hashes, parser version,
  feed title/publisher, publication/first-seen/fetch times, unknown event time, rights, expiry,
  lineage keys and gaps. Known shared URLs/X originals share a lineage; automatic
  semantic claim clustering and independent-source verification remain future work.
- X outgoing originals, replies, quotes and reposts retain the actor, referenced
  IDs/authors, conversation/edit history and bounded expanded text. Incoming actors
  cannot be mislabeled as the monitored person's speech. Memes/media are not interpreted
  into tickers or intent.
- Stable source/external IDs deduplicate replay. Changed content retains a revision
  and records an invalidation. New X edit IDs supersede known earlier versions and
  remove their text/context. Reading and collection purge expired/revoked content
  from current and historical records; shorter retention applies to existing records
  and replay cannot extend an item's retention deadline. X edit-chain locks serialize
  concurrent timeline/lookup ingestion across source IDs.
- Explicit unavailable/private/access-denied X lookups invalidate known stored copies
  and remove their expanded quotations from other current/history observations.
  Transport failures and rate limits do not classify a post as deleted. Lookup output
  is reread from storage so a fresh fetch cannot restore expired or superseded text.
- `read --for-model` redacts feed title, title, summary, post and context text whenever external
  model processing is denied. Ordinary reads also redact expired, unavailable,
  superseded or storage-denied content. Results identify source data as untrusted.
- Unresolved Google links remain `METADATA_ONLY`; no full article is fetched or
  invented. Publisher/API snippets remain unverified source observations. Collection
  does not produce a confidence score, approve a watchlist or authorize orders.

## Network and parser boundaries

The HTTPS client disables ambient proxies and automatic redirects, resolves and
pins public addresses for each connection, and repeats host/address checks for
every manually followed RSS redirect. Authenticated APIs follow no redirects.
Private, loopback, link-local, metadata-service and unsupported destinations are
rejected. API errors expose stable reasons instead of request URLs or response bodies.

Fetches use a five-second DNS/connect limit and fifteen-second HTTP timeout per
hop. Responses are capped at 2 MiB. Compressed bodies are rejected, making the
accepted decompression ratio one. RSS/Atom reject DTDs/external or unknown entities,
malformed XML, excessive depth/fields, invalid dates and unexpected/empty extraction.
Atom XHTML text and inherited `xml:base` references are supported; relative links
use the checked final feed destination as their base.
There is no browser, script execution, attachment download or arbitrary website collector.

## Verification and remaining scope

```sh
cargo test -p trady-adapters news:: --locked
cargo test -p trady-application --test research --locked
cargo test -p trady-application --test x_monitoring --locked
cargo test -p trady-cli --test news --locked
python3 scripts/test-postgres.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
```

The news tests cover outgoing X actions/context, mixed missing references, identity mismatch, edit aliases,
partial responses, unavailable lookup, retry-after, RSS/Atom/conditional fetches,
XHTML/base resolution, XML/URL/address bounds, metadata/model rights, capped restart/resume, request budgets
and CLI behavior. The isolated database scenario additionally verifies migration
replay, cursor durability, correction revisions, invalidations, shortened retention,
expired refetches, unavailable quotation removal, concurrent X edits and role
boundaries. The shared runner uses a fresh disposable PostgreSQL server.
Monitoring checks additionally cover query scope, unknown action metadata,
counts restart, cost-before-HTTP ordering, uncertain consumption, simultaneous
budget reservations, policy/cycle reset rejection and provider reconciliation.

This implements parts of SRC-01/02/03/04/06/07 and X-01/02/03/05. It does not complete
phase 3: live entitlement/identity verification, streams/webhooks/deletion delivery,
Truth Social, original-article resolution, primary/claim verification, semantic
syndication clustering, company/profile dependency invalidation, model-facing tool
registration and holding watchdog scheduling remain separate work. Source
invalidations are durable records for future consumers; they do not currently purge
nonexistent profile/feature/memory pipelines. All sources remain off by default.

The existing research modules were checked against current [X timeline documentation](https://docs.x.com/x-api/users/get-posts),
[X post lookup](https://docs.x.com/x-api/posts/lookup/introduction),
[Alpha Vantage news](https://www.alphavantage.co/documentation/#news-sentiment),
[reqwest 0.12](https://docs.rs/reqwest/0.12.28/reqwest/blocking/struct.ClientBuilder.html)
and [quick-xml 0.38.4](https://docs.rs/quick-xml/0.38.4/quick_xml/reader/struct.Reader.html).
Current X requests use `post.fields` with author/reference/edit IDs in `expansions`;
response parsing also accepts legacy tweet-named aliases. Documentation availability
does not establish this deployment's access or content-processing rights.
The pilot was checked against official
[recent search](https://docs.x.com/x-api/posts/search-recent-posts),
[Recent Counts](https://docs.x.com/x-api/posts/get-count-of-recent-posts) and
[resource pricing](https://docs.x.com/x-api/getting-started/pricing) on 2026-10-31.
Search expands authors only; timeline/lookup retain bounded context expansion.
