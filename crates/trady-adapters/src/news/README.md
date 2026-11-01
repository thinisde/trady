# X and news source adapters

## Overview

Read-only adapters for reviewed registry entries. The [implementation guide](../../../../docs/news.md)
describes configuration and commands. These adapters return source observations,
not verified claims or trading decisions.
The shared HTTP port also supports bounded JSON POSTs for automatic analysis
inputs; the news collectors use GET requests.

## Interface

- `[financial_news.rs:fetch]` normalizes Finnhub company news and GDELT article
  discovery into the existing `SourcePage`/`SourceItem` contract. It preserves
  uncertain GDELT publication and incomplete saturated windows.
- `[http.rs:HttpTransport::get_with_headers]` and `SafeHttp` support bounded
  financial authentication/User-Agent headers with redirects disabled.
  `[http.rs:SafeHttp::get_with_headers]` permits only `User-Agent`,
  `x-cg-demo-api-key` and `Authorization`; `[http.rs:SafeHttp::send]` limits each
  value to 4,096 bytes without control characters. The trait default rejects
  nonempty headers. `[x_meter.rs:MeteredHttp]` retains that default and unsupported POST
  behavior rather than forwarding those methods to its inner transport.

- `[x_meter.rs:MeteredHttp]` decorates `HttpTransport` with a collector-owned
  `XCostRepository` and optional `XBudgetPolicy`. `new` does not perform I/O;
  `set_x_scope`, `get` and `x_budget_enforced` implement the transport port.
- `[mod.rs:SourceClient::recent_x_counts]` and `XCountsSource::counts` return
  domain-owned `XCountsMeasurement`; `[x.rs:recent_search]` and `counts` implement
  bounded recent search and daily Recent Counts behind the existing HTTPS port.

- `[mod.rs:SourceClient]`: `fetch` through `NewsSource`, `resolve_x`, `lookup_x`,
  default production transport and `with_transport(transport, credential)` for
  an injected transport and named-credential callback.
- `[mod.rs:parse_feed]`, `canonical_url`, `x_post_id`, `digest`, `PARSER_VERSION`,
  `MAX_BODY_BYTES`: offline RSS/Atom parsing, reference normalization and limits.
  The parser revision is `trady-news-2`; the shared body limit is 2 MiB.
- `[http.rs:HttpTransport]`, `HttpRequest`, `HttpResponse`, `SafeHttp`,
  `public_address`, `validate_destination`: bounded HTTPS transport and destination checks.
  `HttpResponse::final_url` supplies the checked redirect destination as the feed base.
- `[http.rs:HttpTransport::get]` and `post_json(request, body: Vec<u8>)` are the
  transport methods. The default `post_json` returns `analysis_post_unsupported`;
  `SafeHttp` implements both.
  `x_budget_enforced` defaults false and `set_x_scope` defaults to no action.
  Production X reads require a metered transport; `SourceClient::default` alone
  does not authorize them. Explicit test transports supply their own guarantees.
- `[http.rs:HttpRequest]` exposes `url`, `allowed_hosts`, `bearer`, `etag`,
  `last_modified` and `redirects`. `[http.rs:HttpResponse]` exposes `status`,
  `final_url`, `body`, `etag`, `last_modified`, `retry_after` and `rate_reset`.
- `[x.rs:XUser]`: public `id`, `username` and `name` strings, reexported from the
  private `x` module; resolved identity grants no automatic ownership approval.
- Internal `pub(super)` functions `[x.rs:timeline]`, `lookup`, `[rss.rs:fetch]`,
  `parse` and `[alpha.rs:fetch]` return `SourcePage`; `[x.rs:resolve]` returns
  `XUser`. The application/domain crates own the imported research contracts.

## Invariants

- `[x_meter.rs:MeteredHttp::get]` reserves conservative resources before each X
  HTTP call, records returned post/user/count resources and retains unknown
  consumption on failures. It rejects absent/inactive policies, unsupported
  endpoints and exhausted budgets; overruns are recorded and delivery fails.
  Callers sharing a billing scope must configure the same durable ledger through
  their injected repositories.
- `[x.rs:recent_search]` rechecks stable identity, rejects other actors and posts
  outside the requested interval, sends one versioned query and expands authors
  only. It reports selected/polling coverage and missing context instead of
  claiming a full account timeline. Optional repost exclusion requires verifiable
  reference metadata. `[x.rs:original_references]` defaults an omitted reference
  field to an empty list, classified `Original`; explicit `null` remains
  `SocialAction::Unknown`, adds metadata/context gaps and fails enabled repost exclusion.
- `[x.rs:search_window]` restricts search/counts to supported seven-day history,
  page sizes 10–100, second-aligned timestamps and an end at least ten seconds old.
  `[x.rs:counts]` validates daily bucket totals/continuity, preserves pagination,
  accepts legacy count aliases and marks gapped/partial counts unknown.
  Daily granularity is requested; bucket length/alignment is not enforced.
  Incomplete measurements retain numeric counts with `complete: false`;
  missing data is an error.

- `[http.rs:SafeHttp::send]` shares GET/POST destination and response guards:
  approved HTTPS, pinned public DNS, disabled proxies/compression/automatic retries,
  and bounded time, redirects and body size.
  DNS waiting is five seconds. Request/connection timeouts are 30/15 seconds for
  `api.gdeltproject.org` and 15/5 seconds for other hosts;
  the body limit is 2 MiB and at most three redirects are followed. Bearer requests
  never follow redirects; Alpha's query-key request sets `redirects: 0` explicitly.
  Failures do not expose response bodies. A DNS wait timeout does not cancel its resolver thread.
