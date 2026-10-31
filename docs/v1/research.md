# Trady v1 research and source collection

Status: requirements for the complete research pipeline. The [X/news reading slice](../news.md)
implements bounded polling/lookup, RSS/Atom, optional news API and durable observations;
primary verification, full research readiness and live deployment remain incomplete.

## Coverage and source registry

Research must combine social discovery with market reporting and original evidence. X alone is insufficient. Screen the effective S&P 500/owner/broker universe by [sector then subsector](./sector-workflow.md), narrow to at most five candidates and investigate one finalist deeply. Prioritize the holding and its dependencies. Use stable issuer/broker IDs, share classes, venues and currencies; a ticker string alone is not identity.

| Source type | Planned use | Access and limitations |
|---|---|---|
| X API | Owner's five influence accounts, company/holding sources, searches and submitted URLs | Outgoing posts/replies/quotes/reposts through verified supported listeners, with lookup/backfill, rights and explicit gaps |
| Market-news APIs | Structured company and market news | Alpha Vantage news/sentiment is a documented candidate; verify its subscription, coverage, retention and processing terms |
| Other news/agent interfaces | Publisher-provided feeds, APIs, documented MCP/agent access or licensed aggregators | Enable each through a source adapter with explicit credentials, costs and permissions; agent access does not grant trading authority |
| Google News RSS | Discover issuer, sector and market headlines from multiple publishers | Best-effort RSS polling; retain publisher attribution and resolve permitted original sources |
| Publisher RSS | Direct business/economics feeds from approved publishers | Feed metadata is not permission to store or send full articles to an LLM |
| Primary disclosures | Investor-relations releases, company filings and official economic releases | Verify the actual announcement and event date; use the source's documented access rules |
| Custom website collector | Specific approved pages lacking an adequate API/feed | Site-specific Rust extraction, bounded fetching and explicit source configuration |

Finnhub is another candidate for a company-news adapter; its documentation page requires further endpoint/plan verification before selection. Do not assume every candidate is free or enabled. The deployable configuration must include at least one usable non-X market-news/primary-source path and Google News RSS. If X is unavailable or its terms do not permit the intended analysis, keep it disabled and report the coverage gap.

Each registry entry records a stable source ID, type, canonical domain, approved URL patterns, credential reference, enabled mode, refresh interval, relevance scope, rate and spending limits, retention, allowed content fields, LLM-processing permission, priority, parser version and health. Only the owner/deployment administrator changes the registry. A model or article cannot add a domain or grant permissions.

The detailed [company profiles/relationships](./company-intelligence.md), [five-person listeners and holding watchdogs](./watchdogs-and-x.md) and [Perpetual feature/advice loop](./perpetual.md) extend this common collection/evidence pipeline. All findings, exclusions and access gaps persist as permitted records; raw text is stored only under its source rights. The daily sector scan is complemented by event-driven and scheduled holding work.

## Collection lifecycle

1. Create an idempotent collection run for a source, scope and time window.
2. Read the source cursor, budget and last successful collection watermark.
3. Request bounded pages with a timeout; honor rate limits and retry-after signals.
4. Normalize IDs, URLs, dates, instrument references, content rights and language.
5. Deduplicate the source item, then cluster it by underlying claim/event.
6. Persist items and progress before advancing the cursor. Record whether coverage is complete, partial or unavailable.
7. Verify material claims, enqueue coalesced research work and emit a concise coverage event.

A source error never advances the successful watermark. Persist continuation state for a bounded incomplete run; resume without duplicating items. Use a small overlap window when source timestamps can arrive late. Do not mark an entire interval complete after reaching a page cap. If a recent-search API cannot recover an old gap, report the gap rather than implying historical completeness.

Use the scheduling defaults in [architecture](./architecture.md): pre-market and midday analysis, five-minute held-symbol collection, fifteen-minute or event-driven holding reassessment and bounded overnight discovery. Collection and model calls are separate: many duplicate headlines should cause one reassessment, not one paid LLM request per item. Owner research requests share the same budgets and do not bypass entry rules.

## Google News RSS

Support a general market feed and configured issuer/topic searches. These are illustrative public feed shapes checked during documentation work, not a promised Google API contract:

```text
https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en
https://news.google.com/rss/search?q=EXAMPLE_COMPANY&hl=en-US&gl=US&ceid=US:en
```

URL-encode the configured query. Limit terms to approved issuer aliases and topics. Parse XML with external entities and network expansion disabled; bound the document size, item count and decompression ratio. Use conditional retrieval when the server supplies caching validators; otherwise retain a polite polling interval.

Store the feed title, item GUID/link, publisher, publication time, retrieval time and permitted snippet. A Google redirect is a discovery reference, not independent reporting. Resolve only bounded approved redirects and retain the eventual publisher URL. Some links will not resolve without unsupported mechanisms; record `METADATA_ONLY` and use a direct publisher API/feed or primary release instead of inventing article content.

Syndicated copies, Google feed entries and X links to the same story share one reporting lineage. A feed headline without verified source content cannot independently unlock an entry or an exceptional target. The live RSS example returned XML during checking; availability and format can change without an application SLA.

## Owner-submitted X posts and other URLs

