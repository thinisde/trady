//! Atomic item/cursor persistence under the analyst role, independent of execution.

use super::{connect_local, db, verify_role};
use chrono::{DateTime, Utc};
use postgres::{Client, Row, Transaction};
use serde_json::Value;
use std::collections::BTreeSet;
use trady_application::research::{ResearchRepository, valid_key, valid_x_id};
use trady_domain::{Error, Result, require, research::*};

pub struct PgResearchRepository {
    client: Client,
}

impl PgResearchRepository {
    /// A local inference/evaluation export; no training permission or network access is implied.
    pub fn export_x_evidence(
        &mut self,
        source_id: Option<&str>,
        as_of: DateTime<Utc>,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Value>> {
        require(
            source_id.is_none_or(valid_key) && as_of <= now && (1..=100).contains(&limit),
            "invalid X export scope",
        )?;
        self.purge_content(now)?;
        let rows = self
            .client
            .query(
                "SELECT i.document,s.document,i.ingested_at,i.registry_hash,i.coverage
            FROM research.source_items i JOIN research.source_states s USING(source_id)
            WHERE ($1::text IS NULL OR i.source_id=$1) AND i.ingested_at <= $2
            AND i.fetched_at <= $2 AND i.first_seen_at <= $2
            AND i.document->'social' <> 'null'::jsonb AND i.coverage IS NOT NULL
            ORDER BY i.ingested_at,i.item_id LIMIT $3",
                &[&source_id, &as_of, &(limit as i64)],
            )
            .map_err(db)?;
        let mut output = vec![];
        for row in rows {
            let ingested_at: DateTime<Utc> = row.get(2);
            let registry_hash: Option<String> = row.get(3);
            let coverage: Value = row.get(4);
            let item = read_row(row, now, true)?;
            if !item.rights.store_content
                || !item.rights.llm_processing
                || item.expires_at <= now
                || matches!(
                    item.content_status,
                    ContentStatus::Unavailable | ContentStatus::Superseded
                )
            {
                continue;
            }
            output.push(serde_json::json!({"observation":item,"ingested_at":ingested_at,
                "verified_at":null,"feature_ready_at":null,"registry_hash":registry_hash,"coverage":coverage,
                "training_permitted":false,"issuer_mapping_verified":false}));
        }
        Ok(output)
    }

    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_analyst")?;
        Ok(Self { client })
    }

    /// Removes expired/revoked text from current and historical observations.
    pub fn purge_content(&mut self, now: DateTime<Utc>) -> Result<()> {
        let mut tx = self.client.transaction().map_err(db)?;
        purge(&mut tx, now)?;
        tx.commit().map_err(db)
    }

    pub fn read_item(
        &mut self,
        item_id: &str,
        now: DateTime<Utc>,
        for_model: bool,
    ) -> Result<Option<SourceItem>> {
        require(
            item_id.len() == 64 && item_id.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid research item ID",
        )?;
        self.purge_content(now)?;
        self.client.query_opt("SELECT i.document,s.document FROM research.source_items i JOIN research.source_states s USING(source_id) WHERE item_id=$1",
            &[&item_id]).map_err(db)?.map(|row| read_row(row,now,for_model)).transpose()
    }

