//! Fixtures, scoped model/skill adapters and explicitly configured provider clients.

pub mod api_health;
pub mod autonomous;
pub mod credentials;
pub mod financial;
pub mod finbert;
pub mod inputs_service;
pub mod intelligence;
pub mod news;
pub mod perpetual;
pub mod research_jobs;
pub mod screening;
pub mod telegram;
pub mod trading212;

pub mod opencode;
pub mod sessions;
pub mod shadow;
pub mod weekly_lab;

use chrono::{DateTime, Duration, Utc};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use trady_application::{
    BrokerInformationPort, BrokerPort, Clock, ExchangeCalendar, ExchangeSession,
    ExecutionRepository, QuotePort,
};
use trady_domain::*;

#[derive(Clone)]
pub struct FakeClock(Arc<Mutex<DateTime<Utc>>>);
impl FakeClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }
    pub fn advance(&self, seconds: i64) -> Result<()> {
        let duration =
            Duration::try_seconds(seconds).ok_or_else(|| Error("fake duration overflow".into()))?;
        let mut now = self
            .0
            .lock()
            .map_err(|_| Error("fake clock lock poisoned".into()))?;
        *now = now
            .checked_add_signed(duration)
            .ok_or_else(|| Error("fake time overflow".into()))?;
        Ok(())
    }
}
impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Default)]
pub struct FakeCalendar {
    pub sessions: Vec<ExchangeSession>,
}
impl ExchangeCalendar for FakeCalendar {
    fn session(&self, now: DateTime<Utc>) -> Option<ExchangeSession> {
        self.sessions
            .iter()
            .find(|s| now >= s.opens_at && now < s.closes_at)
            .cloned()
    }
}

pub struct FakeBroker {
    snapshot: AccountSnapshot,
}
impl FakeBroker {
    pub fn new(snapshot: AccountSnapshot) -> Result<Self> {
        snapshot.validate()?;
        require(
            snapshot.mode == Mode::Paper,
            "fake broker accepts paper snapshots only",
        )?;
        Ok(Self { snapshot })
    }
    pub fn set_snapshot(&mut self, snapshot: AccountSnapshot) -> Result<()> {
        snapshot.validate()?;
        require(
            snapshot.mode == Mode::Paper && snapshot.account_id == self.snapshot.account_id,
            "fake account cannot change identity or mode",
        )?;
        self.snapshot = snapshot;
        Ok(())
    }
}
impl BrokerInformationPort for FakeBroker {
    fn snapshot(&mut self, account: &AccountId) -> Result<AccountSnapshot> {
        require(account == &self.snapshot.account_id, "unknown fake account")?;
        Ok(self.snapshot.clone())
    }
}
impl BrokerPort for FakeBroker {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn find_order(&mut self, intent: &IntentId) -> Result<Option<BrokerOrder>> {
        let matches: Vec<_> = self
            .snapshot
            .orders
            .iter()
            .filter(|o| o.client_intent_id.as_ref() == Some(intent))
            .cloned()
            .collect();
        require(matches.len() <= 1, "ambiguous fake order")?;
        Ok(matches.into_iter().next())
    }
}

pub struct FakeQuotes {
    pub quotes: BTreeMap<InstrumentId, Quote>,
}
impl QuotePort for FakeQuotes {
    fn quote(&mut self, instrument: &InstrumentId) -> Result<Quote> {
        self.quotes
            .get(instrument)
            .cloned()
            .ok_or_else(|| Error("fake quote unavailable".into()))
    }
}