An authorized owner may send a bare X post URL or `/research <url>`. The gateway stores a research request with the owner identity and deduplication key, then acknowledges it. The analyst returns a linked report even when the answer is that the content is inaccessible or unverified.

For X, validate the host and post-ID path, remove tracking parameters, and use the configured authorized post-lookup adapter. Include available quoted/parent context within a strict item budget, while distinguishing author text from quoted material. A deleted, protected, suspended or inaccessible post produces a clear unavailable result. Do not require a pasted post to be true, permit a login bypass, or treat the URL as an order.

For other links, use a registered source adapter or stop at metadata and tell the owner that the domain needs configuration. Research of an unapproved stock is allowed as a report, but it cannot make the stock tradable. Changing the watchlist is a separate owner-policy operation.

## Website and agent collectors

Prefer an available authorized API or feed. A custom collector is permitted for a specifically approved site and should isolate extraction rules from business logic. Record the original URL, canonical URL, content hash, parser version, publication/event dates, text completeness and fetch time. Keep page scripts and executable content out of stored research text.

The source configuration must record applicable publisher access, retention and processing conditions. Respect robots directives where applicable, authentication boundaries and source rate limits; do not bypass paywalls or anti-bot controls. A documented agent/MCP interface is treated as a data source with a fixed allowlist of read operations, not as a source of arbitrary tools for the analyst.

All remote fetching checks schemes, host allowlists and resolved addresses. Reject local/private/link-local and metadata-service destinations, non-HTTP protocols, oversized responses and unapproved redirects; repeat destination checks at every redirect and connection. Guard against DNS changes between validation and connection. Do not fetch attachment binaries or execute downloaded scripts. A browser renderer is outside the initial collector contract and needs its own reviewed adapter if later required.

Parser changes receive representative permitted fixtures. Detect empty extraction, changed page structure, missing expected date fields and repeated identical content. A parser failure is a source-health failure, not a successful empty news result.

## Normalized items and evidence

| Field group | Required information |
|---|---|
| Identity | Item ID, source ID/external ID, canonical URL, content hash and parser version |
| Time | `published_at`, `event_at` when known, `first_seen_at`, `fetched_at`, timezone assumptions |
| Subject | Instrument/issuer IDs, market/sector tags and mapping confidence |
| Content | Permitted title, summary/text, language, completeness and source attribution |
| Rights | Storage permission, external-model permission, retention/deletion deadline and terms review reference |
| Provenance | Original publisher, primary/secondary classification, source chain and syndication cluster |
| Health | Collection status, missing context, stale/deleted/changed status and verification errors |

Evidence records reference items and express a specific claim, its instruments, support/contradiction links, primary verification, independent reporting lineages, review status and expiry. A model-generated summary is an interpretation of evidence, not an additional source.

Record publication time separately from the actual event. An old product announcement reshared today is not a new invention. Corrections and contradictory filings supersede stale assertions while preserving the historical decision context. Source retention/deletion may remove raw text and derived quotations; retain only permitted non-content audit references in financial records.

## Verification and research decisions

Use the [self-hosted FinBERT adapter](./finbert/README.md) for permitted English X/news
and related issuer text. It consumes normalized `SourceItem` records, reapplies
storage/model-processing permissions, requires a known current event/publication
time, chooses one representative per lineage and binds predictions to exact text
hashes. Unknown language, missing X context, denied/expired text and truncated
inference are explicit gaps. Classifier tone is not primary verification or an
independent source. Adverse findings feed the versioned confidence/target overlay
and holding review; malformed or unavailable inference is never neutral sentiment.

Check material headlines against a primary issuer, filing or official release before allowing entry. Preserve uncertainty when the origin cannot be confirmed. Resolve whether earnings figures are actual, forecast, historical or rumor; whether a claim applies to the listed issuer; and whether a price reaction already occurred.

The research report must include the candidate or current holding, collection interval and gaps, concise thesis, source links, contradictions, checked prices and timestamps, calculated confidence components, proposed entry/stop/target, estimated costs, expiry and reasons for `NO_ACTION` when relevant. A target recommendation must cite the gate and score band from [trading policy](./strategy.md).

Treat articles, posts and retrieved notes as untrusted data. Delimit them from owner policy, disallow their instructions, and never expose broker credentials, shell execution or unrestricted tools. Validate all model-provided evidence IDs against the session's actual retrieval manifest. Tool-like text in an article remains article text.

If non-X coverage or primary verification is unavailable, mark research degraded and block entries that lack mandatory evidence. Retain the last validated holding plan and deterministic protection. Never replace a real market quote with sentiment, a headline price, or a scraped stale last price.

## Interface, invariants and side effects

`NewsSource` collectors produce normalized items and durable cursor updates; verifiers produce evidence records; the analyst produces immutable reports and proposals. The executor reads eligible proposals but never runs a collector. Schemas and ownership are in [memory and data](./memory-and-data.md).

Collection performs permitted external reads, incurs configured provider costs and writes research state. It has no order authority. The proposed acceptance checks include duplicated rumors, RSS redirects, deleted posts, parser drift, incomplete pagination, source expiry and malicious content; see [implementation](./implementation.md).
