# Trady company profiles and relationships

Status: proposed PostgreSQL research components and included skills. This task creates no populated company database.

## Company deep dive

For each screened candidate, reuse its attributed historical profile, then refresh the finalist before entry. Use stable issuer/entity IDs, CIK where applicable, approved instrument/share-class links, effective sector/group, official domains, product aliases, segment exposure and management appointments. Read the latest annual filing, subsequent quarterly filings and material current reports, current earnings release/guidance and relevant product/regulator releases. SEC submissions and structured financial facts provide original disclosure paths; company statements still require interpretation and independent comparison. [SEC EDGAR APIs](https://www.sec.gov/search-filings/edgar-application-programming-interfaces)

Capture operations and revenue/profit drivers, cash/debt/dilution, customer/supplier concentration, geographic and currency exposure, regulatory/product milestones, valuation context and upcoming dated events. Store financial period, units, currency, GAAP/non-GAAP basis and original reconciliation with each metric. A revised filing supersedes a fact; it does not erase the version used in an earlier trade. A current web page can contain an old role description, so appointment dates and filings outrank an undated biography for role history.

After selecting the finalist, investigate what the **CEO, CFO, relevant product leader, board spokesperson, regulator, major customer and supplier** actually said about the catalyst. Include earnings Q&A, official prepared remarks, permitted interview transcripts, public X posts/replies/quotes/reposts and company announcements. An absence of a personal X account is not missing research when equivalent original statements are available. Do not invent an account for an executive.

Use three time windows: latest filing/earnings baseline; all material changes since that baseline; and at least the last 72 hours plus the original catalyst's first publication for current statements. The window expands when a statement refers to an older promise or dispute. Record exact query windows and access gaps. For a continuing holding, collect changes since its verified watermark with an overlap; do not repeatedly reread the whole profile at every alert.

The [company-profile skill](../../skills/company-profile.SKILL.md) asks whether a new promise changes revenue, cost, timing, regulation, funding or competition. A CEO's optimism, product demo, contract announcement and recognized revenue are different facts. Countercheck later corrections, customer statements, contract conditions, capacity and filing risk factors. Preserve the source's incentives, not just its sentiment.

## Directional mentions and economic links

Represent the public-source graph in ordinary PostgreSQL tables; a separate graph database is unnecessary for v1. Company/person/product/regulator are entity types. Separate **economic relationships** (`SUPPLIES`, `CUSTOMER_OF`, `PARTNERS_WITH`, `COMPETES_WITH`, `INVESTED_IN`, `REGULATES`, `LEADS`) from **attributed mention events** (`PRAISES`, `CRITICIZES`, `MENTIONS`, `QUOTES`, `REPOSTS`). An economic edge needs its own disclosure evidence. A compliment does not establish a contract, and criticism does not establish a factual defect.

Each event has speaker/author entity, actual target entity, optional quoted speaker, post/transcript/filing reference and permitted locator/excerpt, publication/event time, direction, event type, tone, stance confidence, entity-resolution confidence, materiality, economic transmission hypothesis, contrary evidence and expiry/review date. Confidence in identifying the target or tone is **separate from trading confidence**. Ambiguous sarcasm, names or quoted speech stays `UNCLEAR`/unresolved.

For example, if executive A quotes executive B's criticism of company C, store B → C criticism and A → B quotation separately only when each attribution is readable. Do not label A as criticizing C without supporting context. A repost records distribution; it does not automatically mean endorsement, a business relationship, or an independent reporting lineage. Deleted/edited statements update dependent evidence and invalidate stale material interpretations under source retention rules.

Expand at most two relationship hops, initially ten relevant neighbors per finalist. Prefer disclosed revenue/cost exposure, a direct named target and a recent material event. Unverified co-mentions can be stored as candidate associations, excluded from factual graph edges and entry evidence. Relationship searches must look for both praise and criticism, actual cooperation and denied/ended arrangements; avoiding only unfavorable sources would bias the profile.

Use the [relationship-analysis skill](../../skills/relationship-analysis.SKILL.md) to ask: what was said, by whom, about whom, when, in what context, and through which verified economic channel could it matter for this listed issuer? A person's market visibility does not establish any causal price effect. Event studies need time-aligned observations, competing news and market/group controls, not a screenshot after a price move.

## Database model and write ownership

| Planned relation | Key content and invariant |
|---|---|
| `research.entities` / `entity_aliases` | Application-resolved stable IDs, entity type, canonical name, official domains, alias validity; ambiguous aliases never merge automatically |
| `research.company_profiles` | Issuer ID and immutable revision, baseline filing IDs, as-of/review times, material gaps, profile status; current pointer updated by validated compare-and-swap |
| `research.profile_facts` | Typed metric/claim, period, units/currency, source/evidence IDs, observed/valid times, certainty and superseded-by; hypotheses distinct from facts |
| `research.person_roles` / `social_accounts` | Person → organization role with effective dates; platform user ID, historical handles, ownership evidence and verification state |
| `research.relationships` | Directed economic edge, disclosed period/scope, evidence, validity, verification and supersession |
| `research.mention_events` | Speaker/target/reference edges, attribution, tone and ambiguity, content rights, extraction revision; no inference of endorsement from repost |
| `research.profile_reviews` | Candidate/run, pinned profile revision, checked statements/relations, independent comparison and blockers |
| `research.artifacts` / `artifact_dependencies` | Immutable typed stage outputs and their evidence/snapshot/skill versions; permission/expiry invalidation propagates to dependents |

Collectors store permitted fetched items before advancing watermarks. The analyst submits typed draft facts/mentions/reviews through `research.store_artifact`; the application validates identity, provenance, schema, rights and references before committing them. A draft cannot promote itself to a verified fact or overwrite a financial state table. Trusted source-verifier outcomes and profile merge validation set authoritative status. The model receives generated IDs in tool results.

Store **all investigation outcomes**, including rejected stocks, contradictions, inaccessible URLs, missing contexts and reasons an item was irrelevant. Deduplicate identical raw material and preserve underlying reporting lineage. “All” means all permitted collected findings and their provenance, not unlimited web mirroring. When storage/processing rights prohibit raw text, store permitted metadata and an access result instead. Purges must remove dependent quotes, summaries, embeddings and memory projections when required, keeping only permitted non-content audit references. [Memory and data](./memory-and-data.md) defines PostgreSQL authority and permissions.

Company memory projections use `companies/<entity-id>/profile.md`, `people/<entity-id>.md` and permitted relationship summaries. Include database revision, evidence links, as-of and expiry; regenerate after corrections or deletion. Never copy a verified badge or an old summary into a new fact without rechecking its underlying evidence.

## Refresh and verification

Refresh profiles on filings, results, management/relationship changes and material corrections; check their freshness before each entry. During a holding, changes update its watchdog dependencies. A missing original executive statement is a documented gap; if it is material to the thesis, it blocks entry. A resolved company profile may validly say there were no relevant recent executive statements after the recorded searches.

Acceptance includes duplicate company names, CEO changes, disputed quotations, praise mistaken for a partnership, repost attribution, nonmember counterparties, corrected guidance, deletion propagation and repeat ingestion. A model-authored database ID, unsupported reference or forged verification flag must fail validation.