    /// An explicit unavailable lookup invalidates known copies and stored context.
    pub fn mark_x_unavailable(&mut self, post_id: &str, now: DateTime<Utc>) -> Result<usize> {
        require(valid_x_id(post_id), "invalid X post ID")?;
        let mut tx = self.client.transaction().map_err(db)?;
        let context = serde_json::json!([{"post_id": post_id}]);
        let affected = tx.query("SELECT i.item_id,COALESCE(i.document->'social'->'edit_history_ids'->>0,i.external_id)
            FROM research.source_items i WHERE jsonb_typeof(i.document->'social')='object'
            AND (i.external_id=$1 OR i.document->'social'->'context' @> $2
                OR EXISTS (SELECT 1 FROM research.source_item_revisions r WHERE r.item_id=i.item_id
                    AND r.document->'social'->'context' @> $2))", &[&post_id,&context]).map_err(db)?;
        let roots: BTreeSet<String> = affected.iter().map(|row| row.get(1)).collect();
        for root in roots {
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('trady.x.edit:' || $1,861204734))",
                &[&root],
            )
            .map_err(db)?;
        }
        let mut ids: Vec<String> = affected.iter().map(|row| row.get(0)).collect();
        ids.sort();
        let mut unavailable = 0;
        for id in ids {
            let row = tx
                .query_one(
                    "SELECT document FROM research.source_items WHERE item_id=$1 FOR UPDATE",
                    &[&id],
                )
                .map_err(db)?;
            let mut item: SourceItem = serde_json::from_value(row.get(0))
                .map_err(|_| Error("stored research item is invalid".into()))?;
            if item.external_id == post_id {
                item.content_status = ContentStatus::Unavailable;
                item.redact(now, false);
                unavailable += 1;
            } else if let Some(social) = &mut item.social {
                for referenced in &mut social.context {
                    if referenced.post_id == post_id {
                        referenced.text = None;
                        social.missing_context = true;
                    }
                }
            }
            tx.execute(
                "UPDATE research.source_items SET document=$2 WHERE item_id=$1",
                &[&id, &json(&item)?],
            )
            .map_err(db)?;
            tx.execute("INSERT INTO research.source_invalidations(item_id,replacement_hash,reason) VALUES($1,$2,'UNAVAILABLE') ON CONFLICT DO NOTHING",
                &[&id,&item.content_hash]).map_err(db)?;
            for row in tx.query("SELECT content_hash,document FROM research.source_item_revisions WHERE item_id=$1 FOR UPDATE", &[&id]).map_err(db)? {
                let hash: String = row.get(0);
                let mut old: SourceItem = serde_json::from_value(row.get(1))
                    .map_err(|_| Error("stored research revision is invalid".into()))?;
                if old.external_id == post_id {
                    old.content_status = ContentStatus::Unavailable;
                    old.redact(now, false);
                } else if let Some(social) = &mut old.social {
                    for referenced in &mut social.context {
                        if referenced.post_id == post_id {
                            referenced.text = None;
                            social.missing_context = true;
                        }
                    }
                }
                tx.execute("UPDATE research.source_item_revisions SET document=$3 WHERE item_id=$1 AND content_hash=$2", &[&id,&hash,&json(&old)?]).map_err(db)?;
            }
        }
        purge(&mut tx, now)?;
        tx.commit().map_err(db)?;
        Ok(unavailable)
    }
}

fn read_row(row: Row, now: DateTime<Utc>, for_model: bool) -> Result<SourceItem> {
    let mut item: SourceItem = serde_json::from_value(row.get(0))
        .map_err(|_| Error("stored research item is invalid".into()))?;
    let state: SourceState = serde_json::from_value(row.get(1))
        .map_err(|_| Error("stored source state is invalid".into()))?;
    if let Some(rights) = state.rights {
        item.rights.store_content &= rights.store_content;
        item.rights.llm_processing &= rights.llm_processing;
    }
    item.redact(now, for_model);
    Ok(item)
}

fn json<T: serde::Serialize>(value: &T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error("research record serialization failed".into()))
}

