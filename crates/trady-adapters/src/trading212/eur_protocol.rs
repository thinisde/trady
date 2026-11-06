//! Trading 212 transport for the durable EUR protocol. No ticker inference,
//! pending-absence inference, mutation retries or attached protection claims.
use super::*;
use trady_application::eur_protocol::{
    OrderLeg, ProtocolState,
    driver::{
        BrokerObservation, ExactOrderObservation, ProtectionConfirmation, ProtocolBroker,
        request_for,
    },
};
use trady_domain::{OrderStatus, Side};

pub struct Trading212ProtocolBroker<C: Clock = SystemClock> {
    pub executor: Trading212Executor<C>,
    pub binding: EurInstrumentBinding,
    pub(super) observed_sellable: Option<(String, Decimal, DateTime<Utc>)>,
    complete_account_view: Option<(DateTime<Utc>, Vec<String>)>,
    entry_authority: Option<(std::path::PathBuf, String, DateTime<Utc>)>,
}

fn authority_file_is_trusted(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return false;
        };
        if !metadata.is_file()
            || metadata.uid() != 0
            || metadata.len() > 65_536
            || !matches!(metadata.mode() & 0o777, 0o400 | 0o600 | 0o440)
        {
            return false;
        }
        path.is_absolute()
            && path.ancestors().skip(1).all(|parent| {
                std::fs::symlink_metadata(parent)
                    .is_ok_and(|m| m.is_dir() && m.uid() == 0 && m.mode() & 0o022 == 0)
            })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn observed_json<T: serde::de::DeserializeOwned, C: Clock>(
    reader: &mut Trading212Reader<C>,
    bucket: Bucket,
    path: &str,
    evidence: &mut Vec<serde_json::Value>,
) -> BrokerResult<T> {
    reader.http.pace(bucket)?;
    let bytes = reader.http.request(bucket, Method::GET, path, None)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let typed: T = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    evidence
        .push(serde_json::json!({"path":path,"received_at":reader.clock.now(),"response":value}));
    Ok(typed)
}

type ObservedFills = BTreeMap<String, BTreeMap<i64, (Decimal, DateTime<Utc>)>>;

/// Completeness concerns the current exact orders, rather than all trading
/// ever performed in the ticker. Every filled share must have a distinct
/// native execution; pagination can stop only once those totals agree.
fn exact_history_complete(
    state: &ProtocolState,
    pending: &[dto::Order],
    history: &BTreeMap<String, dto::Order>,
    fills: &ObservedFills,
) -> BrokerResult<bool> {
    for progress in state
        .entry
        .iter()
        .chain(state.protection.iter())
        .chain(state.exits.last())
    {
        let Some(id) = progress.broker_id.as_ref() else {
            if !progress.reconciled_terminal {
                return Ok(false);
            }
            continue;
        };
        let Some(wire) = pending
            .iter()
            .find(|p| p.id.to_string() == *id)
            .or_else(|| history.get(id))
        else {
            return Ok(false);
        };
        let order = wire.normalize()?;
        let total = fills
            .get(id)
            .into_iter()
            .flat_map(|rows| rows.values())
            .try_fold(Decimal::ZERO, |sum, (quantity, _)| {
                sum.checked_add(*quantity).ok_or_else(invalid)
            })?;
        if total != order.filled_quantity {
            return Ok(false);
        }
    }
    Ok(true)
}

impl<C: Clock> ProtocolBroker for Trading212ProtocolBroker<C> {
    fn now(&self) -> DateTime<Utc> {
        self.executor.reader.clock.now()
    }

