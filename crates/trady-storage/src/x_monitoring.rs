//! Shared, serialized billing reservations. A crash never refunds a provider request.
use super::{connect_local, db, verify_role};
use chrono::{DateTime, Utc};
use postgres::{Client, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use trady_application::{research::valid_key, x_monitoring::*};
use trady_domain::{Error, Result, require};

pub struct PgXCostRepository {
    client: Client,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct XReconciliation {
    pub provider_total_microusd: u64,
    pub as_of: DateTime<Utc>,
    pub evidence_reference: String,
}

fn document(value: &impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error("X ledger serialization failed".into()))
}

fn lock(tx: &mut Transaction<'_>, scope: &str) -> Result<()> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended($1,861204735))",
        &[&scope],
    )
    .map_err(db)?;
    Ok(())
}

fn totals(
    tx: &mut Transaction<'_>,
    scope: &str,
    start: DateTime<Utc>,
) -> Result<(i64, i64, i64, bool)> {
    let row = tx.query_one("SELECT
        COALESCE(sum(r.ordinary_microusd),0)::bigint,
        COALESCE(sum(r.support_microusd),0)::bigint,
        c.provider_total_microusd + COALESCE(sum(r.ordinary_microusd+r.support_microusd)
            FILTER (WHERE r.reserved_at > c.provider_as_of OR r.settled_at > c.provider_as_of
                OR r.settled_at IS NULL OR r.outcome='UNKNOWN_CONSUMPTION'),0)::bigint,
        c.blocked
        FROM research.x_budget_cycles c LEFT JOIN research.x_cost_reservations r USING(billing_scope,cycle_start)
        WHERE c.billing_scope=$1 AND c.cycle_start=$2 GROUP BY c.billing_scope,c.cycle_start", &[&scope,&start]).map_err(db)?;
    Ok((row.get(0), row.get(1), row.get(2), row.get(3)))
}

impl PgXCostRepository {
    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_analyst")?;
        Ok(Self { client })
    }

    pub fn status(&mut self, policy: &XBudgetPolicy) -> Result<Value> {
        policy.validate()?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &policy.billing_scope)?;
        let exists=tx.query_opt("SELECT policy_hash FROM research.x_budget_cycles WHERE billing_scope=$1 AND cycle_start=$2",
            &[&policy.billing_scope,&policy.cycle_start]).map_err(db)?;
        let result = if let Some(row) = exists {
            let (ordinary, support, provider, blocked) =
                totals(&mut tx, &policy.billing_scope, policy.cycle_start)?;
            let effective = (ordinary + support).max(provider);
            let by_rule:Vec<Value>=tx.query("SELECT source_id,rule_hash,count(*)::bigint,sum(ordinary_microusd)::bigint,sum(support_microusd)::bigint,
                count(*) FILTER (WHERE outcome IN ('RESERVED','UNKNOWN_CONSUMPTION'))::bigint,
                COALESCE(sum((COALESCE(actual_resources,resource_bound)->'ordinary'->>'posts')::bigint),0)::bigint,
                COALESCE(sum((COALESCE(actual_resources,resource_bound)->'support'->>'posts')::bigint),0)::bigint,
                COALESCE(sum((COALESCE(actual_resources,resource_bound)->'support'->>'users')::bigint),0)::bigint,
                COALESCE(sum((COALESCE(actual_resources,resource_bound)->'support'->>'counts')::bigint),0)::bigint
                FROM research.x_cost_reservations WHERE billing_scope=$1 AND cycle_start=$2 GROUP BY source_id,rule_hash ORDER BY source_id,rule_hash",
                &[&policy.billing_scope,&policy.cycle_start]).map_err(db)?.into_iter().map(|r| json!({
                    "source_id":r.get::<_,String>(0),"rule_hash":r.get::<_,Option<String>>(1),"requests":r.get::<_,i64>(2),
                    "ordinary_microusd":r.get::<_,i64>(3),"support_microusd":r.get::<_,i64>(4),"uncertain_requests":r.get::<_,i64>(5),
                    "ordinary_post_reads":r.get::<_,i64>(6),"context_post_reads":r.get::<_,i64>(7),"user_reads":r.get::<_,i64>(8),"counts_requests":r.get::<_,i64>(9)
                })).collect();
            json!({"ordinary_microusd":ordinary,"support_microusd":support,"effective_microusd":effective,
                "remaining_microusd":policy.ceiling_microusd.saturating_sub(effective as u64),"blocked":blocked,
                "policy_matches":row.get::<_,String>(0)==fingerprint(policy)?,"by_source_rule":by_rule})
        } else {
            json!({"ordinary_microusd":0,"support_microusd":0,"effective_microusd":0,
            "remaining_microusd":policy.ceiling_microusd,"blocked":false,"policy_matches":true,"by_source_rule":[]})
        };
        tx.commit().map_err(db)?;
        Ok(
            json!({"billing_scope":policy.billing_scope,"cycle_start":policy.cycle_start,"cycle_end":policy.cycle_end,
            "policy_version":policy.version,"currency":"USD","amount_unit":"microdollars","accounting":result,
            "local_estimate_is_not_final_invoice":true,"entry_authorized":false}),
        )
    }

    pub fn reconcile(&mut self, policy: &XBudgetPolicy, evidence: &XReconciliation) -> Result<()> {
        policy.validate()?;
        require(
            evidence.provider_total_microusd <= 1_000_000_000_000
                && evidence.as_of >= policy.cycle_start
                && evidence.as_of < policy.cycle_end
                && evidence.as_of <= Utc::now()
                && !evidence.evidence_reference.trim().is_empty()
                && evidence.evidence_reference.len() <= 512,
            "invalid X provider reconciliation",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &policy.billing_scope)?;
        let row=tx.query_one("SELECT policy_hash,provider_total_microusd,provider_as_of FROM research.x_budget_cycles
            WHERE billing_scope=$1 AND cycle_start=$2 FOR UPDATE", &[&policy.billing_scope,&policy.cycle_start]).map_err(db)?;
        require(
            row.get::<_, String>(0) == fingerprint(policy)?
                && evidence.provider_total_microusd >= row.get::<_, i64>(1) as u64
                && row
                    .get::<_, Option<DateTime<Utc>>>(2)
                    .is_none_or(|at| at <= evidence.as_of),
            "X reconciliation cannot change policy or regress provider usage",
        )?;
        let amount = evidence.provider_total_microusd as i64;
        tx.execute("INSERT INTO research.x_cost_reconciliations(billing_scope,cycle_start,provider_total_microusd,provider_as_of,evidence_reference)
            VALUES($1,$2,$3,$4,$5)", &[&policy.billing_scope,&policy.cycle_start,&amount,&evidence.as_of,&evidence.evidence_reference]).map_err(db)?;
        tx.execute("UPDATE research.x_budget_cycles SET provider_total_microusd=$3,provider_as_of=$4,blocked=false
            WHERE billing_scope=$1 AND cycle_start=$2", &[&policy.billing_scope,&policy.cycle_start,&amount,&evidence.as_of]).map_err(db)?;
        tx.commit().map_err(db)
    }
}

impl XCostRepository for PgXCostRepository {
    fn reserve(
        &mut self,
        policy: &XBudgetPolicy,
        scope: &XCostScope,
        plan: XCostPlan,
        at: DateTime<Utc>,
    ) -> Result<Option<i64>> {
        policy.active(at)?;
        let hash_ok = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        require(
            valid_key(&scope.source_id)
                && hash_ok(&scope.registry_hash)
                && scope.rule_hash.as_deref().is_none_or(hash_ok),
            "invalid X accounting scope",
        )?;
        let (ordinary, support) = plan.costs(policy)?;
        require(
            ordinary + support > 0 && ordinary <= i64::MAX as u64 && support <= i64::MAX as u64,
            "invalid X reservation amount",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &policy.billing_scope)?;
        let now: DateTime<Utc> = tx
            .query_one("SELECT clock_timestamp()", &[])
            .map_err(db)?
            .get(0);
        policy.active(now)?;
        let overlap = tx
            .query_opt(
                "SELECT cycle_start FROM research.x_budget_cycles WHERE billing_scope=$1
            AND cycle_start < $3 AND cycle_end > $2 AND cycle_start <> $2 LIMIT 1",
                &[
                    &policy.billing_scope,
                    &policy.cycle_start,
                    &policy.cycle_end,
                ],
            )
            .map_err(db)?;
        require(
            overlap.is_none(),
            "overlapping X billing cycles cannot reset spend",
        )?;
        let hash = fingerprint(policy)?;
        tx.execute("INSERT INTO research.x_budget_cycles(billing_scope,cycle_start,cycle_end,policy_hash,policy)
            VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING", &[&policy.billing_scope,&policy.cycle_start,&policy.cycle_end,&hash,&document(policy)?]).map_err(db)?;
        let old:String=tx.query_one("SELECT policy_hash FROM research.x_budget_cycles WHERE billing_scope=$1 AND cycle_start=$2",
            &[&policy.billing_scope,&policy.cycle_start]).map_err(db)?.get(0);
        require(old == hash, "X billing policy is immutable within a cycle")?;
        let (spent_ordinary, spent_support, provider, blocked) =
            totals(&mut tx, &policy.billing_scope, policy.cycle_start)?;
        let allowed = !blocked
            && (spent_ordinary as u64).saturating_add(ordinary) <= policy.ordinary_microusd
            && (spent_support as u64).saturating_add(support) <= policy.support_microusd
            && ((spent_ordinary + spent_support).max(provider) as u64)
                .saturating_add(ordinary)
                .saturating_add(support)
                <= policy.ceiling_microusd;
        if !allowed {
            tx.commit().map_err(db)?;
            return Ok(None);
        }
        let id:i64=tx.query_one("INSERT INTO research.x_cost_reservations(billing_scope,cycle_start,source_id,registry_hash,rule_hash,
            resource_bound,ordinary_microusd,support_microusd,outcome) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'RESERVED') RETURNING reservation_id",
            &[&policy.billing_scope,&policy.cycle_start,&scope.source_id,&scope.registry_hash,&scope.rule_hash,&document(&plan)?,&(ordinary as i64),&(support as i64)]).map_err(db)?.get(0);
        tx.commit().map_err(db)?;
        Ok(Some(id))
    }

    fn settle(&mut self, reservation: i64, actual: Option<XCostPlan>, outcome: &str) -> Result<()> {
        require(
            matches!(
                outcome,
                "OBSERVED" | "UNKNOWN_CONSUMPTION" | "RESOURCE_BOUND_EXCEEDED"
            ),
            "invalid X settlement outcome",
        )?;
        require(
            actual.is_none() == (outcome == "UNKNOWN_CONSUMPTION"),
            "uncertain X consumption cannot refund a reservation",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        let row=tx.query_one("SELECT billing_scope,cycle_start FROM research.x_cost_reservations WHERE reservation_id=$1", &[&reservation]).map_err(db)?;
        let scope: String = row.get(0);
        let start: DateTime<Utc> = row.get(1);
        lock(&mut tx, &scope)?;
        let row=tx.query_one("SELECT r.outcome,r.actual_resources,c.policy,r.resource_bound FROM research.x_cost_reservations r
            JOIN research.x_budget_cycles c USING(billing_scope,cycle_start) WHERE reservation_id=$1 FOR UPDATE OF r", &[&reservation]).map_err(db)?;
        let bound: XCostPlan = serde_json::from_value(row.get(3))
            .map_err(|_| Error("invalid stored X resource bound".into()))?;
        let exceeds = actual.is_some_and(|a| {
            a.ordinary.posts > bound.ordinary.posts
                || a.ordinary.users > bound.ordinary.users
                || a.ordinary.counts > bound.ordinary.counts
                || a.support.posts > bound.support.posts
                || a.support.users > bound.support.users
                || a.support.counts > bound.support.counts
        });
        let outcome = if exceeds {
            "RESOURCE_BOUND_EXCEEDED"
        } else if actual.is_some() {
            "OBSERVED"
        } else {
            "UNKNOWN_CONSUMPTION"
        };
        if row.get::<_, String>(0) != "RESERVED" {
            require(
                row.get::<_, String>(0) == outcome
                    && row.get::<_, Option<Value>>(1)
                        == actual.as_ref().map(document).transpose()?,
                "X reservation already settled differently",
            )?;
            return tx.commit().map_err(db);
        }
        let policy: XBudgetPolicy = serde_json::from_value(row.get(2))
            .map_err(|_| Error("invalid stored X policy".into()))?;
        if let Some(actual) = actual {
            let (ordinary, support) = actual.costs(&policy)?;
            require(
                ordinary <= i64::MAX as u64 && support <= i64::MAX as u64,
                "invalid X settlement amount",
            )?;
            tx.execute("UPDATE research.x_cost_reservations SET actual_resources=$2,ordinary_microusd=$3,support_microusd=$4 WHERE reservation_id=$1",
                &[&reservation,&document(&actual)?,&(ordinary as i64),&(support as i64)]).map_err(db)?;
        }
        tx.execute("UPDATE research.x_cost_reservations SET outcome=$2,settled_at=clock_timestamp() WHERE reservation_id=$1", &[&reservation,&outcome]).map_err(db)?;
        if outcome == "RESOURCE_BOUND_EXCEEDED" {
            tx.execute("UPDATE research.x_budget_cycles SET blocked=true WHERE billing_scope=$1 AND cycle_start=$2", &[&scope,&start]).map_err(db)?;
        }
        tx.commit().map_err(db)
    }
}