fn purge(tx: &mut Transaction<'_>, now: DateTime<Utc>) -> Result<()> {
    tx.execute("UPDATE research.source_items i SET
        expires_at=LEAST(i.expires_at,i.first_seen_at+(s.document->'rights'->>'retention_hours')::integer*interval '1 hour'),
        document=jsonb_set(i.document,'{expires_at}',to_jsonb(LEAST(i.expires_at,i.first_seen_at+(s.document->'rights'->>'retention_hours')::integer*interval '1 hour')))
        FROM research.source_states s WHERE s.source_id=i.source_id
        AND i.first_seen_at+(s.document->'rights'->>'retention_hours')::integer*interval '1 hour' < i.expires_at", &[]).map_err(db)?;
    tx.execute("UPDATE research.source_item_revisions r SET
        expires_at=LEAST(r.expires_at,i.expires_at),document=jsonb_set(r.document,'{expires_at}',to_jsonb(LEAST(r.expires_at,i.expires_at)))
        FROM research.source_items i WHERE r.item_id=i.item_id AND i.expires_at < r.expires_at", &[]).map_err(db)?;
    tx.execute("UPDATE research.source_items i SET document=research.redact_item(i.document)
        FROM research.source_states s WHERE s.source_id=i.source_id
        AND (i.expires_at <= $1 OR s.document->'rights'->>'store_content'='false')
        AND (i.document->>'feed_title' IS NOT NULL OR i.document->>'title' IS NOT NULL OR i.document->>'summary' IS NOT NULL
            OR i.document->>'text' IS NOT NULL OR i.document->'social'->'context' @? '$[*].text ? (@ != null)')", &[&now]).map_err(db)?;
    tx.execute("UPDATE research.source_item_revisions r SET document=research.redact_item(r.document)
        FROM research.source_items i, research.source_states s
        WHERE r.item_id=i.item_id AND s.source_id=i.source_id
        AND (r.expires_at <= $1 OR s.document->'rights'->>'store_content'='false')
        AND (r.document->>'feed_title' IS NOT NULL OR r.document->>'title' IS NOT NULL OR r.document->>'summary' IS NOT NULL
            OR r.document->>'text' IS NOT NULL OR r.document->'social'->'context' @? '$[*].text ? (@ != null)')", &[&now]).map_err(db)?;
    Ok(())
}

fn validate_item(item: &SourceItem, source_id: &str, at: DateTime<Utc>) -> Result<()> {
    let hash = |s: &str| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit());
    require(
        item.source_id == source_id
            && hash(&item.item_id)
            && hash(&item.content_hash)
            && hash(&item.lineage_id)
            && !item.external_id.is_empty()
            && item.external_id.len() <= 4096
            && item.canonical_url.len() <= 4096
            && item.related_urls.len() <= 16
            && item.first_seen_at <= item.fetched_at
            && item.fetched_at <= at
            && item.expires_at > item.first_seen_at
            && item.title.as_ref().is_none_or(|s| s.len() <= 4096)
            && item.feed_title.as_ref().is_none_or(|s| s.len() <= 4096)
            && item.summary.as_ref().is_none_or(|s| s.len() <= 32768)
            && item.text.as_ref().is_none_or(|s| s.len() <= 65536),
        "invalid research item",
    )?;
    require(
        item.social.as_ref().is_none_or(|s| {
            s.context.len() <= 3
                && s.context
                    .iter()
                    .all(|c| c.text.as_ref().is_none_or(|t| t.len() <= 65536))
        }),
        "invalid research context",
    )
}

impl ResearchRepository for PgResearchRepository {
    fn reserve_shared_requests(
        &mut self,
        source: &trady_application::research::SourceConfig,
        at: DateTime<Utc>,
        requests: u32,
    ) -> Result<bool> {
        use trady_application::financial::{BudgetLimits, ProviderAccount};
        use trady_domain::financial::Provider;
        let (provider, minute, daily) = match source.kind {
            SourceKind::AlphaVantage => (Provider::AlphaVantage, 5, 25),
            SourceKind::Finnhub => (Provider::Finnhub, 20, 200),
            SourceKind::Gdelt => (Provider::Gdelt, 5, 200),
            _ => return Ok(true),
        };
        let daily = daily.min(source.daily_request_limit);
        let mut limits = BudgetLimits {
            per_minute: minute.min(daily),
            per_day: daily,
            per_month: daily * 31,
        };
        let mut tx = self.client.transaction().map_err(db)?;
        if let Some(row) = tx
            .query_opt(
                "SELECT document FROM research.financial_accounts WHERE budget_key=$1",
                &[&provider.budget_key()],
            )
            .map_err(db)?
        {
            let account: ProviderAccount = serde_json::from_value(row.get(0))
                .map_err(|_| Error("invalid provider account".into()))?;
            if !account.enabled {
                return Ok(false);
            }
            limits.per_minute = limits.per_minute.min(account.limits.per_minute);
            limits.per_day = limits.per_day.min(account.limits.per_day);
            limits.per_month = limits.per_month.min(account.limits.per_month);
        }
        let allowed = crate::financial::reserve_credits(
            &mut tx,
            &provider.budget_key(),
            &limits,
            requests,
            at,
        )?;
        tx.commit().map_err(db)?;
        Ok(allowed)
    }
    fn state(&mut self, source_id: &str) -> Result<SourceState> {
        require(valid_key(source_id), "invalid research source ID")?;
        self.client.execute("INSERT INTO research.source_states(source_id,document) VALUES($1,$2) ON CONFLICT DO NOTHING",
            &[&source_id, &json(&SourceState::default())?]).map_err(db)?;
        let row = self
            .client
            .query_one(
                "SELECT revision,document FROM research.source_states WHERE source_id=$1",
                &[&source_id],
            )
            .map_err(db)?;
        let mut state: SourceState = serde_json::from_value(row.get(1))
            .map_err(|_| Error("stored source state is invalid".into()))?;
        state.revision = row.get(0);
        Ok(state)
    }

    fn commit_page(
        &mut self,
        source_id: &str,
        expected_revision: i64,
        state: &SourceState,
        items: &[SourceItem],
        at: DateTime<Utc>,
    ) -> Result<usize> {
        require(
            valid_key(source_id)
                && state.revision == expected_revision
                && items.len() <= 100
                && state
                    .next_token
                    .as_ref()
                    .is_none_or(|s| !s.is_empty() && s.len() <= 2048),
            "invalid research page",
        )?;
        for item in items {
            validate_item(item, source_id, at)?;
            require(
                state.rights.as_ref() == Some(&item.rights),
                "item rights differ from approved source",
            )?;
        }
        let mut tx = self.client.transaction().map_err(db)?;
        let revision: i64 = tx
            .query_one(
                "SELECT revision FROM research.source_states WHERE source_id=$1 FOR UPDATE",
                &[&source_id],
            )
            .map_err(db)?
            .get(0);
        require(
            revision == expected_revision,
            "research source changed; retry from its durable cursor",
        )?;
        let next_revision = revision
            .checked_add(1)
            .ok_or_else(|| Error("source revision overflow".into()))?;
        let mut next_state = state.clone();
        next_state.revision = next_revision;
        let roots: BTreeSet<_> = items
            .iter()
            .filter_map(|i| {
                i.social
                    .as_ref()
                    .map(|s| s.edit_history_ids.first().unwrap_or(&i.external_id))
            })
            .collect();
        for root in roots {
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('trady.x.edit:' || $1,861204734))",
                &[&root],
            )
            .map_err(db)?;
        }
        let mut changed = 0;
        for incoming in items {
            let mut item = incoming.clone();
            if item.social.is_some()
                && tx
                    .query_opt(
                        "SELECT item_id FROM research.source_items
                WHERE document->'social'->'edit_history_ids' ? $1 AND external_id <> $1
                AND document->'social'->'edit_history_ids'->>-1=external_id
                AND document->>'content_status' NOT IN ('SUPERSEDED','UNAVAILABLE') LIMIT 1",
                        &[&item.external_id],
                    )
                    .map_err(db)?
                    .is_some()
            {
                item.content_status = ContentStatus::Superseded;
            }
            let previous = tx
                .query_opt(
                    "SELECT document FROM research.source_items WHERE item_id=$1 FOR UPDATE",
                    &[&item.item_id],
                )
                .map_err(db)?;
            if let Some(row) = &previous {
                let old: SourceItem = serde_json::from_value(row.get(0))
                    .map_err(|_| Error("stored research item is invalid".into()))?;
                require(
                    item.fetched_at >= old.fetched_at,
                    "research fetch time regression",
                )?;
                item.first_seen_at = old.first_seen_at;
                item.expires_at = item.expires_at.min(old.expires_at);
                if old.content_status == ContentStatus::Superseded {
                    item.content_status = ContentStatus::Superseded;
                }
                item.redact(at, false);
                if old.content_hash != item.content_hash
                    || old.content_status != item.content_status
                {
                    tx.execute("INSERT INTO research.source_item_revisions(item_id,content_hash,document,expires_at)
                        VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING", &[&old.item_id,&old.content_hash,&json(&old)?,&old.expires_at]).map_err(db)?;
                    let reason = match item.content_status {
                        ContentStatus::Unavailable => "UNAVAILABLE",
                        ContentStatus::Superseded => "SUPERSEDED",
                        _ => "CONTENT_CHANGED",
                    };
                    tx.execute("INSERT INTO research.source_invalidations(item_id,replacement_hash,reason) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
                        &[&item.item_id,&item.content_hash,&reason]).map_err(db)?;
                    changed += 1;
                }
            } else {
                changed += 1;
            }
            item.redact(at, false);
            tx.execute("INSERT INTO research.source_items(item_id,source_id,external_id,lineage_id,content_hash,document,first_seen_at,fetched_at,expires_at,registry_hash,coverage,ingestion_revision)
                VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(item_id) DO UPDATE SET
                lineage_id=EXCLUDED.lineage_id,content_hash=EXCLUDED.content_hash,document=EXCLUDED.document,
                fetched_at=EXCLUDED.fetched_at,expires_at=EXCLUDED.expires_at,registry_hash=EXCLUDED.registry_hash,coverage=EXCLUDED.coverage,ingestion_revision=EXCLUDED.ingestion_revision",
                &[&item.item_id,&source_id,&item.external_id,&item.lineage_id,&item.content_hash,&json(&item)?,
                    &item.first_seen_at,&item.fetched_at,&item.expires_at,&state.registry_hash,&state.coverage.as_ref().map(json).transpose()?,&next_revision]).map_err(db)?;
            if matches!(
                item.content_status,
                ContentStatus::Unavailable | ContentStatus::Superseded
            ) {
                let status = if item.content_status == ContentStatus::Unavailable {
                    "UNAVAILABLE"
                } else {
                    "SUPERSEDED"
                };
                tx.execute("UPDATE research.source_item_revisions SET document=research.redact_item(document || jsonb_build_object('content_status',$2::text)) WHERE item_id=$1", &[&item.item_id,&status]).map_err(db)?;
            }
            if let Some(social) = &item.social {
                for older in social
                    .edit_history_ids
                    .iter()
                    .take_while(|id| *id != &item.external_id)
                {
                    for row in tx.query("SELECT item_id FROM research.source_items WHERE external_id=$1 AND document->'social' <> 'null'::jsonb ORDER BY item_id FOR UPDATE", &[&older]).map_err(db)? {
                        let older_id: String = row.get(0);
                        tx.execute("UPDATE research.source_items SET document=research.redact_item(document || '{\"content_status\":\"SUPERSEDED\"}'::jsonb) WHERE item_id=$1", &[&older_id]).map_err(db)?;
                        tx.execute("UPDATE research.source_item_revisions SET document=research.redact_item(document || '{\"content_status\":\"SUPERSEDED\"}'::jsonb) WHERE item_id=$1", &[&older_id]).map_err(db)?;
                        tx.execute("INSERT INTO research.source_invalidations(item_id,replacement_hash,reason) VALUES($1,$2,'SUPERSEDED') ON CONFLICT DO NOTHING", &[&older_id,&item.content_hash]).map_err(db)?;
                    }
                }
            }
        }
        tx.execute("UPDATE research.source_states SET revision=$2,document=$3,updated_at=$4 WHERE source_id=$1",
            &[&source_id,&next_revision,&json(&next_state)?,&at]).map_err(db)?;
        let status = json(&state.last_status.unwrap_or(CollectionStatus::Partial))?
            .as_str()
            .ok_or_else(|| Error("invalid collection status".into()))?
            .to_string();
        tx.execute("INSERT INTO research.collection_events(source_id,revision,status,document) VALUES($1,$2,$3,$4)",
            &[&source_id,&next_revision,&status,&json(&next_state)?]).map_err(db)?;
        purge(&mut tx, at)?;
        if state.last_status == Some(CollectionStatus::RateLimited)
            && let (Some(retry), Some(coverage)) = (state.retry_at, state.coverage.as_ref())
        {
            use trady_domain::financial::Provider;
            let provider = match coverage.transport {
                SourceKind::AlphaVantage => Some(Provider::AlphaVantage),
                SourceKind::Finnhub => Some(Provider::Finnhub),
                SourceKind::Gdelt => Some(Provider::Gdelt),
                _ => None,
            };
            if let Some(provider) = provider {
                tx.execute("INSERT INTO research.provider_retry_deadlines(budget_key,retry_at) VALUES($1,$2) ON CONFLICT(budget_key) DO UPDATE SET retry_at=GREATEST(research.provider_retry_deadlines.retry_at,EXCLUDED.retry_at)",&[&provider.budget_key(),&retry]).map_err(db)?;
            }
        }
        tx.commit().map_err(db)?;
        if !items.is_empty() {
            // Content/cursor are durable before recording an availability time. A crash here leaves
            // NULL availability, which exports exclude. The revision guard cannot mark a newer page.
            self.client
                .execute(
                    "UPDATE research.source_items SET ingested_at=clock_timestamp()
                WHERE source_id=$1 AND ingestion_revision=$2 AND ingested_at IS NULL",
                    &[&source_id, &next_revision],
                )
                .map_err(db)?;
        }
        Ok(changed)
    }

    fn read_items(
        &mut self,
        source_id: Option<&str>,
        limit: usize,
        now: DateTime<Utc>,
        for_model: bool,
    ) -> Result<Vec<SourceItem>> {
        require(
            (1..=100).contains(&limit) && source_id.is_none_or(valid_key),
            "invalid research read bounds",
        )?;
        self.purge_content(now)?;
        let limit = limit as i64;
        let rows = self.client.query("SELECT i.document,s.document FROM research.source_items i
            JOIN research.source_states s USING(source_id) WHERE ($1::text IS NULL OR i.source_id=$1)
            ORDER BY i.fetched_at DESC,i.item_id LIMIT $2", &[&source_id,&limit]).map_err(db)?;
        let mut items = vec![];
        for row in rows {
            items.push(read_row(row, now, for_model)?);
        }
        Ok(items)
    }
}