    fn observe(&mut self, state: &ProtocolState) -> BrokerResult<BrokerObservation> {
        self.observed_sellable = None;
        self.complete_account_view = None;
        let r = &mut self.executor.reader;
        if state.context.runtime_mode != trady_domain::ExecutionStage::Demo
            || r.config.environment != Environment::Demo
            || r.config.broker_account_id as u64 != state.context.broker_account_id
            || r.config.account_id.as_str() != state.context.account_id
            || self.binding.instrument_id.as_str() != state.context.instrument_id
            || self.binding.broker_instrument_id != state.context.broker_ticker
            || self.binding.isin != state.context.isin
        {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        r.http.pace(Bucket::Summary)?;
        let read_started_at = r.clock.now();
        r.spendable_cash()?;
        let mut evidence = Vec::new();
        let positions: Vec<dto::PositionDto> = observed_json(
            r,
            Bucket::Positions,
            &format!("{API}/positions"),
            &mut evidence,
        )?;
        let positions_at = r.clock.now();
        let mut holding = Decimal::ZERO;
        let mut available = Decimal::ZERO;
        if positions.len() > 1 {
            return Err(invalid());
        }
        for p in positions {
            if p.instrument.ticker != state.context.broker_ticker
                || p.instrument.currency.as_str() != "EUR"
                || p.quantity.0 < Decimal::ZERO
            {
                return Err(invalid());
            }
            holding = p.quantity.0;
            available = p.available.ok_or_else(invalid)?.0;
        }
        let pending: Vec<dto::Order> =
            observed_json(r, Bucket::Orders, &format!("{API}/orders"), &mut evidence)?;
        let mut pending_ids = BTreeSet::new();
        for p in &pending {
            if !pending_ids.insert(p.normalize()?.broker_order_id) {
                return Err(invalid());
            }
        }
        let mut history = BTreeMap::new();
        let mut history_fills: ObservedFills = BTreeMap::new();
        let mut entry_first_filled_at = None;
        let mut cursor = None;
        let mut history_complete = false;
        // Reads stay bounded without requiring unrelated old ticker history.
        // Missing exact orders or incomplete execution totals still block.
        if state.entry.is_some() {
            for _ in 0..8 {
                let query = HistoryQuery {
                    limit: 50,
                    cursor,
                    instrument_id: Some(
                        InstrumentId::new(state.context.broker_ticker.clone())
                            .map_err(|_| invalid())?,
                    ),
                };
                let path = format!(
                    "{API}/history/orders?limit=50&ticker={}{}",
                    state.context.broker_ticker,
                    cursor.map_or(String::new(), |v| format!("&cursor={v}"))
                );
                let page: dto::History = observed_json(r, Bucket::History, &path, &mut evidence)?;
                if page.items.len() > 50 {
                    return Err(invalid());
                }
                for row in page.items {
                    let order = row.order.normalize()?;
                    if order.instrument_id.as_str() != state.context.broker_ticker {
                        return Err(invalid());
                    }
                    let relevant = state
                        .entry
                        .iter()
                        .chain(state.protection.iter())
                        .chain(state.exits.last())
                        .find(|p| p.broker_id.as_ref() == Some(&order.broker_order_id));
                    if let Some(progress) = relevant
                        && let Some(fill) = &row.fill
                    {
                        let id = fill.id.filter(|id| *id > 0).ok_or_else(invalid)?;
                        let quantity = fill.quantity.ok_or_else(invalid)?.0;
                        let filled_at = fill.filled_at.ok_or_else(invalid)?;
                        if !matches!(
                            (order.side, quantity.cmp(&Decimal::ZERO)),
                            (Side::Buy, std::cmp::Ordering::Greater)
                                | (Side::Sell, std::cmp::Ordering::Less)
                        ) // Native filledAt has whole-second precision; match the reducer.
                            || filled_at < progress.dispatched_at - chrono::Duration::seconds(1)
                            || filled_at > r.clock.now()
                        {
                            return Err(invalid());
                        }
                        // Native executions use signed quantities, whereas protocol
                        // progress and exact-order totals use positive magnitudes.
                        let quantity = quantity.abs();
                        if let Some(previous) = history_fills
                            .entry(order.broker_order_id.clone())
                            .or_default()
                            .insert(id, (quantity, filled_at))
                            && previous != (quantity, filled_at)
                        {
                            return Err(invalid());
                        }
                        if state.entry.as_ref().and_then(|p| p.broker_id.as_ref())
                            == Some(&order.broker_order_id)
                        {
                            entry_first_filled_at = Some(
                                entry_first_filled_at
                                    .map_or(filled_at, |old: DateTime<Utc>| old.min(filled_at)),
                            );
                        }
                    }
                    if let Some(old) = history.insert(order.broker_order_id.clone(), row.order)
                        && old.normalize()? != order
                    {
                        return Err(invalid());
                    }
                }
                cursor = page
                    .next_page_path
                    .map(|path| next_cursor(&path, &query))
                    .transpose()?;
                history_complete =
                    exact_history_complete(state, &pending, &history, &history_fills)?;
                if history_complete || cursor.is_none() {
                    break;
                }
            }
        } else {
            history_complete = true;
        }
        let mut exact_orders = Vec::new();
        for (leg, progress) in [
            (OrderLeg::Entry, state.entry.as_ref()),
            (OrderLeg::Protection, state.protection.as_ref()),
            (OrderLeg::Exit, state.exits.last()),
        ] {
            let Some(id) = progress.and_then(|p| p.broker_id.as_ref()) else {
                continue;
            };
            let pending_order = pending.iter().find(|p| p.id.to_string() == *id);
            let history_order = history.get(id);
            let Some(wire) = pending_order.or(history_order) else {
                history_complete = false;
                continue;
            };
            let normalized = wire.normalize()?;
            if leg == OrderLeg::Entry
                && normalized.filled_quantity > Decimal::ZERO
                && entry_first_filled_at.is_none()
            {
                return Err(invalid());
            }
            let request = request_for(state, leg).map_err(|_| invalid())?;
            let matches = if pending_order.is_some() {
                wire.matches(&request, &normalized)
            } else {
                wire.matches_history(&request, &normalized, false)
            };
            if !matches {
                return Err(invalid());
            }
            if let Some(old) = history_order {
                let old_order = old.normalize()?;
                if !old.matches_history(&request, &old_order, pending_order.is_some())
                    || (old_order.status.terminal() && pending_order.is_some())
                {
                    return Err(invalid());
                }
            }
            exact_orders.push(ExactOrderObservation {
                leg,
                order: normalized,
                stop_price: wire.stop_price_value(),
                pending_present: pending_order.is_some(),
            });
        }
        if (r.clock.now() - read_started_at).num_milliseconds() > 10000 {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidResponse));
        }
        self.observed_sellable =
            Some((state.context.broker_ticker.clone(), available, positions_at));
        if history_complete {
            self.complete_account_view =
                Some((r.clock.now(), pending_ids.iter().cloned().collect()));
        }
        Ok(BrokerObservation {
            holding_quantity: holding,
            available_quantity: available,
            pending_order_ids: pending_ids.into_iter().collect(),
            exact_orders,
            history_complete,
            history_coverage_exhausted: !history_complete,
            observed_at: r.clock.now(),
            entry_first_filled_at,
            entry_confirmation_from_full_view_at: None,
            evidence: serde_json::json!({"provider":"TRADING212_DEMO","broker_account_id":state.context.broker_account_id,"history_scope":"CURRENT_EXACT_ORDERS_WITH_COMPLETE_FILL_TOTALS","responses":evidence}),
        })
    }

    fn confirm_entry(&mut self, state: &ProtocolState) -> BrokerResult<Option<BrokerObservation>> {
        let Some((prior, pending)) = self.complete_account_view.clone() else {
            return Ok(None);
        };
        let Some(entry) = state.entry.as_ref() else {
            return Ok(None);
        };
        let Some(id) = entry.broker_id.as_ref() else {
            return Ok(None);
        };
        if state.reconciliation_required
            || state.exit_requested
            || state.protection.is_some()
            || !state.exits.is_empty()
            || entry.reconciled_terminal
            || pending.iter().any(|p| p != id)
            || self.now() < prior
            || (self.now() - prior).num_milliseconds() > 10000
        {
            return Ok(None);
        }
        self.fresh_binding(
            &AccountId::new(state.context.account_id.clone()).map_err(|_| invalid())?,
        )?;
        let request = request_for(state, OrderLeg::Entry).map_err(|_| invalid())?;
        let r = &mut self.executor.reader;
        let mut evidence = Vec::new();
        for _ in 0..3 {
            let path = format!("{API}/orders/{}", numeric_id(id)?);
            let lookup: BrokerResult<dto::Order> =
                observed_json(r, Bucket::Order, &path, &mut evidence);
            let mut exact = match lookup {
                Ok(wire) => {
                    let order = wire.normalize()?;
                    if !wire.matches(&request, &order) || order.broker_order_id != *id {
                        return Err(invalid());
                    }
                    Some(order)
                }
                Err(error) if error.http_status == Some(404) => {
                    evidence.push(serde_json::json!({"path":path,"received_at":r.clock.now(),"http_status":404}));
                    None
                }
                Err(error) => return Err(error),
            };
            let pending_present = exact.is_some();
            let mut fills = BTreeMap::new();
            let mut first_fill = None;
            if exact
                .as_ref()
                .is_none_or(|o| o.filled_quantity > Decimal::ZERO)
            {
                let mut cursor = None;
                for _ in 0..8 {
                    let query = HistoryQuery {
                        limit: 50,
                        cursor,
                        instrument_id: Some(
                            InstrumentId::new(state.context.broker_ticker.clone())
                                .map_err(|_| invalid())?,
                        ),
                    };
                    let path = format!(
                        "{API}/history/orders?limit=50&ticker={}{}",
                        state.context.broker_ticker,
                        cursor.map_or(String::new(), |c| format!("&cursor={c}"))
                    );
                    let page: dto::History =
                        observed_json(r, Bucket::History, &path, &mut evidence)?;
                    for row in page.items {
                        if row.order.id.to_string() != *id {
                            continue;
                        }
                        let order = row.order.normalize()?;
                        if !row.order.matches_history(&request, &order, pending_present) {
                            return Err(invalid());
                        }
                        if exact.is_none() {
                            exact = Some(order);
                        }
                        if let Some(fill) = row.fill {
                            let fid = fill.id.ok_or_else(invalid)?;
                            let quantity = fill.quantity.ok_or_else(invalid)?.0;
                            let at = fill.filled_at.ok_or_else(invalid)?;
                            if quantity <= Decimal::ZERO
                                || fills
                                    .insert(fid, quantity)
                                    .is_some_and(|old| old != quantity)
                            {
                                return Err(invalid());
                            }
                            first_fill =
                                Some(first_fill.map_or(at, |old: DateTime<Utc>| old.min(at)));
                        }
                    }
                    if exact.as_ref().is_some_and(|o| {
                        fills.values().copied().sum::<Decimal>() == o.filled_quantity
                    }) {
                        break;
                    }
                    cursor = page
                        .next_page_path
                        .map(|path| next_cursor(&path, &query))
                        .transpose()?;
                    if cursor.is_none() {
                        break;
                    }
                }
            }
            let Some(order) = exact else {
                return Ok(None);
            };
            if order.filled_quantity != fills.values().copied().sum::<Decimal>()
                || (!pending_present && !order.status.terminal())
            {
                return Ok(None);
            }
            let positions: Vec<dto::PositionDto> = observed_json(
                r,
                Bucket::Positions,
                &format!("{API}/positions"),
                &mut evidence,
            )?;
            let (holding, available) = match positions.as_slice() {
                [] => (Decimal::ZERO, Decimal::ZERO),
                [position]
                    if position.instrument.ticker == state.context.broker_ticker
                        && position.instrument.currency.as_str() == "EUR" =>
                {
                    (
                        position.quantity.0,
                        position.available.as_ref().ok_or_else(invalid)?.0,
                    )
                }
                _ => return Err(invalid()),
            };
            if (r.clock.now() - prior).num_milliseconds() > 10000 {
                return Ok(None);
            }
            if holding != order.filled_quantity || available != holding {
                continue;
            }
            self.observed_sellable = Some((
                state.context.broker_ticker.clone(),
                available,
                r.clock.now(),
            ));
            return Ok(Some(BrokerObservation {
                holding_quantity: holding,
                available_quantity: available,
                pending_order_ids: if pending_present {
                    vec![id.clone()]
                } else {
                    vec![]
                },
                history_complete: order.status.terminal() && !pending_present,
                history_coverage_exhausted: false,
                exact_orders: vec![ExactOrderObservation {
                    leg: OrderLeg::Entry,
                    order,
                    stop_price: None,
                    pending_present,
                }],
                observed_at: r.clock.now(),
                entry_first_filled_at: first_fill,
                entry_confirmation_from_full_view_at: Some(prior),
                evidence: serde_json::json!({"provider":"TRADING212_DEMO","scope":"EXACT_ENTRY_CONFIRMATION","inherited_complete_account_view_at":prior,"responses":evidence}),
            }));
        }
        Ok(None)
    }

    fn confirm_protection(
        &mut self,
        state: &ProtocolState,
    ) -> BrokerResult<Option<ProtectionConfirmation>> {
        let account = AccountId::new(state.context.account_id.clone()).map_err(|_| invalid())?;
        self.fresh_binding(&account)?;
        let request = request_for(state, OrderLeg::Protection).map_err(|_| invalid())?;
        let id = state
            .protection
            .as_ref()
            .and_then(|p| p.broker_id.as_ref())
            .ok_or_else(invalid)?;
        let id = numeric_id(id)?;
        let r = &mut self.executor.reader;
        let mut evidence = Vec::new();
        let wire: dto::Order = observed_json(
            r,
            Bucket::Order,
            &format!("{API}/orders/{id}"),
            &mut evidence,
        )?;
        let order = wire.normalize()?;
        if order.broker_order_id != id.to_string()
            || !wire.matches(&request, &order)
            || !matches!(
                order.status,
                OrderStatus::Open | OrderStatus::PartiallyFilled
            )
        {
            return Err(invalid());
        }
        let positions: Vec<dto::PositionDto> = observed_json(
            r,
            Bucket::Positions,
            &format!("{API}/positions"),
            &mut evidence,
        )?;
        if positions.len() != 1
            || positions[0].instrument.ticker != state.context.broker_ticker
            || positions[0].instrument.currency.as_str() != "EUR"
            || positions[0].quantity.0 != state.remaining() - order.filled_quantity
            || order.quantity - order.filled_quantity != positions[0].quantity.0
            || positions[0]
                .available
                .as_ref()
                .is_none_or(|q| q.0 != Decimal::ZERO)
        {
            return Err(invalid());
        }
        Ok(Some(ProtectionConfirmation {
            order,
            stop_price: wire.stop_price_value().ok_or_else(invalid)?,
            holding_quantity: positions[0].quantity.0,
            observed_at: r.clock.now(),
            evidence: serde_json::json!({"provider":"TRADING212_DEMO","kind":"EXACT_STOP_AND_HOLDING_CONFIRMATION","responses":evidence}),
        }))
    }

    fn submit(
        &mut self,
        request: &BrokerOrderRequest,
        expires_at: DateTime<Utc>,
    ) -> BrokerResult<BrokerOrder> {
        self.fresh_binding(&request.account_id)?;
        let recent_sellable = self.observed_sellable.take();
        if request.side == trady_domain::Side::Sell {
            let r = &mut self.executor.reader;
            let fresh = recent_sellable.filter(|(ticker, _, at)| {
                ticker == request.instrument_id.as_str()
                    && r.clock.now() >= *at
                    && (r.clock.now() - *at).num_milliseconds() <= 1000
            });
            let available = if let Some((_, quantity, _)) = fresh {
                quantity
            } else {
                r.http.pace(Bucket::Positions)?;
                let positions: Vec<dto::PositionDto> = r.http.json(
                    Bucket::Positions,
                    Method::GET,
                    &format!("{API}/positions"),
                    None,
                )?;
                if positions.len() != 1
                    || positions[0].instrument.ticker != request.instrument_id.as_str()
                    || positions[0].instrument.currency != request.instrument_currency
                {
                    return Err(invalid());
                }
                positions[0].available.as_ref().ok_or_else(invalid)?.0
            };
            if available < request.quantity {
                return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
            }
        }
        let authority = self.entry_authority.clone();
        let buy = request.side == trady_domain::Side::Buy;
        let expires_at = if buy {
            authority
                .as_ref()
                .map_or(expires_at, |(_, _, until)| expires_at.min(*until))
        } else {
            expires_at
        };
        self.executor
            .submit_eur_bound_guarded(request, &self.binding, Some(expires_at), || {
                if buy && let Some((path, expected, _)) = &authority {
                    let current = std::fs::read_to_string(path)
                        .ok()
                        .map(|text| trady_application::harness::content_hash(&text));
                    if !authority_file_is_trusted(path)
                        || current.as_deref() != Some(expected.as_str())
                    {
                        return Err(invalid());
                    }
                }
                Ok(())
            })
    }

    fn prepare_cancel(&mut self, state: &ProtocolState, leg: OrderLeg) -> BrokerResult<()> {
        if leg == OrderLeg::Protection {
            let account =
                AccountId::new(state.context.account_id.clone()).map_err(|_| invalid())?;
            self.fresh_binding(&account)?;
            let instruments = self.executor.information().liquidation_catalog()?;
            if !instruments.iter().any(|i| {
                i.instrument_id.as_str() == self.binding.broker_instrument_id
                    && i.isin == self.binding.isin
                    && i.currency == self.binding.currency
                    && i.kind == "STOCK"
            }) {
                return Err(invalid());
            }
        }
        Ok(())
    }

    fn cancel(
        &mut self,
        account: &AccountId,
        broker_id: &str,
    ) -> BrokerResult<CancellationReceipt> {
        self.observed_sellable = None;
        self.fresh_binding(account)?;
        let id = numeric_id(broker_id)?;
        let r = &mut self.executor.reader;
        r.http.pace(Bucket::Cancel)?;
        r.http
            .request(
                Bucket::Cancel,
                Method::DELETE,
                &format!("{API}/orders/{id}"),
                None,
            )
            .map_err(|mut e| {
                e.broker_order_id = Some(id.to_string());
                e
            })?;
        Ok(CancellationReceipt {
            broker_order_id: id.to_string(),
        })
    }
}

impl<C: Clock> Trading212ProtocolBroker<C> {
    pub fn new(executor: Trading212Executor<C>, binding: EurInstrumentBinding) -> Self {
        Self {
            executor,
            binding,
            observed_sellable: None,
            complete_account_view: None,
            entry_authority: None,
        }
    }
    pub fn require_entry_authority(
        &mut self,
        path: std::path::PathBuf,
        sha256: String,
        until: DateTime<Utc>,
    ) {
        self.entry_authority = Some((path, sha256, until));
    }
    fn fresh_binding(&mut self, account: &AccountId) -> BrokerResult<()> {
        let r = &mut self.executor.reader;
        r.check_account(account)?;
        if r.config.environment != Environment::Demo || r.config.broker_account_id != 52211463 {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        // A just-reconciled broker-owned identity remains valid for this one
        // dispatch. Re-reading its five-second bucket prevents timely stops.
        if r.bound_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(10))
        {
            r.http.pace(Bucket::Summary)?;
            r.spendable_cash()?;
        }
        Ok(())
    }
}
