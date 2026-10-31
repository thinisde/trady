---
name: influential-people
description: >-
  Review Trady's five priority public X speakers, verify their identities and interpret
  their outgoing posts, replies, quotes and reposts with issuer-specific evidence.
metadata:
  version: "1.0.0"
---

# Five-person X watchlist

Read [the owner-selected people and listener contract](../docs/v1/watchdogs-and-x.md#five-priority-people-on-x). In the owner's order: Trump and Musk are P0; Gill/Roaring Kitty is P0 specifically for GME; Ackman and Icahn are P1. This is a qualitative potential-influence ranking. Scope priority to the actual issuer/topic, verify current ownership/role/API ID and activity, and never invent IDs or activate a pending-verification handle. These five cannot be automatically replaced by a monthly review.

Use `x.listener_status` and `profiles.get` to check coverage. Only deployment/owner policy controls subscriptions. A personal post may be less authoritative than an actual company filing, agency action or effective regulation. Include official alternative channels and a clear warning in the research artifact when a speaker mainly communicates elsewhere.

For each event, distinguish the person's own action from incoming reactions. X Activity `post.create` covers their originals, replies, quotes and reposts; `post.reply.create`/`post.repost.create` describe others acting on their posts. Fetch bounded parent/quoted/original context. Record actor versus original author, edit lineage, deletion state and missing context. A repost is neither independent confirmation nor automatic endorsement.

Map the topic to a verified member's exposure. GME/meme-stock, crypto, currency and bond attention cannot expand the S&P 500/owner universe. Check Trump's permitted Truth Social/official channels and deduplicate cross-posts. Ignore personal banter without a material channel. Use [news analysis](./news-analysis/SKILL.md) and [source comparison](./source-comparison/SKILL.md) with adverse official evidence. Persist company/event/novelty/uncertainty features for [Perpetual](./perpetual-review.SKILL.md); priority flags are not numerical return weights. Prefer lasting information over chasing an immediate spike.

Propose monthly watchlist review notes using material original events, coverage, false alerts and sector relevance. Do not infer influence from cherry-picked winning posts or a follower count. Store event findings/mentions in the database through typed tools; store review notes as observations. Handle/API access failures remain visible gaps. This skill cannot turn on inaccessible services or issue orders.