- `[http.rs:SafeHttp::post_json]` rejects bodies above 2 MiB with
  `analysis_request_too_large`, forces zero redirects and sets
  `Content-Type: application/json`. It sends the supplied bytes without parsing
  their JSON syntax; callers construct and validate their request contracts.
- `[x.rs:timeline]` checks current handle/stable-ID agreement and accepts only the
  monitored actor's outgoing posts. `[x.rs:normalize]` preserves reference/edit/context
  attribution, marks missing context across every reference, supports legacy response
  aliases and does not infer instrument identity. Author/reference/edit IDs are
  requested as `expansions=author_id,referenced_tweets.id,edit_history_tweet_ids`,
  separately from `tweet.fields`. These are the emitted parameters; source inspection
  does not establish provider acceptance. `[x.rs:timeline]` does not locally
  reject returned timestamps outside its requested interval; recent search does.
- `[x.rs:normalize]` bounds references to three and supplied edit-history IDs to
  six. Edit history must contain the returned post ID; a later final edit marks it
  `Superseded`. `[x.rs:timeline]` always reports polling/history/deletion/media
  gaps; `complete` means no provider errors and can coexist with a continuation
  or empty timeline. `[x.rs:lookup]` returns one post and reports partial context
  separately; neither flag proves complete account history.
- `[rss.rs:parse]` rejects DTDs, XML-reader errors and invalid entity references,
  and bounds depth, fields and items. Self-closing elements receive no root-position
  validation. Atom text constructs extract plain text from nested
  XHTML; RSS/Atom item titles and summaries are reduced to plain text. Language
  tags are normalized with scoped `xml:lang` inheritance and RSS channel-language
  fallback; an empty `xml:lang` clears inheritance. Item language follows summary
  scope when summary content exists, otherwise title scope, before entry/channel
  fallback; explicitly cleared or mixed scope remains absent. Relative links resolve against inherited
  `xml:base` and the fetched feed URL. Feed titles survive element reordering.
  Google results remain metadata-only; RSS supplies snapshot coverage, not complete history.
  `[rss.rs:fetch]` forwards cache validators and rejects unsolicited 304s;
  it does not apply request start/end filtering or consume continuation tokens.
- `[alpha.rs:fetch]` treats quota notices as failures and carries capped time-window
  subdivisions as continuation work. Saturated unsplittable minutes retain pending
  siblings and an explicit gap; multiple such windows can rotate through cursors
  without draining. `complete: true` with continuation does not promise full history.
  Publication times returned by Alpha are not locally checked against the
  requested window or caller time.
- `[financial_news.rs:fetch]` rejects continuation tokens and processes only the
  first requested-limit rows. Finnhub rejects future publication times and skips
  publications outside the inclusive collection window; GDELT rejects future or
  out-of-window discovery times without treating them as publication times.
  Saturation is determined from the raw row count before filtering, so a filtered
  page can remain incomplete. Publisher article URLs are provenance only and are
  not fetched. A GDELT payload without an article array fails with
  `gdelt_articles_missing` and a 60-second retry timestamp. An explicit empty
  article array remains a valid empty page; malformed responses do not assert
  complete empty coverage.
- `[mod.rs:canonical_url]` strips tracking parameters/fragments and rejects local
  literal destinations, but performs no DNS or fetch authorization. Network entry
  points require enabled valid sources; `fetch` and `recent_x_counts` also
  check activation gaps.
  `[mod.rs:parse_feed]` is an offline parser restricted to RSS kinds and limits
  `1..=100`; it does not require source enablement or check activation gaps.
- `[mod.rs:item]` hashes normalized content including language, preserves lineage/rights and redacts denied
  storage. Source text and provider errors cannot modify configuration or order state.

## Side Effects

`[x_meter.rs:MeteredHttp::get]` invokes its durable cost repository before and
after X reads; a network request can remain conservatively charged after failure.
`[x.rs:recent_search]` and `counts` make paid read-only calls only through that
transport and still return unverified source data. HTTP 402 is reported as
`provider_credits_exhausted` by `[mod.rs:SourceClient::get]`.
`[x.rs:timeline]`, `recent_search` and `counts` each resolve identity before their
data request, adding a support read. `[x_meter.rs:MeteredHttp::get]` forwards
non-`api.x.com` requests to the inner transport without X reservations; X ledger
reservation/settlement is separate from collected-page persistence.

`[http.rs:SafeHttp::get]` resolves DNS and opens HTTPS connections; provider calls
can consume allowance. `[mod.rs:SourceClient::default]` installs the central
credential-name loader; authenticated fetches invoke it. `[alpha.rs:fetch]` sends
its API key in the query string; `[financial_news.rs:fetch]` likewise sends
Finnhub's key in `token`, while GDELT discovery is unauthenticated. X uses bearer
authentication. Observation times are supplied by callers;
`[x_meter.rs:MeteredHttp::get]` uses actual UTC time for policy/reservation checks.
`[mod.rs:parse_feed]` performs no external calls; injected transports and credential
callbacks determine their own effects. Persistence belongs to the application repository.
`[http.rs:SafeHttp::post_json]` sends caller-supplied analysis bytes to the approved
destination, with optional bearer forwarding; it does not change news collection to POST.

## Verification

Run `cargo test --workspace --locked` from the repository root.

Run `cargo test -p trady-adapters news:: --locked` from the repository root.
