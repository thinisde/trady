# X and news source adapters

## Overview

Read-only adapters for reviewed registry entries. The [implementation guide](../../../../docs/news.md)
describes configuration and commands. These adapters return source observations,
not verified claims or trading decisions.

## Interface

- `[mod.rs:SourceClient]`: `fetch` through `NewsSource`, `resolve_x`, `lookup_x`,
  default production transport and injected `with_transport` for fixtures.
- `[mod.rs:parse_feed]`, `canonical_url`, `x_post_id`, `digest`, `PARSER_VERSION`,
  `MAX_BODY_BYTES`: offline RSS/Atom parsing, reference normalization and limits.
- `[http.rs:HttpTransport]`, `HttpRequest`, `HttpResponse`, `SafeHttp`,
  `public_address`, `validate_destination`: bounded HTTPS transport and destination checks.
  `HttpResponse::final_url` supplies the checked redirect destination as the feed base.
- `[x.rs:XUser]`: resolved platform identity; no automatic ownership approval.
- Internal `[x.rs:timeline]`, `lookup`, `resolve`, `[rss.rs:fetch]`, `parse`
  and `[alpha.rs:fetch]` normalize provider responses into `SourceItem`/`SourcePage`.

## Invariants

- `[http.rs:SafeHttp::get]` permits approved HTTPS destinations, pins validated public
  DNS results, disables proxies/compression and bounds time, redirects and body size.
  Authenticated APIs never follow redirects; failures do not expose response bodies.
- `[x.rs:timeline]` checks current handle/stable-ID agreement and accepts only the
  monitored actor's outgoing posts. `[x.rs:normalize]` preserves reference/edit/context
  attribution, marks missing context across every reference, supports legacy response
  aliases and does not infer instrument identity. Author/reference/edit IDs are
  requested through documented expansions, separately from `post.fields`.
- `[rss.rs:parse]` rejects DTD/entity/parser failures and bounds depth, fields and items.
  Atom text constructs preserve nested XHTML; relative links resolve against inherited
  `xml:base` and the fetched feed URL. Feed titles survive element reordering.
  Google results remain metadata-only; RSS supplies snapshot coverage, not complete history.
- `[alpha.rs:fetch]` treats quota notices as failures and carries capped time-window
  subdivisions as continuation work; saturated minutes retain pending siblings and
  remain explicitly incomplete after recoverable work is exhausted.
- `[mod.rs:item]` hashes normalized content, preserves lineage/rights and redacts denied
  storage. Source text and provider errors cannot modify configuration or order state.

## Side Effects

Production fetching resolves DNS, opens HTTPS connections, consumes provider allowance,
resolves configured credential names through the central loader and reads wall-clock times supplied
by callers. Offline parsing and injected fixtures perform no external calls. Persistence
belongs to the application repository, not these source adapters.

## Verification

Run `cargo test -p trady-adapters news::tests --locked` from the repository root.
