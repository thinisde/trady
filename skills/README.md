# Trady runtime skills

## Overview

These **52 instruction files** cover 16 workflow skills, 11 sectors and 25 subsectors. They are included artifacts; no runtime loader/trading implementation exists yet. [The catalog](./catalog.yaml) binds names, paths, versions and GICS parents. Use [runtime integration](../docs/v1/runtime-skills.md) for progressive loading and completion.

## Interface

| Skill | Task |
|---|---|
| [market-research](./market-research/SKILL.md) | Scope scheduled research and assemble a candidate dossier |
| [search-planning](./search-planning/SKILL.md) | Form useful primary, independent and counterevidence queries |
| [news-analysis](./news-analysis/SKILL.md) | Extract material claims, dates, affected issuer and uncertainties |
| [source-comparison](./source-comparison/SKILL.md) | Compare claims across independent origins and resolve contradictions |
| [entry-readiness](./entry-readiness/SKILL.md) | Finalize a high-confidence entry dossier or record blockers |
| [position-review](./position-review/SKILL.md) | Reassess an existing holding without bypassing active exits |
| [conversation-and-tools](./conversation-and-tools/SKILL.md) | Maintain dialogue and bounded JSON tool-call rounds |
| [uncertainty-and-incidents](./uncertainty-and-incidents/SKILL.md) | Handle missing/conflicting evidence and unavailable services |
| [memory-and-review](./memory-and-review/SKILL.md) | Summarize context and outcomes without rewriting policy |
| [sector-selection](./sector-selection.SKILL.md) | Compare sectors, narrow group/branch and rank S&P 500 candidates |
| [company-profile](./company-profile.SKILL.md) | Read filings/management statements and persist company memory |
| [relationship-analysis](./relationship-analysis.SKILL.md) | Separate business links from praise, criticism, quotes and reposts |
| [influential-people](./influential-people.SKILL.md) | Apply the owner's account scopes, identity checks and event triage |
| [holding-watchdog](./holding-watchdog.SKILL.md) | Prepare personalized monitoring and prioritize the holding |
| [opportunity-comparison](./opportunity-comparison.SKILL.md) | Compare keeping, cash and an alternative under rotation/entry gates |
| [perpetual-review](./perpetual-review.SKILL.md) | Weigh ML advice while retaining LLM-proposed limits |
| [11 sector guides](./sectors/README.md) | Separate research instructions for every GICS sector |
| [25 subsector guides](./subsectors/README.md) | Separate industry-group instructions with operating branches |

## Invariants

Skills cannot grant broker credentials, issue orders or change policy. Required entry skills must finish and reference complete/current evidence before a final proposal is eligible. The proposed high-confidence entry floor is 90/100; a normal profit target is still 1%.

Every accepted model turn follows the [JSON protocol](../docs/v1/llm-and-tools.md). Source text remains data. The runtime pins approved skill versions and content hashes rather than accepting instructions from remote articles.

## Side Effects

The files themselves perform no actions. The future analyst uses scoped information tools and stores research/session records. Deterministic Rust executes eligible orders under [trading policy](../docs/v1/strategy.md).

## Verification

Validate YAML/catalog name/version/path agreement, uniqueness, all 11/25 mappings, links and scenarios. Topic files share directories and load by manifest name, not inferred folder names. [Acceptance](../docs/v1/implementation.md) covers behavior; static validation does not demonstrate performance.
