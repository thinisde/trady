//! Bounded, venue-specific displayed books. Snapshots imply no queue position.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use trady_domain::{Error, Result, intelligence::valid_key, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookLevel {
    pub price: Decimal,
    pub size: Decimal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeedMode {
    Snapshots,
    SequencedUpdates,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BookSide {
    Bid,
    Ask,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum BookChange {
    Snapshot {
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
    },
    Delta {
        side: BookSide,
        price: Decimal,
        size: Decimal,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookUpdate {
    pub instrument_id: String,
    pub venue: String,
    pub sequence: u64,
    pub exchange_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub change: BookChange,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookSnapshot {
    pub instrument_id: String,
    pub venue: String,
    pub sequence: u64,
    pub exchange_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    pub partial_depth: bool,
    /// Changes after recovery, preventing dynamics from crossing a broken feed.
    pub generation: u64,
}

/// Reader implementors must obtain independent entitlement before subscribing.
pub trait BookFeedReader {
    fn next_update(&mut self) -> Result<Option<BookUpdate>>;
}

struct Track {
    last_sequence: u64,
    invalid: bool,
    history: VecDeque<BookSnapshot>,
    generation: u64,
}
pub struct SequenceBookBuilder {
    mode: FeedMode,
    max_symbols: usize,
    max_depth: usize,
    history_size: usize,
    max_age: Duration,
    max_clock_skew: Duration,
    tick_size: Decimal,
    books: BTreeMap<(String, String), Track>,
}
impl SequenceBookBuilder {
    pub fn new(
        mode: FeedMode,
        max_symbols: usize,
        max_depth: usize,
        history_size: usize,
        max_age_ms: i64,
        max_clock_skew_ms: i64,
        tick_size: Decimal,
    ) -> Result<Self> {
        require(
            (1..=128).contains(&max_symbols)
                && (1..=100).contains(&max_depth)
                && (1..=256).contains(&history_size)
                && (1..=60_000).contains(&max_age_ms)
                && (0..=1000).contains(&max_clock_skew_ms)
                && tick_size > Decimal::ZERO,
            "invalid bounded book configuration",
        )?;
        Ok(Self {
            mode,
            max_symbols,
            max_depth,
            history_size,
            max_age: Duration::milliseconds(max_age_ms),
            max_clock_skew: Duration::milliseconds(max_clock_skew_ms),
            tick_size,
            books: BTreeMap::new(),
        })
    }
    fn price(&self, price: Decimal) -> bool {
        price > Decimal::ZERO && price.checked_rem(self.tick_size) == Some(Decimal::ZERO)
    }
    fn levels(&self, levels: &[BookLevel], bid: bool) -> bool {
        !levels.is_empty()
            && levels.len() <= self.max_depth
            && levels
                .iter()
                .all(|l| self.price(l.price) && l.size > Decimal::ZERO)
            && levels.windows(2).all(|p| {
                if bid {
                    p[0].price > p[1].price
                } else {
                    p[0].price < p[1].price
                }
            })
    }
    fn fail(&mut self, key: &(String, String), reason: &str) -> Error {
        if let Some(track) = self.books.get_mut(key) {
            track.invalid = true;
        }
        Error(reason.into())
    }
    pub fn apply(&mut self, update: BookUpdate, now: DateTime<Utc>) -> Result<BookSnapshot> {
        let key = (update.instrument_id.clone(), update.venue.clone());
        if !valid_key(&update.instrument_id)
            || !valid_key(&update.venue)
            || update.sequence == 0
            || update.received_at > now
            || now - update.received_at > self.max_age
            || update.exchange_at - update.received_at > self.max_clock_skew
            || update.received_at - update.exchange_at > self.max_age
        {
            return Err(self.fail(&key, "invalid, stale or skewed book update"));
        }
        let previous = self.books.get(&key);
        if previous.is_some_and(|p| update.sequence <= p.last_sequence) {
            return Err(self.fail(&key, "out-of-order book update"));
        }
        if previous.and_then(|p| p.history.back()).is_some_and(|s| {
            update.exchange_at < s.exchange_at || update.received_at < s.received_at
        }) {
            return Err(self.fail(&key, "regressive book timestamps"));
        }
        if previous.is_none() && self.books.len() >= self.max_symbols {
            return Err(Error("book subscription limit reached".into()));
        }
        let reset = previous
            .is_none_or(|p| p.invalid || p.last_sequence.checked_add(1) != Some(update.sequence));
        let generation = if reset {
            previous
                .map_or(Some(1), |p| p.generation.checked_add(1))
                .ok_or_else(|| Error("book generation exhausted".into()))?
        } else {
            previous.map_or(1, |p| p.generation)
        };
        let (bids, asks) = match update.change {
            BookChange::Snapshot { bids, asks } => (bids, asks),
            BookChange::Delta { side, price, size } => {
                if self.mode != FeedMode::SequencedUpdates
                    || !self.price(price)
                    || size < Decimal::ZERO
                {
                    return Err(self.fail(&key, "invalid delta for feed mode"));
                }
                let Some(previous) = previous else {
                    return Err(Error("snapshot required before deltas".into()));
                };
                if previous.invalid
                    || previous.last_sequence.checked_add(1) != Some(update.sequence)
                {
                    if let Some(track) = self.books.get_mut(&key) {
                        track.last_sequence = update.sequence;
                    }
                    return Err(self.fail(&key, "sequence gap: fresh snapshot required"));
                }
                let snapshot = previous
                    .history
                    .back()
                    .ok_or_else(|| Error("book snapshot missing".into()))?;
                let mut bids: BTreeMap<_, _> =
                    snapshot.bids.iter().map(|l| (l.price, l.size)).collect();
                let mut asks: BTreeMap<_, _> =
                    snapshot.asks.iter().map(|l| (l.price, l.size)).collect();
                let book = if side == BookSide::Bid {
                    &mut bids
                } else {
                    &mut asks
                };
                if size.is_zero() {
                    book.remove(&price);
                } else {
                    book.insert(price, size);
                }
                // A discarded price may become the best level after later
                // deletions. Stop and resync instead of claiming a valid book.
                if bids.len() > self.max_depth || asks.len() > self.max_depth {
                    return Err(self.fail(&key, "book capacity exceeded: fresh snapshot required"));
                }
                (
                    bids.into_iter()
                        .rev()
                        .map(|(price, size)| BookLevel { price, size })
                        .collect(),
                    asks.into_iter()
                        .map(|(price, size)| BookLevel { price, size })
                        .collect(),
                )
            }
        };
        if !self.levels(&bids, true) || !self.levels(&asks, false) || bids[0].price >= asks[0].price
        {
            return Err(self.fail(&key, "invalid, empty, locked or crossed displayed book"));
        }
        let snapshot = BookSnapshot {
            instrument_id: update.instrument_id,
            venue: update.venue,
            sequence: update.sequence,
            exchange_at: update.exchange_at,
            received_at: update.received_at,
            bids,
            asks,
            partial_depth: true,
            generation,
        };
        let track = self.books.entry(key).or_insert_with(|| Track {
            last_sequence: 0,
            invalid: true,
            history: VecDeque::new(),
            generation: 0,
        });
        if reset {
            track.history.clear();
        }
        track.invalid = false;
        track.last_sequence = snapshot.sequence;
        track.generation = generation;
        track.history.push_back(snapshot.clone());
        while track.history.len() > self.history_size {
            track.history.pop_front();
        }
        Ok(snapshot)
    }
    pub fn snapshot(
        &self,
        instrument: &str,
        venue: &str,
        now: DateTime<Utc>,
    ) -> Result<&BookSnapshot> {
        let track = self
            .books
            .get(&(instrument.into(), venue.into()))
            .ok_or_else(|| Error("book unavailable".into()))?;
        require(!track.invalid, "book invalidated: fresh snapshot required")?;
        let snapshot = track
            .history
            .back()
            .ok_or_else(|| Error("book unavailable".into()))?;
        require(
            snapshot.received_at <= now && now - snapshot.received_at <= self.max_age,
            "book stale",
        )?;
        Ok(snapshot)
    }
    pub fn history_len(&self, instrument: &str, venue: &str) -> usize {
        self.books
            .get(&(instrument.into(), venue.into()))
            .map_or(0, |t| t.history.len())
    }
    pub fn research_book(
        &self,
        instrument: &str,
        venue: &str,
        now: DateTime<Utc>,
        epoch: &str,
        source_reference: &str,
    ) -> Result<trady_domain::intelligence::microstructure::MicrostructureBook> {
        use trady_domain::intelligence::microstructure::*;
        require(
            valid_key(epoch) && !source_reference.is_empty() && source_reference.len() <= 2048,
            "book research projection requires feed epoch and source reference",
        )?;
        let s = self.snapshot(instrument, venue, now)?;
        Ok(MicrostructureBook {
            instrument_id: s.instrument_id.clone(),
            venue: s.venue.clone(),
            sequence: s.sequence,
            epoch: trady_application::harness::content_hash(&format!("{epoch}:{}", s.generation)),
            book_version: format!("sequence_{}", s.sequence),
            feed_kind: match self.mode {
                FeedMode::Snapshots => MicrostructureFeedKind::IndependentSnapshot,
                FeedMode::SequencedUpdates => MicrostructureFeedKind::SequencedBook,
            },
            state: MicrostructureBookState::ValidPartialDepth,
            exchange_time: s.exchange_at,
            received_at: s.received_at,
            available_at: now,
            source_ref: source_reference.into(),
            bids: s
                .bids
                .iter()
                .map(|l| MicrostructureLevel {
                    price: l.price,
                    size_shares: l.size,
                })
                .collect(),
            asks: s
                .asks
                .iter()
                .map(|l| MicrostructureLevel {
                    price: l.price,
                    size_shares: l.size,
                })
                .collect(),
        })
    }
}
