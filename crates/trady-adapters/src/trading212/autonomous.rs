//! Pending and terminal observations for the durable executor; correlation uses exact IDs.

use super::*;
use trady_application::autonomous::{ExecutionBroker, ExecutionObservation, TrackedOrder};
use trady_domain::{Money, require};

/// Runtime pacing is separate from low-level nonblocking transport. Every POST
/// still has exactly one send and every mutation checks actual account identity.
pub struct AutonomousTrading212 {
    executor: Trading212Executor,
    binding_consumed: bool,
}
impl AutonomousTrading212 {
    pub fn new(config: Trading212Config, credentials: Credentials) -> BrokerResult<Self> {
        Ok(Self {
            executor: Trading212Executor::new(config, credentials)?,
            binding_consumed: true,
        })
    }
}
impl BrokerExecutionPort for AutonomousTrading212 {
    fn submit_order(&mut self, request: &BrokerOrderRequest) -> BrokerResult<BrokerOrder> {
        let bucket = match request.kind {
            OrderKind::Market { .. } => Bucket::Market,
            OrderKind::Limit { .. } => Bucket::Limit,
            OrderKind::Stop { .. } => Bucket::Stop,
            OrderKind::StopLimit { .. } => Bucket::StopLimit,
        };
        self.executor.reader.http.pace(bucket)?;
        // The final account observation is one fresh, single-use binding. Reusing it
        // avoids another five-second summary wait after the final quote/ML checks.
        if self.binding_consumed
            || self
                .executor
                .reader
                .bound_at
                .is_none_or(|at| at.elapsed() > Duration::from_secs(5))
        {
            self.executor.reader.http.pace(Bucket::Summary)?;
            self.executor.reader.spendable_cash()?;
        }
        self.binding_consumed = true;
        self.executor.submit_bound(request)
    }
    fn cancel_order(&mut self, account: &AccountId, id: &str) -> BrokerResult<CancellationReceipt> {
        self.binding_consumed = true;
        self.executor.reader.http.pace(Bucket::Summary)?;
        self.executor.reader.http.pace(Bucket::Cancel)?;
        self.executor.cancel_order(account, id)
    }
}
impl ExecutionBroker for AutonomousTrading212 {
    fn submit_order_before(
        &mut self,
        request: &BrokerOrderRequest,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> BrokerResult<BrokerOrder> {
        let bucket = match request.kind {
            OrderKind::Market { .. } => Bucket::Market,
            OrderKind::Limit { .. } => Bucket::Limit,
            OrderKind::Stop { .. } => Bucket::Stop,
            OrderKind::StopLimit { .. } => Bucket::StopLimit,
        };
        self.executor.reader.http.pace(bucket)?;
        if self.binding_consumed
            || self
                .executor
                .reader
                .bound_at
                .is_none_or(|at| at.elapsed() > Duration::from_secs(5))
        {
            self.executor.reader.http.pace(Bucket::Summary)?;
            self.executor.reader.spendable_cash()?;
        }
        self.binding_consumed = true;
        self.executor.submit_bound_before(request, Some(expires_at))
    }
    fn observe(
        &mut self,
        account: &AccountId,
        tracked: &[TrackedOrder],
    ) -> Result<ExecutionObservation> {
        self.binding_consumed = true;
        let observation = self.executor.observe(account, tracked)?;
        self.binding_consumed = false;
        Ok(observation)
    }
}

impl<C: Clock> ExecutionBroker for Trading212Executor<C> {
    fn submit_order_before(
        &mut self,
        request: &BrokerOrderRequest,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> BrokerResult<BrokerOrder> {
        self.reader.spendable_cash()?;
        self.submit_bound_before(request, Some(expires_at))
    }
    fn observe(
        &mut self,
        account: &AccountId,
        tracked: &[TrackedOrder],
    ) -> Result<ExecutionObservation> {
        self.observe_execution(account, tracked)
            .map_err(|error| Error(error.to_string()))
    }
}

impl<C: Clock> Trading212Executor<C> {
    fn observe_execution(
        &mut self,
        account: &AccountId,
        tracked: &[TrackedOrder],
    ) -> BrokerResult<ExecutionObservation> {
        let reader = &mut self.reader;
        reader.check_account(account)?;
        reader.http.pace(Bucket::Summary)?;
        let observed_at = reader.clock.now();
        let summary: AccountSummary = reader.http.json(
            Bucket::Summary,
            Method::GET,
            &format!("{API}/account/summary"),
            None,
        )?;
        if summary.id != reader.config.broker_account_id
            || summary.currency != reader.config.base_currency
            || summary.cash.available_to_trade.0 < rust_decimal::Decimal::ZERO
        {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        reader.bound_at = Some(Instant::now());
        let equity = summary.total_value.ok_or_else(invalid)?.0;
        if equity < rust_decimal::Decimal::ZERO {
            return Err(invalid());
        }
        reader.http.pace(Bucket::Positions)?;
        let positions: Vec<PositionDto> = reader.http.json(
            Bucket::Positions,
            Method::GET,
            &format!("{API}/positions"),
            None,
        )?;
        let mut costs = BTreeMap::new();
        let mut available = BTreeMap::new();
        let mut normalized = Vec::new();
        for position in positions {
            let id = position.instrument.ticker.clone();
            let quantity = position.quantity.0;
            let free = position.available.ok_or_else(invalid)?.0;
            if free < rust_decimal::Decimal::ZERO || free > quantity {
                return Err(invalid());
            }
            let wallet = position.wallet_impact.as_ref().ok_or_else(invalid)?;
            if wallet.currency != reader.config.base_currency
                || wallet.total_cost.0 <= rust_decimal::Decimal::ZERO
            {
                return Err(invalid());
            }
            if costs
                .insert(
                    id.clone(),
                    Money {
                        amount: wallet.total_cost.0,
                        currency: wallet.currency.clone(),
                    },
                )
                .is_some()
            {
                return Err(invalid());
            }
            available.insert(id, free);
            normalized.push(position.normalize()?);
        }
        reader.http.pace(Bucket::Orders)?;
        let mut orders: BTreeMap<_, _> = reader
            .pending_orders()?
            .into_iter()
            .map(|o| (o.broker_order_id.clone(), o))
            .collect();
        let missing: BTreeSet<_> = tracked
            .iter()
            .filter(|o| {
                o.observation
                    .as_ref()
                    .is_some_and(|row| !row.status.terminal())
            })
            .filter_map(|o| o.broker_id())
            .filter(|id| !orders.contains_key(*id))
            .map(str::to_owned)
            .collect();
        if !missing.is_empty() {
            let mut remaining = missing;
            let mut cursor = None;
            let mut seen = BTreeSet::new();
            // Bound recovery work. Incomplete history leaves the ID absent, which the application
            // treats as recovery-required; it never guesses from ticker/quantity/time.
            for _ in 0..10 {
                reader.http.pace(Bucket::History)?;
                let page = reader.historical_orders(&HistoryQuery {
                    instrument_id: None,
                    cursor,
                    limit: 50,
                })?;
                for order in page.orders {
                    if remaining.remove(&order.broker_order_id) {
                        orders.insert(order.broker_order_id.clone(), order);
                    }
                }
                if remaining.is_empty() {
                    break;
                }
                let Some(next) = page.next_cursor else {
                    break;
                };
                if !seen.insert(next) {
                    return Err(invalid());
                }
                cursor = Some(next);
            }
        }
        let elapsed = reader.clock.now() - observed_at;
        if elapsed < TimeDelta::zero() || elapsed > TimeDelta::seconds(60) {
            return Err(invalid());
        }
        let snapshot = AccountSnapshot {
            snapshot_id: format!("t212_execution_{}", uuid_free_id(observed_at)),
            account_id: account.clone(),
            mode: reader.config.environment.mode(),
            spendable_cash: Money {
                amount: summary.cash.available_to_trade.0,
                currency: summary.currency.clone(),
            },
            positions: normalized,
            orders: orders.into_values().collect(),
            observed_at,
        };
        snapshot.validate().map_err(|_| invalid())?;
        require(
            snapshot.positions.len() == costs.len(),
            "position accounting mismatch",
        )
        .map_err(|_| invalid())?;
        Ok(ExecutionObservation {
            snapshot,
            position_costs: costs,
            available_quantities: available,
            total_equity: Money {
                amount: equity,
                currency: summary.currency,
            },
        })
    }
}

fn uuid_free_id(now: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "{}_{}_{}",
        std::process::id(),
        now.timestamp_micros(),
        SNAPSHOT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
