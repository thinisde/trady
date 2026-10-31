---
name: sector-selection
description: >-
  Choose Trady's daily S&P 500 sector, then industry-group subsector and stock
  shortlist using effective membership, comparable evidence and a no-entry alternative.
metadata:
  version: "1.0.0"
---

# Sector then subsector selection

Read [the funnel and scoring rules](../docs/v1/sector-workflow.md). Start with the actual exchange session, approved universe snapshot, source health and whether a position is held. The whole index is a screening universe, not permission to buy every constituent. A weekend run prepares the next session.

Obtain a bounded all-sector market snapshot and current catalysts. Compare all 11 sectors before choosing up to three research priorities and one focus. Use the sector guides in [the catalog](./sectors/README.md); incomplete coverage stays unknown. Check breadth/equal-weight participation, relative returns, volume/spreads, macro sensitivity and a concrete revenue/cost channel. Ask whether the move is already priced in. Do not choose whichever sector has the most posts.

Load the focus sector's guide and all its relevant group guides from [subsectors](./subsectors/README.md). Compare groups, select one, then filter by operating branch and material issuer exposure. In a single-group sector, branch filtering is mandatory. Use effective GICS mapping, not an assumed label such as putting every AI company into Technology.

Build at most five eligible candidates plus two peer controls. Store exclusions, uncertainty and a cash alternative. Use `calculator.research_priority` for component validation and the separate 0–20 priority calculation; never present it as entry confidence. An unknown component cannot win the selection. Search for adverse evidence and the best rival sector/group, not only confirmation of the leading narrative.

Store `SCREENING` artifacts through `research.store_artifact`, using the [exact contract](../docs/v1/research-contracts.md). Reuse validated stage records across bounded runs, refreshing mutable inputs. Complete the finalist's [profile](./company-profile.SKILL.md), [relationships](./relationship-analysis.SKILL.md) and [watchdog template](./holding-watchdog.SKILL.md) before [entry readiness](./entry-readiness/SKILL.md). Only the chosen topic guides are mandatory for the final dossier; every sector still needs a recorded breadth observation.

Return final JSON with tool-supplied screening/universe references in `research.focus`, selected/rejected reasons and current blockers. No eligible focus, missing classification or weak research means no entry. Existing position protection never waits for this funnel.
