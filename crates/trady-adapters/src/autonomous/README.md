# Automatic trading inputs

## Overview

[mod.rs](./mod.rs) implements the application `TradingInputs` port over an
approved HTTP input service. It combines the [universe source](../screening/README.md),
application screening, authoritative limit calculation and the
[native Perpetual advisor](../perpetual/README.md) to produce an `EntryCandidate`.
Deployment supplies the input service, research repository and approved model artifacts.
The CLI runs this synchronous analyst adapter in a bounded child process; executor
market reads use the separate [market adapter](./market.rs).

## Interface

- `[mod.rs:TradingInputsConfig]`: `config_version`, `base_url`, `allowed_hosts`
  and `screening`; `validate(&TradingConfig)` checks their deployment binding.
  `[mod.rs:TradingInputsConfig::uses_placeholder_endpoint]` identifies parsed
  hosts `invalid` or ending in `.invalid`, after trimming trailing dots. It
  returns false for an unparseable URL and does not replace `validate`.
- `[mod.rs:ResearchDraft]`: proposal/instrument/currency identity, validity and
  deadline, entry range, confidence/formula evidence, entry/exit costs, instrument
  rules, dossier, feature schema/values and the Perpetual review reason. Its
  `sentiment_reference: Option<SentimentReadinessRef>` defaults to absent on
  deserialization and is forwarded to the candidate. Live readiness requires it;
  `EntryCandidate::validate_dossier` checks its owner/account/policy/instrument,
  content hashes, model revision, original scoring time and expiry.
- `[mod.rs:AnalystPacket]`: versioned owner/account/policy binding, observation,
  validity and retention times, screening observations and proposed drafts.
  `[mod.rs:SessionPacket]` binds an account to an optional `MarketSession`;
  `[mod.rs:CostsPacket]` binds an instrument and times to `EntryCosts`.
- `[mod.rs:PredictionRequest]`: versioned owner/account binding, the frozen
  `LimitsInput` and its `BoundCalculation`, serialized for the configured
  prediction service. `LimitsInput` belongs to policy; `BoundCalculation` and
  the response `ScenarioPredictionInput` belong to the application.
- `[mod.rs:AutomaticInputs]`: generic
  `AutomaticInputs<R, U = HttpUniverseSource, T = SafeHttp>`;
  `new` constructs production ports; `with_ports` injects a `UniverseSource`,
  `HttpTransport` and refresh interval. The `repository: R` field is public.
  `R` implements the application-owned `TradingResearchStore`.
  `with_clock(self, impl Clock + 'static)` replaces the default `SystemClock`.
- `[mod.rs:AutomaticInputs]` implements `TradingInputs::session`,
  `TradingInputs::quote`, `TradingInputs::current_exit_costs` and
  `TradingInputs::candidate` for the application port. Screening, limit
  calculation, assessment persistence and final-scenario review belong to their
  application contracts.
- `[market.rs:MarketInputs<T = SafeHttp>]`: `new`, `with_ports`, `with_clock`,
  `session`, `quote`, `current_exit_costs`. It exposes market reads without a
  model, universe, research repository or candidate method. The shared private
  `InputService<T>` owns HTTP destination, parsing, credential and backoff guards.

## Invariants

- `[mod.rs:TradingInputsConfig::validate]` requires version 1, matching screening
  and trading owners, a base URL ending in `/` without query or fragment, an
  approved HTTPS destination and a valid screening policy. The packet DTOs deny
  unknown fields. `[mod.rs:AutomaticInputs::with_ports]` requires a 60–86,400
  second refresh interval. `[market.rs:InputService::new]` requires a nonempty
  token without ASCII control bytes when supplied.
- `[market.rs:InputService::request]` shares GET/POST guards: HTTP 200, the exact
  requested final URL, at most `MAX_BODY_BYTES` (2 MiB), UTF-8 and bounded JSON parsing. Requests
  disable redirects. HTTP 429/503 activates shared service backoff using a
  1–86,400 second integer `Retry-After`, defaulting to 60 seconds. Market and
  analyst instances maintain independent backoff so analysis throttling does not
  suppress the executor's market reads.
