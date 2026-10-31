# Trady sector, subsector and stock selection

Status: researched specification and included instruction files, checked 2026-10-31. No scanner or scheduled research service is implemented.

## Universe and taxonomy

The entry universe is the **effective S&P 500 membership intersected with the owner's permitted stock set and broker-supported ordinary shares**. The owner's set may be the whole index or a narrower list. Index membership alone does not authorize an instrument or establish liquidity, affordability or protection. Watch private companies, foreign suppliers and nonmembers as evidence about members; do not silently make those entities tradable.

Use GICS's 11 sectors and **25 industry groups as the operational subsectors**. This is the first filtering depth, not a claim that GICS has only two levels: it also has industries and sub-industries. Load the corresponding industry branch within each guide when a group contains different business models. Energy, Materials and Utilities each have one industry group, so their second stage must narrow by operating branch rather than merely repeating the sector label. [S&P GICS methodology](https://www.spglobal.com/spdji/en/documents/methodologies/methodology-gics.pdf)

Store taxonomy edition, provider, effective dates and classification/membership snapshot IDs. The October 2026 methodology still describes 11/25/74/163 levels; a 2026 consultation is a proposal, not an effective classification. Review announced changes before their effective date. Obtain a permitted constituent/classification dataset; a public index description is not a complete redistributable constituent feed. Missing or stale required membership/classification records block new entries; existing holdings remain managed after index removal. [S&P 500](https://www.spglobal.com/spdji/en/indices/equity/sp-500/), [2026 GICS consultation](https://press.spglobal.com/2026-07-17-S-P-DOW-JONES-INDICES-AND-MSCI-ANNOUNCE-CONSULTATION-ON-POTENTIAL-CHANGES-TO-THE-GLOBAL-INDUSTRY-CLASSIFICATION-STANDARD-GICS-R)

Do not hardcode 500 instrument rows: company membership and traded share classes differ, and both can change. Resolve issuer ID/CIK, listing, share class, venue, currency, broker ID and GICS code. Distinguish a corporate sector classification from secondary revenue exposure; an AI beneficiary does not automatically belong to Information Technology.

## Daily funnel

1. **Session and regime.** Resolve the next actual exchange session, macro-release calendar, market breadth, rates, currencies, source health and account state. On a holiday/weekend, prepare for the next session without pretending there is a tradable day. While invested, [holding priority](./watchdogs-and-x.md#holding-watchdog) governs the budget.
2. **Compare all sectors.** A bounded quantitative/data pass covers all 11 sectors; do not call the LLM separately for every stock. Read the relevant sector guides progressively. Record unobserved sectors as `UNKNOWN`, never a measured zero. Compare breadth and equal-weight participation with capitalization-weighted performance to detect a single-stock effect.
3. **Choose today's focus.** Rank up to three sectors for deeper research, choose one primary sector, and record why it beats alternatives and cash. A risk-off or poorly covered day can have no selected sector. The fallback sector is a research fallback, not an automatic trade permission.
4. **Narrow the subsector.** Compare every industry group in the focused sector using its separate guide. Identify the catalyst's actual beneficiaries and losers, then choose one primary group and an operating branch where needed. Keep rejected groups and contrary observations in the screening record.
5. **Shortlist stocks.** Filter effective members by verified instrument support, material exposure, current spread/liquidity, execution economics and event timing. Rank at most five candidates and two peer controls; nominate one finalist. A small account and fractional-order minimum can eliminate an otherwise interesting stock.
6. **Deep company work.** Build/refresh the finalist's [company profile, management statements and relationships](./company-intelligence.md). Read original filings/releases and the CEO, CFO and relevant counterparties' actual statements. Compare independent reporting and adverse evidence.
7. **Finish readiness.** Complete selected sector/group skill artifacts, company and relationship reviews, evidence comparison, deterministic confidence/economics and a valid watchdog template. Only then may an entry proposal reach Rust's readiness gate. No sector score substitutes for the 90-point entry-confidence floor.

The orchestration skill is [sector selection](../../skills/sector-selection.SKILL.md); topic paths are indexed in [sector guides](../../skills/sectors/README.md) and [subsector guides](../../skills/subsectors/README.md). Their instructions are part of the deployed approved bundle, not text fetched from news sites.

## Ranking without false confidence

Use separate ordinal priority components, each `0..4`, with evidence and snapshot references: **new verified catalyst**, **breadth/relative-strength confirmation**, **economic transmission to members**, **tradability after costs**, and **coverage/counterevidence quality**. A deterministic tool sums these to `0..20` only when all five are known. An unknown component makes the row incomplete and ineligible as the selected focus. The model proposes reasons and observation references; a verifier/calculator checks component criteria and returns the calculation ID.

For each component, 0 means observed adverse/no support, 1 weak, 2 mixed, 3 supported, 4 strong support under the versioned rubric. Catalyst requires an actual newly released fact, not just a new article timestamp; strength uses timestamped prices/breadth, not tone. The score is a **research priority**, not a probability, expected return or entry score. Until paper evaluation calibrates the rubric, ties and marginal differences remain ties; prefer better coverage and simpler execution, or no action.

Use a common observation interval, consistent return basis and market-data source across peers. Compare the move with the market and group, realized volatility, volume baseline and spread; do not mix pre-market prints with regular-session returns. Record consensus only when a timestamped permitted forecast dataset exists. Otherwise compare with the previous release/guidance and say that a consensus surprise is unknown. Never invent a market expectation.

The initial breadth pass can be completed across several bounded runs. Persist progress and refresh mutable prices before the final selection. The six-turn model-loop limit applies per run, not to the entire multi-stage day's investigation. A stored stage result is reusable only while its inputs, taxonomy and evidence remain valid.

## Output and persistence

Persist the scope, every covered/rejected sector/group, component calculation references, chosen codes/branch, candidate IDs, reasons and missing coverage through `research.store_artifact`. Store full breadth arrays in PostgreSQL; final JSON references their application-generated IDs so the model context stays bounded. Exact stage contracts are in [research artifact contracts](./research-contracts.md).

A completed entry dossier binds the universe/classification snapshot, screening
revision, skill hashes, profile/relationship review, watchdog and
[Perpetual assessment](./perpetual.md). Under [prediction fusion](./prediction-fusion.md),
the LLM proposes constraints and the approved mathematical engine selects numerical
limits, quantity and horizon. Bind the exact final scenario, original forecasts
and formula/fusion/calibration revisions. The executor refreshes account/prices,
recalculates score/economics and rejects invalidated assessments. A macro scan
cannot repair missing disclosure, incomplete predictions or failed protection.

## Verification

Check a sector led by one mega-cap, a sector with no eligible members, an unknown component, a new index constituent effective tomorrow, an ambiguous share class, a holiday run and Energy's single-group branch filtering. All must leave an auditable selection or a specific no-action reason.
