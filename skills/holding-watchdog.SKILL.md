---
name: holding-watchdog
description: >-
  Prepare and refresh a personalized Trady stock watchdog using thesis events,
  company people, related entities and sector drivers while preserving price protection.
metadata:
  version: "1.0.0"
---

# Personalized stock watchdog

Read [the watchdog contract](../docs/v1/watchdogs-and-x.md#holding-watchdog), current policy, selected sector/group guides, profile, relationships and thesis. Before entry, prepare the required template. On a holding review, read actual position/plan version, current protection, last collection watermarks and source health before interpreting news.

Choose concrete selectors: company/product aliases; official filings/IR; verified CEO/CFO/relevant leader accounts; thesis deadlines and regulatory milestones; disclosed material customers/suppliers/competitors; and guide-specific macro/sector drivers. Record why each matters. Prefer a few relevant sources over every account in a graph. Keep quotes, FX, broker orders/protection, trading halts and broad market shock detection in the mandatory safety path.

Propose `WATCHDOG_TEMPLATE` through `research.store_artifact` with allowlisted source/account/entity IDs, bounded literal terms, severities, cadence, cooldown and expiry. No arbitrary code, regex execution, URLs, cron, SQL or shell. Rust validates capabilities/rate budgets and compiles jobs. A missing mandatory source is a blocker; an optional-source gap is explicit.

While held, prioritize new facts that could invalidate the thesis, change costs/earnings/timing or affect an already active threshold. Coalesce copied stories and repeated mentions. Use [position review](./position-review/SKILL.md) for threshold changes; never remove a reached exit, loosen protection or postpone the original deadline. A model/tool failure retains the last valid plan.

Defer unrelated news, storing permitted metadata and the reason. Reserve the documented 80/10/10 research split; protection has its own capacity. Route a genuinely material alternative through [opportunity comparison](./opportunity-comparison.SKILL.md), not an impulsive sell request. If the entry slot is used, report it before promising a same-day switch.

Return the validated template ID in `research.focus`, coverage gaps and specific alerts. A template is not active until the application binds a confirmed fill to the real position; retired or stale-version jobs cannot modify a current plan. Keep critical subscriptions until flat reconciliation completes.
