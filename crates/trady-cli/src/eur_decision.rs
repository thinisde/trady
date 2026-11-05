//! Research selection and deterministic risk planning for durable EUR SHADOW.
//! Inputs are captured observations; this module has no broker mutation port.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use trady_adapters::opencode::llm::OpenCodeGoConfig;
use trady_application::{
    analyst::AnalystRequest,
    eur_runtime::{EurRuntimeConfig, build_risk_plan},
    eur_shadow::ShadowCandidate,
    harness::content_hash,
    market_tools::EurAnalystReadPort,
};
use trady_domain::{
    AccountSnapshot, BarSnapshot, Currency, EurUniverseSnapshot, Quote, RankedCandidate, Result,
    SelectionFailure, SelectionRun, SignalSnapshot, Strategy, require,
};
use trady_domain::{
    selection::{ModelFingerprint, SelectionFallbackStage},
    strategy::DonchianStrategy,
};
use trady_policy::{EntryCosts, InstrumentRules, economics::ExitCosts};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedCosts {
    pub review_reference: String,
    pub reviewed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub entry: EntryCosts,
    pub exit: ExitCosts,
}

pub struct CapturedTools(pub BTreeMap<String, Value>);
impl EurAnalystReadPort for CapturedTools {
    fn execute(&mut self, name: &str, args: &Value) -> Result<Value> {
        let id = args["evidence_id"]
            .as_str()
            .or(args["instrument_id"].as_str())
            .unwrap_or_default();
        if name == "evidence.search" {
            return Ok(
                json!({"items":self.0.iter().filter(|(k,v)| !k.contains(':') && v["instrument_id"].as_str()==Some(id)).map(|(k,_)|json!({"evidence_id":k})).collect::<Vec<_>>()}),
            );
        }
        let value = self
            .0
            .get(&format!("{name}:{id}"))
            .or_else(|| self.0.get(id))
            .cloned()
            .ok_or_else(|| {
                trady_domain::Error("requested captured research observation unavailable".into())
            })?;
        if name == "market.bars" {
            let bars:Vec<_>=value["bars"].as_array().into_iter().flatten().rev().take(3).map(|b|json!({"closes_at":b["closes_at"],"high":b["high"],"low":b["low"],"close":b["close"],"volume":b["volume"]})).collect();
            return Ok(
                json!({"snapshot_id":value["snapshot_id"],"instrument_id":value["instrument_id"],"provider":value["provider"],"interval_seconds":value["interval_seconds"],"completed_bar_count":value["bars"].as_array().map(Vec::len),"recent_completed_bars":bars}),
            );
        }
        if name == "market.quote" {
            let mut compact = serde_json::Map::new();
            for key in [
                "instrument_id",
                "symbol",
                "broker_ticker",
                "isin",
                "currency",
                "bid",
                "ask",
                "observed_at",
                "received_at",
                "market_state",
                "fresh",
                "provider_instrument_id",
                "source_venue",
            ] {
                if let Some(v) = value.get(key) {
                    compact.insert(key.into(), v.clone());
                }
            }
            return Ok(Value::Object(compact));
        }
        Ok(value)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn decide(
    config: &EurRuntimeConfig,
    cycle_id: &str,
    candidates: &[ShadowCandidate],
    observations: &BTreeMap<String, Value>,
    account: &AccountSnapshot,
    universe: Option<&EurUniverseSnapshot>,
    costs: Option<&ReviewedCosts>,
    deadlines: &BTreeMap<String, DateTime<Utc>>,
    llm: OpenCodeGoConfig,
    now: DateTime<Utc>,
    refresh_quote: &mut dyn FnMut(&trady_domain::InstrumentId) -> Result<(Quote, String)>,
    select: &mut dyn FnMut(&AnalystRequest, CapturedTools) -> Result<SelectionRun>,
) -> Result<Value> {
    let run_id = format!("{cycle_id}_research");
    let fingerprint = ModelFingerprint {
        run_id: run_id.clone(),
        session_id: cycle_id.into(),
        provider: "opencode-go".into(),
        model: config.opencode_model.clone(),
        prompt_hash: content_hash("eur_selection_prompt_v1"),
        tool_manifest_hash: content_hash("captured_eur_research"),
        model_revision: content_hash(
            &serde_json::to_string(&llm)
                .map_err(|_| trady_domain::Error("model configuration encoding failed".into()))?,
        ),
    };
    let mut signals = BTreeMap::<String, SignalSnapshot>::new();
    let mut strategy_assessments = BTreeMap::new();
    let mut tools = BTreeMap::new();
    let mut ranks = Vec::new();
    let mut gaps = Vec::new();
    for c in candidates {
        let id = c.instrument_id.to_string();
        let mut excluded: Vec<String> = c
            .blockers
            .iter()
            .filter(|b| {
                !matches!(
                    b.as_str(),
                    "BROKER_EXECUTION_RULES_UNVERIFIED"
                        | "QUOTE_TO_BROKER_COMPATIBILITY_UNACCEPTED"
                )
            })
            .cloned()
            .collect();
        let bars: Option<BarSnapshot> = observations
            .get(&format!("bars:{id}"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let deadline = deadlines.get(&id).copied();
        let (signal, status, detail) = match (&bars, deadline) {
            (Some(bars), Some(deadline))
                if deadline > now && bars.instrument_id == c.instrument_id =>
            {
                let current_session_open =
                    observations
                        .get(&format!("session:{id}"))
                        .and_then(|session| {
                            serde_json::from_value::<DateTime<Utc>>(session["opens_at"].clone())
                                .ok()
                        });
                if !current_session_open.is_some_and(|open| {
                    trady_application::eur_shadow::completed_bar_is_current(bars, open, now)
                }) {
                    (None, "COMPLETED_BAR_STALE_OR_PRIOR_SESSION", None)
                } else {
                    match DonchianStrategy::intraday_default().evaluate(bars, deadline) {
                        Ok(Some(signal)) => (Some(signal), "SIGNAL_AVAILABLE", None),
                        Ok(None) => (None, "NO_BREAKOUT", None),
                        Err(error) => (None, "COMPLETED_BAR_EVALUATION_REJECTED", Some(error.0)),
                    }
                }
            }
            (None, _) => (None, "COMPLETED_BARS_MISSING_OR_INVALID", None),
            (_, None) => (None, "SESSION_DEADLINE_UNAVAILABLE", None),
            _ => (None, "SESSION_EXPIRED_OR_BAR_IDENTITY_CONFLICT", None),
        };
        strategy_assessments.insert(id.clone(), json!({"status":status,"detail":detail,"interval_seconds":bars.as_ref().map(|b|b.interval_seconds),"completed_bar_count":bars.as_ref().map(|b|b.bars.len())}));
        if let Some(s) = signal {
            let sid = format!("{cycle_id}_signal_{id}");
            tools.insert(sid, serde_json::to_value(&s).unwrap_or(Value::Null));
            signals.insert(id.clone(), s);
        } else {
            excluded.push("STRATEGY_SIGNAL_UNAVAILABLE".into());
        }
        if let Some(v) = observations.get(&c.quote_observation_id) {
            tools.insert(format!("market.quote:{id}"), v.clone());
        }
        if let Some(bars) = bars {
            tools.insert(
                format!("market.bars:{id}"),
                serde_json::to_value(bars).unwrap_or(Value::Null),
            );
        }
        ranks.push(RankedCandidate {
            instrument_id: c.instrument_id.clone(),
            currency: Currency::new("EUR")?,
            eligibility_snapshot_id: config.approved_universe_snapshot_id.clone(),
            eligible: c.research_eligible && excluded.is_empty(),
            ranking_score: rust_decimal::Decimal::ZERO,
            sentiment_score: None,
            confidence: None,
            evidence_ids: vec![c.quote_observation_id.clone()],
            exclusion_reasons: excluded,
            catalyst: None,
            contradictions: vec![],
            strategy_snapshot_ids: vec![],
        });
    }
    let mut run = SelectionRun {
        schema_version: 1,
        run_id,
        account_id: config.account_id.to_string(),
        session_id: cycle_id.into(),
        universe_snapshot_id: config.approved_universe_snapshot_id.clone(),
        started_at: now,
        completed_at: now,
        model: fingerprint,
        fallback_attempts: vec![],
        candidates: ranks,
        selected_instrument_id: None,
        proposal: None,
        evidence_snapshot_ids: candidates
            .iter()
            .map(|c| c.quote_observation_id.clone())
            .collect(),
        failure: None,
    };
    let eligible: Vec<_> = run
        .candidates
        .iter()
        .filter(|c| c.eligible)
        .map(|c| c.instrument_id.clone())
        .collect();
    let invoked = !eligible.is_empty();
    if invoked {
        let evidence_ids: Vec<_> = tools
            .keys()
            .filter(|k| !k.contains(':'))
            .cloned()
            .chain(run.evidence_snapshot_ids.clone())
            .collect();
        for c in candidates {
            if let Some(v) = observations.get(&c.quote_observation_id) {
                tools.insert(c.quote_observation_id.clone(), v.clone());
            }
        }
        let request = AnalystRequest {
            run: run.clone(),
            eligible_instrument_ids: eligible,
            candidate_evidence_ids: evidence_ids,
            fallback_attempts: vec![],
            as_of: now,
        };
        run = match select(&request, CapturedTools(tools)) {
            Ok(selection) => selection,
            Err(error) if error.0 == "EUR_RESEARCH_PENDING" => return Err(error),
            Err(_) => run.operational_failure(
                SelectionFailure::OperationalInputsUnavailable {
                    stage: SelectionFallbackStage::ReviewedUniverse,
                    code: "ANALYST_RESPONSE_UNAVAILABLE_OR_INVALID".into(),
                    message: "Pinned analyst did not return a valid scoped response".into(),
                },
                Utc::now(),
            )?,
        };
    } else {
        run=run.operational_failure(SelectionFailure::NoEligibleInstrument {message:"No candidate has current quotes, an open primary session and a deterministic strategy signal".into()},now)?;
    }
    run.validate(Utc::now())?;
    require(
        run.account_id == config.account_id.as_str()
            && run.session_id == cycle_id
            && run.universe_snapshot_id == config.approved_universe_snapshot_id,
        "analyst selection scope changed",
    )?;
    let mut plan = None;
    if let Some(id) = run.selected_instrument_id.as_ref() {
        let member = universe.and_then(|u| u.members.iter().find(|m| &m.instrument_id == id));
        if member.is_none() {
            gaps.push("REVIEWED_ORDER_RULES_UNAVAILABLE".to_owned());
        }
        if costs.is_none() {
            gaps.push("REVIEWED_EUR_COSTS_UNAVAILABLE".to_owned());
        }
        if let (Some(binding), Some(costs), Some(signal)) =
            (member, costs, signals.get(id.as_str()))
        {
            let fresh = refresh_quote(id);
            let (quote, quote_id) = match fresh {
                Ok(value) => value,
                Err(_) => {
                    return Ok(
                        json!({"selection":run,"analyst_invoked":invoked,"risk_plan":null,"risk_blockers":["POST_RESEARCH_IBKR_QUOTE_UNAVAILABLE"],"strategy_signals":signals,"strategy_assessments":strategy_assessments,"strategy_authority":"RESEARCH_BASELINE_ONLY","entry_authorized":false}),
                    );
                }
            };
            require(
                &quote.instrument_id == id,
                "refreshed quote instrument changed",
            )?;
            let at = Utc::now();
            require(
                costs.reviewed_at <= at
                    && at < costs.valid_until
                    && !costs.review_reference.is_empty(),
                "EUR cost review expired",
            )?;
            let rules = InstrumentRules {
                price_tick: binding.price_tick,
                quantity_step: binding.quantity_step,
                minimum_quantity: binding.entry_minimum_quantity(quote.ask)?,
                currency_unit: rust_decimal::Decimal::new(1, 2),
            };
            let mut evidence = run.evidence_snapshot_ids.clone();
            evidence.push(quote_id);
            let interval = observations
                .get(&format!("bars:{id}"))
                .and_then(|b| b["interval_seconds"].as_i64())
                .unwrap_or(0);
            if interval <= 0
                || signal.signal_at > at
                || (at - signal.signal_at).num_seconds() > interval * 2 + 15
            {
                gaps.push("COMPLETED_BAR_SIGNAL_EXPIRED_DURING_ANALYST".into());
            } else {
                match build_risk_plan(
                    account,
                    &quote,
                    signal,
                    &costs.entry,
                    &costs.exit,
                    &rules,
                    deadlines[id.as_str()],
                    &config.policy,
                    format!("{cycle_id}_plan"),
                    evidence,
                    at,
                ) {
                    Ok(value) => plan = Some(value),
                    Err(_) => {
                        gaps.push("RISK_PLAN_REJECTED_QUOTE_COST_STRATEGY_OR_DEADLINE".into())
                    }
                }
            }
        }
    }
    Ok(
        json!({"selection":run,"analyst_invoked":invoked,"risk_plan":plan,"risk_cost_model":costs,"risk_blockers":gaps,"strategy_signals":signals,"strategy_assessments":strategy_assessments,"strategy_authority":"RESEARCH_BASELINE_ONLY","entry_authorized":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use rust_decimal::Decimal;
    use trady_domain::{CompletedBar, Money, SelectionProposal};

    #[test]
    fn current_research_survivor_is_selected_and_risk_uses_a_post_model_quote() {
        let now = Utc::now();
        let config: EurRuntimeConfig =
            serde_json::from_str(include_str!("../../../config/eur-demo.example.json")).unwrap();
        let mut universe: EurUniverseSnapshot = serde_json::from_str(include_str!(
            "../../../config/eur_equity_universe.example.json"
        ))
        .unwrap();
        universe.snapshot_id = config.approved_universe_snapshot_id.clone();
        let id = universe.members[0].instrument_id.clone();
        let account = AccountSnapshot {
            snapshot_id: "fixture_account".into(),
            account_id: config.account_id.clone(),
            mode: trady_domain::Mode::Paper,
            spendable_cash: Money {
                amount: Decimal::from(5000),
                currency: Currency::new("EUR").unwrap(),
            },
            positions: vec![],
            orders: vec![],
            observed_at: now,
        };
        let quote = Quote {
            instrument_id: id.clone(),
            currency: Currency::new("EUR").unwrap(),
            bid: Decimal::new(10050, 2),
            ask: Decimal::new(10051, 2),
            observed_at: now,
            received_at: now,
            tradable: true,
        };
        let bars = BarSnapshot {
            snapshot_id: "fixture_bars".into(),
            instrument_id: id.clone(),
            provider: "yfinance_rs".into(),
            interval_seconds: 60,
            observed_at: now - Duration::minutes(1),
            received_at: now - Duration::seconds(1),
            bars: (0..22)
                .map(|i| {
                    let close = if i == 21 {
                        Decimal::new(10050, 2)
                    } else {
                        Decimal::from(100)
                    };
                    CompletedBar {
                        bar_id: format!("bar_{i}"),
                        instrument_id: id.clone(),
                        interval_seconds: 60,
                        opens_at: now - Duration::minutes(24 - i),
                        closes_at: now - Duration::minutes(23 - i),
                        open: close,
                        high: close + Decimal::new(1, 1),
                        low: close - Decimal::new(1, 1),
                        close,
                        volume: Some(Decimal::from(10000)),
                        provider: "yfinance_rs".into(),
                        received_at: now - Duration::seconds(1),
                        completed: true,
                    }
                })
                .collect(),
        };
        let c = ShadowCandidate {
            instrument_id: id.clone(),
            broker_ticker: "AIRp_EQ".into(),
            isin: "NL0000235190".into(),
            quote_observation_id: "fixture_quote".into(),
            research_eligible: true,
            entry_eligible: false,
            blockers: vec![
                "BROKER_EXECUTION_RULES_UNVERIFIED".into(),
                "QUOTE_TO_BROKER_COMPATIBILITY_UNACCEPTED".into(),
            ],
            assessed_at: Some(now),
        };
        let observations = BTreeMap::from([
            (
                format!("session:{id}"),
                json!({"opens_at":now-Duration::hours(1)}),
            ),
            (format!("bars:{id}"), serde_json::to_value(bars).unwrap()),
            (
                "fixture_quote".into(),
                json!({"instrument_id":id,"symbol":"IBKR:1","currency":"EUR","bid":"100.50","ask":"100.51","last":null,"market_state":"REGULAR","observed_at":now,"received_at":now}),
            ),
        ]);
        let zero = Money {
            amount: Decimal::ZERO,
            currency: Currency::new("EUR").unwrap(),
        };
        let costs = ReviewedCosts {
            review_reference: "fixture_review".into(),
            reviewed_at: now - Duration::hours(1),
            valid_until: now + Duration::hours(1),
            entry: EntryCosts {
                fixed: zero.clone(),
                rate: Decimal::new(5, 4),
                uncertainty: zero.clone(),
                fx: None,
            },
            exit: ExitCosts {
                fixed: zero.clone(),
                rate: Decimal::new(5, 4),
                uncertainty: zero,
                fx: None,
            },
        };
        let mut refreshed = false;
        let result = decide(
            &config,
            "fixture_cycle",
            &[c],
            &observations,
            &account,
            Some(&universe),
            Some(&costs),
            &BTreeMap::from([(id.to_string(), now + Duration::hours(1))]),
            OpenCodeGoConfig::default(),
            now,
            &mut |_| {
                refreshed = true;
                Ok((quote.clone(), "fixture_post_model_book".into()))
            },
            &mut |request, tools| {
                let s: SignalSnapshot =
                    serde_json::from_value(tools.0[&format!("fixture_cycle_signal_{id}")].clone())
                        .unwrap();
                let mut run = request.run.clone();
                run.candidates[0].ranking_score = Decimal::ONE;
                run.successful(
                    id.clone(),
                    SelectionProposal {
                        entry_minimum: s.entry_zone.minimum,
                        entry_maximum: s.entry_zone.maximum,
                        stop_price: s.invalidation,
                        target_price: s.expected_target,
                        maximum_hold_until: s.max_hold_deadline,
                        rationale: "fixture deterministic breakout".into(),
                    },
                    Utc::now(),
                )
            },
        )
        .unwrap();
        assert!(refreshed);
        assert_eq!(result["analyst_invoked"], true);
        assert_eq!(result["selection"]["selected_instrument_id"], id.as_str());
        assert!(!result["risk_plan"].is_null(), "{result}");
        assert!(
            result["risk_plan"]["evidence_snapshot_ids"]
                .as_array()
                .unwrap()
                .contains(&json!("fixture_post_model_book"))
        );
        assert_eq!(result["entry_authorized"], false);
    }
}