#[derive(Clone)]
pub struct MemoryExecutionRepository {
    clock: FakeClock,
    accounts: Arc<Mutex<BTreeMap<AccountId, MemoryAccount>>>,
}
#[derive(Default)]
struct MemoryAccount {
    fence: Option<Fence>,
    expires: Option<DateTime<Utc>>,
    recovered_generation: Option<i64>,
    reconciliation: Option<Reconciliation>,
    snapshots: BTreeMap<String, AccountSnapshot>,
    intents: BTreeMap<IntentId, OrderIntent>,
    outbox: BTreeMap<String, PositionState>,
}
impl MemoryExecutionRepository {
    pub fn new(clock: FakeClock) -> Self {
        Self {
            clock,
            accounts: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
    pub fn register_paper_account(&mut self, account: &AccountId) -> Result<()> {
        require(
            account.as_str().starts_with("paper_"),
            "only paper accounts can use memory execution",
        )?;
        self.accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?
            .entry(account.clone())
            .or_default();
        Ok(())
    }
    pub fn outbox_count(&self, account: &AccountId) -> Result<usize> {
        Ok(self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?
            .get(account)
            .ok_or_else(|| Error("unknown account".into()))?
            .outbox
            .len())
    }
}
fn check(account: &MemoryAccount, fence: &Fence, now: DateTime<Utc>) -> Result<()> {
    require(
        account.fence.as_ref() == Some(fence) && account.expires.is_some_and(|t| t > now),
        "executor lease lost or fenced",
    )
}
fn ttl(seconds: i32) -> Result<Duration> {
    require(
        (1..=300).contains(&seconds),
        "lease TTL must be 1 through 300 seconds",
    )?;
    Ok(Duration::seconds(i64::from(seconds)))
}
impl ExecutionRepository for MemoryExecutionRepository {
    fn acquire(
        &mut self,
        account: &AccountId,
        worker: &WorkerId,
        ttl_seconds: i32,
    ) -> Result<Fence> {
        let duration = ttl(ttl_seconds)?;
        let now = self.clock.now();
        let expires = now
            .checked_add_signed(duration)
            .ok_or_else(|| Error("lease timestamp overflow".into()))?;
        let mut accounts = self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?;
        let a = accounts
            .get_mut(account)
            .ok_or_else(|| Error("unregistered account".into()))?;
        require(
            a.expires.is_none_or(|expiry| expiry <= now),
            "executor lease is busy",
        )?;
        let generation = match &a.fence {
            Some(f) => f
                .generation
                .checked_add(1)
                .ok_or_else(|| Error("fence overflow".into()))?,
            None => 1,
        };
        let fence = Fence {
            account_id: account.clone(),
            worker_id: worker.clone(),
            generation,
        };
        a.fence = Some(fence.clone());
        a.expires = Some(expires);
        a.recovered_generation = None;
        Ok(fence)
    }
    fn renew(&mut self, fence: &Fence, ttl_seconds: i32) -> Result<()> {
        let duration = ttl(ttl_seconds)?;
        let now = self.clock.now();
        let expires = now
            .checked_add_signed(duration)
            .ok_or_else(|| Error("lease timestamp overflow".into()))?;
        let mut accounts = self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?;
        let a = accounts
            .get_mut(&fence.account_id)
            .ok_or_else(|| Error("unknown account".into()))?;
        check(a, fence, now)?;
        a.expires = Some(expires);
        Ok(())
    }
    fn unresolved_intents(&mut self, account: &AccountId) -> Result<Vec<OrderIntent>> {
        let accounts = self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?;
        Ok(accounts
            .get(account)
            .ok_or_else(|| Error("unknown account".into()))?
            .intents
            .values()
            .filter(|i| i.state != IntentState::Resolved)
            .cloned()
            .collect())
    }
    fn save_reconciliation(&mut self, fence: &Fence, r: &Reconciliation) -> Result<()> {
        r.snapshot.validate()?;
        require(
            r.snapshot.account_id == fence.account_id && r.snapshot.mode == Mode::Paper,
            "wrong account or mode",
        )?;
        require(
            trady_policy_fresh(r.snapshot.observed_at, self.clock.now()),
            "stale or future reconciliation",
        )?;
        let mut accounts = self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?;
        let a = accounts
            .get_mut(&fence.account_id)
            .ok_or_else(|| Error("unknown account".into()))?;
        check(a, fence, self.clock.now())?;
        if let Some(previous) = &a.reconciliation {
            require(
                r.snapshot.observed_at >= previous.snapshot.observed_at,
                "snapshot time regression",
            )?;
            require(
                r.snapshot.spendable_cash.currency == previous.snapshot.spendable_cash.currency,
                "account currency changed",
            )?;
        }
        if let Some(old) = a.snapshots.get(&r.snapshot.snapshot_id) {
            require(
                old == &r.snapshot,
                "snapshot ID reused with different content",
            )?;
        }
        require(
            r.resolutions
                .iter()
                .all(|resolution| a.intents.contains_key(&resolution.intent_id)),
            "resolution references unknown intent",
        )?;
        for resolution in &r.resolutions {
            let i = a
                .intents
                .get_mut(&resolution.intent_id)
                .ok_or_else(|| Error("resolution references unknown intent".into()))?;
            if i.state != IntentState::Resolved {
                i.state = resolution.state;
                i.broker_order_id = resolution.broker_order_id.clone();
            }
        }
        a.outbox
            .entry(r.snapshot.snapshot_id.clone())
            .or_insert(r.state);
        a.reconciliation = Some(r.clone());
        a.snapshots
            .insert(r.snapshot.snapshot_id.clone(), r.snapshot.clone());
        a.recovered_generation = Some(fence.generation);
        Ok(())
    }
    fn latest_reconciliation(&mut self, account: &AccountId) -> Result<Option<Reconciliation>> {
        Ok(self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?
            .get(account)
            .ok_or_else(|| Error("unknown account".into()))?
            .reconciliation
            .clone())
    }
    fn reserve_paper_intent(&mut self, fence: &Fence, intent: &OrderIntent) -> Result<bool> {
        intent.validate()?;
        require(
            intent.account_id == fence.account_id
                && intent.side == Side::Buy
                && intent.state == IntentState::Prepared
                && intent.broker_order_id.is_none(),
            "only new paper entry fixtures can be reserved",
        )?;
        let mut accounts = self
            .accounts
            .lock()
            .map_err(|_| Error("memory repository lock poisoned".into()))?;
        let a = accounts
            .get_mut(&fence.account_id)
            .ok_or_else(|| Error("unknown account".into()))?;
        check(a, fence, self.clock.now())?;
        if let Some(old) = a.intents.get(&intent.intent_id) {
            require(
                old.account_id == intent.account_id
                    && old.instrument_id == intent.instrument_id
                    && old.quantity == intent.quantity
                    && old.limit_price == intent.limit_price
                    && old.session_id == intent.session_id
                    && old.created_at == intent.created_at,
                "idempotency key reused for another intent",
            )?;
            return Ok(false);
        }
        require(
            a.recovered_generation == Some(fence.generation),
            "takeover requires reconciliation",
        )?;
        require(
            a.reconciliation.as_ref().is_some_and(|r| {
                r.state == PositionState::Flat
                    && r.snapshot.positions.is_empty()
                    && trady_policy_fresh(r.snapshot.observed_at, self.clock.now())
            }),
            "paper account must be freshly reconciled and flat",
        )?;
        require(
            a.intents
                .values()
                .all(|i| i.session_id != intent.session_id && i.state == IntentState::Resolved),
            "entry slot consumed or intent unresolved",
        )?;
        a.intents.insert(intent.intent_id.clone(), intent.clone());
        a.recovered_generation = None;
        Ok(true)
    }
}
fn trady_policy_fresh(at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    at <= now && now.signed_duration_since(at) <= Duration::seconds(60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ord_08_expiry_fences_old_worker_and_requires_takeover_reconciliation() {
        let now = DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
            .unwrap()
            .to_utc();
        let clock = FakeClock::new(now);
        let mut repo = MemoryExecutionRepository::new(clock.clone());
        let account = AccountId::new("paper_test").unwrap();
        repo.register_paper_account(&account).unwrap();
        let first = repo
            .acquire(&account, &WorkerId::new("first").unwrap(), 1)
            .unwrap();
        assert!(
            repo.acquire(&account, &WorkerId::new("second").unwrap(), 1)
                .is_err()
        );
        clock.advance(2).unwrap();
        let second = repo
            .acquire(&account, &WorkerId::new("second").unwrap(), 1)
            .unwrap();
        assert!(second.generation > first.generation);
        assert!(repo.renew(&first, 1).is_err());
    }
    #[test]
    fn holidays_are_absent_rather_than_inferred_weekdays() {
        let calendar = FakeCalendar::default();
        assert!(calendar.session(Utc::now()).is_none());
    }
    #[test]
    fn extreme_fake_times_and_durations_return_errors_without_mutation() {
        let clock = FakeClock::new(DateTime::<Utc>::MAX_UTC);
        assert!(clock.advance(i64::MAX).is_err());
        let mut repo = MemoryExecutionRepository::new(clock);
        let account = AccountId::new("paper_overflow").unwrap();
        repo.register_paper_account(&account).unwrap();
        assert!(
            repo.acquire(&account, &WorkerId::new("worker").unwrap(), 1)
                .is_err()
        );
        assert!(
            repo.accounts
                .lock()
                .unwrap()
                .get(&account)
                .unwrap()
                .fence
                .is_none()
        );
    }
    #[test]
    fn failed_memory_reconciliation_rolls_back_and_snapshot_ids_are_immutable() {
        let now = DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
            .unwrap()
            .to_utc();
        let mut repo = MemoryExecutionRepository::new(FakeClock::new(now));
        let account = AccountId::new("paper_atomic").unwrap();
        repo.register_paper_account(&account).unwrap();
        let fence = repo
            .acquire(&account, &WorkerId::new("worker").unwrap(), 30)
            .unwrap();
        let snapshot = AccountSnapshot {
            snapshot_id: "flat".into(),
            account_id: account.clone(),
            mode: Mode::Paper,
            spendable_cash: Money {
                amount: rust_decimal::Decimal::new(50, 0),
                currency: Currency::new("EUR").unwrap(),
            },
            positions: vec![],
            orders: vec![],
            observed_at: now,
        };
        let original = trady_application::reconcile(snapshot, &[], now).unwrap();
        repo.save_reconciliation(&fence, &original).unwrap();
        let intent = OrderIntent {
            intent_id: IntentId::new("intent_1").unwrap(),
            account_id: account.clone(),
            instrument_id: InstrumentId::new("stock").unwrap(),
            side: Side::Buy,
            quantity: rust_decimal::Decimal::ONE,
            limit_price: rust_decimal::Decimal::TEN,
            session_id: "session_1".into(),
            created_at: now,
            state: IntentState::Prepared,
            broker_order_id: None,
        };
        repo.reserve_paper_intent(&fence, &intent).unwrap();
        let mut bad = original.clone();
        bad.snapshot.snapshot_id = "bad".into();
        bad.resolutions = vec![
            IntentResolution {
                intent_id: intent.intent_id.clone(),
                state: IntentState::Resolved,
                broker_order_id: Some("broker".into()),
            },
            IntentResolution {
                intent_id: IntentId::new("missing").unwrap(),
                state: IntentState::Resolved,
                broker_order_id: None,
            },
        ];
        assert!(repo.save_reconciliation(&fence, &bad).is_err());
        assert_eq!(
            repo.unresolved_intents(&account).unwrap()[0].state,
            IntentState::Prepared
        );
        assert_eq!(
            repo.latest_reconciliation(&account).unwrap().unwrap(),
            original
        );
        let mut changed = original;
        changed.snapshot.spendable_cash.amount = rust_decimal::Decimal::ONE;
        assert!(repo.save_reconciliation(&fence, &changed).is_err());
        assert_eq!(repo.outbox_count(&account).unwrap(), 1);
    }
}