- `[mod.rs:AutomaticInputs::new]` shares the service bearer with the universe
  source only when their parsed URL origins match.
- `[mod.rs:CachedUniverse::fetch_universe]` reuses a universe only before its
  expiry and configured refresh age; refresh failures propagate.
- `[market.rs:InputService::session]` checks the returned account;
  `[market.rs:InputService::quote]` checks the returned instrument. These methods
  do not independently validate session or quote freshness.
  `[market.rs:InputService::costs]` additionally requires an
  observation within 60 seconds and unexpired costs, using the later of the
  supplied time and the clock read after fetching.
- `[mod.rs:AutomaticInputs::candidate]` requires an active limits authority and
  a version-1 analyst packet matching owner, account and policy, observed within
  300 seconds, unexpired, retained beyond validity and containing at most five
  proposals. Only the first screening finalist is considered; no result,
  finalist or matching draft returns `None` without trying later finalists.
- `[mod.rs:AutomaticInputs::candidate]` recomputes confidence and authoritative
  limits. Formula base confidence must equal the recomputed value; formula
  confidence cannot exceed it and exceptional evidence requires at least 95.
  The draft's exchange-calendar deadline is bound into `LimitsInput`.
  Candidate validity incorporates draft, packet, formula, authority and
  calculated-limit expiry and a five-minute ceiling.
  Company-profile evidence, round-trip costs and a valid scenario deadline are required.
- `[mod.rs:AutomaticInputs::candidate]` requires `OK` native advice, successful
  final-scenario review, a persisted assessment and its model before returning a
  validated candidate. The advisor receipt starts unusable; its final drift
  tolerance is capped at `0.001`; assessment market validity is also bounded by
  the quote's observation time plus ten seconds. Model availability and execution authorization
  are separate deployment/application requirements.
- `[mod.rs:AutomaticInputs::candidate]` persists the bound calculation, obtains
  `ScenarioPredictionInput`, and validates `FormulaProposal` against a second
  fetched quote before candidate construction. It rereads the configured clock
  after research, quote/prediction fetching and synchronous native inference for validation. The supplied time
  is used for the preliminary authority check.

## Side Effects

- `[market.rs:InputService::session]`, `quote` and `costs` perform GETs for
  `session/{account}`, `quote/{instrument}` and `costs/{instrument}`.
  `[mod.rs:AutomaticInputs::get]` fetches `research/{account}`;
  `[mod.rs:AutomaticInputs::request]` POSTs `PredictionRequest` to
  `predictions/{account}`. Candidate construction fetches quotes before calculation
  and after prediction retrieval. These requests use the optional bearer and
  `[market.rs:InputService::request]` backoff based on the configured clock.
  `[mod.rs:CachedUniverse::fetch_universe]` replaces an instance-local cache.
- `[mod.rs:AutomaticInputs::native_evidence]` replaces dossier records of the
  supplied kind, rewrites dependent references and appends content-hashed native
  evidence. It does not mark these records `primary_verified`.
- `[mod.rs:AutomaticInputs::candidate]` persists screening, the bound calculation,
  dossier, assessment context, advice and review through the supplied repository,
  then validates the final candidate. Earlier writes can remain after a later
  error; this method provides no enclosing rollback. Calculation/dossier/context
  retention is capped by packet retention and universe-source retention. Native
  inference runs synchronously. This adapter makes no broker order calls and
  ships no trained deployment model.
- `[market.rs:MarketInputs]` performs only the session/quote/cost GETs with the
  same HTTP guards; it does not POST research, screen instruments or persist data.

## Verification

Run `cargo test -p trady-adapters --locked` from the repository root.
Workspace verification is `cargo test --workspace --locked`.
