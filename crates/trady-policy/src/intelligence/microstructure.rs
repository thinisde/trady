//! Pure displayed-book metrics. Snapshot comparisons never infer individual cancels,
//! hidden liquidity, queue priority, routing quality or executable fill probability.

use crate::{add, div, mul, sub};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use trady_domain::intelligence::microstructure::*;
use trady_domain::{Error, Result, require};

pub fn assess(input: &MicrostructureInput, at: DateTime<Utc>) -> Result<MicrostructureAssessment> {
    require(
        !input.instrument_id.trim().is_empty()
            && !input.venue.trim().is_empty()
            && input.tick_size > Decimal::ZERO
            && input.size_increment > Decimal::ZERO
            && input.max_age_milliseconds > 0
            && input.max_clock_skew_milliseconds > 0
            && input.max_age_milliseconds <= i64::MAX as u64
            && input.max_clock_skew_milliseconds <= i64::MAX as u64
            && input.depth_limit > 0
            && input.depth_limit <= 1024
            && input.capacity_band_ticks > 0
            && input.history.len() <= 512,
        "invalid bounded microstructure configuration",
    )?;
    let mut out = MicrostructureAssessment {
        at,
        instrument_id: input.instrument_id.clone(),
        venue: input.venue.clone(),
        currency: input.currency.clone(),
        routing: input.routing,
        derived_cost_model: "DISPLAYED_DEPTH_V1_NO_FILL_PROBABILITY_MODEL".into(),
        depth_limit: input.depth_limit,
        capacity_band_ticks: input.capacity_band_ticks,
        status: MicrostructureStatus::Unavailable,
        book_state: None,
        sequence: None,
        book_version: None,
        exchange_time: None,
        received_at: None,
        receive_minus_exchange_milliseconds: None,
        metrics: None,
        dynamics: None,
        auction: None,
        post_signal_adverse_selection_basis_points: None,
        markout_is_confirmed_fill: None,
        source_refs: vec![],
        issues: vec![],
    };
    let Some(book) = &input.book else {
        out.issues
            .push("book absent; missing feed is not zero liquidity".into());
        return Ok(out);
    };
    out.book_state = Some(book.state);
    out.sequence = Some(book.sequence);
    out.book_version = Some(book.book_version.clone());
    out.exchange_time = Some(book.exchange_time);
    out.received_at = Some(book.received_at);
    out.receive_minus_exchange_milliseconds =
        Some((book.received_at - book.exchange_time).num_milliseconds());
    out.source_refs.push(book.source_ref.clone());
    if let Some(reason) = invalid_book(input, book, at, true) {
        out.status = MicrostructureStatus::Invalidated;
        out.issues.push(reason.into());
        return Ok(out);
    }
    if matches!(
        book.state,
        MicrostructureBookState::Auction | MicrostructureBookState::Halted
    ) {
        out.status = if book.state == MicrostructureBookState::Auction {
            MicrostructureStatus::Auction
        } else {
            MicrostructureStatus::Halted
        };
        if book.state == MicrostructureBookState::Auction {
            if let Some(a) = &input.auction {
                require(
                    a.indicative_price > Decimal::ZERO
                        && a.indicative_price.checked_rem(input.tick_size) == Some(Decimal::ZERO)
                        && a.observed_at <= a.available_at
                        && !a.source_ref.trim().is_empty(),
                    "invalid auction observation",
                )?;
                if a.available_at <= at
                    && a.observed_at <= at
                    && at - a.observed_at
                        <= chrono::TimeDelta::milliseconds(input.max_age_milliseconds as i64)
                {
                    out.auction = Some(a.clone());
                    out.source_refs.push(a.source_ref.clone());
                } else {
                    out.issues
                        .push("auction indication stale/unavailable".into());
                }
            } else {
                out.issues.push(
                    "auction indication missing; continuous quotes were not substituted".into(),
                );
            }
        }
        out.issues
            .push("continuous-book metrics withheld in auction/halt state".into());
        return Ok(out);
    }
    if book.bids.is_empty() || book.asks.is_empty() {
        out.issues
            .push("book side absent; depth cannot be interpreted as zero liquidity".into());
        return Ok(out);
    }
    validate_levels(input, book)?;
    // A regressive snapshot invalidates the comparison and current book together.
    let mut previous: Option<&MicrostructureBook> = None;
    for h in input.history.iter().chain(std::iter::once(book)) {
        if let Some(p) = previous
            && (h.sequence <= p.sequence
                || h.exchange_time <= p.exchange_time
                || h.received_at < p.received_at
                || h.available_at < p.available_at)
        {
            out.status = MicrostructureStatus::Invalidated;
            out.issues
                .push("out-of-order snapshot/sequence/timestamp".into());
            return Ok(out);
        }
        previous = Some(h);
    }
    let metrics = metrics(input, book)?;
    if metrics.bid_slope_shares_per_tick.is_none() || metrics.ask_slope_shares_per_tick.is_none() {
        out.issues
            .push("book slope requires at least two selected price levels per side".into());
    }
    if metrics.bid_capacity_truncated || metrics.ask_capacity_truncated {
        out.issues
            .push("capacity is a lower bound on truncated displayed depth".into());
    }
    if book.state == MicrostructureBookState::ValidPartialDepth {
        out.issues.push(
            "partial venue depth; unseen, hidden and other-venue liquidity is unknown".into(),
        );
    }
    if input.routing != MicrostructureRouting::VerifiedSelectedVenue {
        out.issues.push("routing unknown or outside selected venue; research metrics have no execution-quality authority".into());
    }
    if input.history.is_empty() {
        out.issues
            .push("snapshot history absent; persistence/update dynamics unavailable".into());
    } else {
        let mut usable = true;
        for h in &input.history {
            if invalid_book(input, h, at, false).is_some()
                || h.epoch != book.epoch
                || h.feed_kind != book.feed_kind
                || !matches!(
                    h.state,
                    MicrostructureBookState::ValidCompleteDepth
                        | MicrostructureBookState::ValidPartialDepth
                )
                || h.bids.is_empty()
                || h.asks.is_empty()
            {
                usable = false;
                break;
            }
            validate_levels(input, h)?;
        }
        if usable {
            out.dynamics = Some(dynamics(input, book)?);
            out.source_refs
                .extend(input.history.iter().map(|h| h.source_ref.clone()));
        } else {
            out.issues.push(
                "history contains invalid state, venue, clock, gap or reset; dynamics withheld"
                    .into(),
            );
        }
    }
    out.metrics = Some(metrics);
    if let Some(m) = &input.markout {
        require(
            m.signal_price > Decimal::ZERO
                && m.mark_mid > Decimal::ZERO
                && m.signal_at < m.mark_at
                && m.mark_at <= m.available_at
                && m.registered_window_milliseconds > 0
                && m.registered_window_milliseconds <= i64::MAX as u64
                && !m.signal_source_ref.trim().is_empty()
                && !m.mark_source_ref.trim().is_empty(),
            "invalid registered markout",
        )?;
        let elapsed = m.mark_at - m.signal_at;
        if m.available_at > at
            || m.mark_at > at
            || elapsed != chrono::TimeDelta::milliseconds(m.registered_window_milliseconds as i64)
        {
            out.issues.push(
                "post-signal window incomplete/unpublished or differs from registered horizon"
                    .into(),
            );
        } else {
            let adverse = match m.side {
                MicrostructureMarkoutSide::Buy => sub(m.signal_price, m.mark_mid)?,
                MicrostructureMarkoutSide::Sell => sub(m.mark_mid, m.signal_price)?,
            };
            out.post_signal_adverse_selection_basis_points =
                Some(mul(div(adverse, m.signal_price)?, Decimal::from(10000))?);
            out.markout_is_confirmed_fill = Some(m.signal_price_is_confirmed_fill);
            out.source_refs
                .extend([m.signal_source_ref.clone(), m.mark_source_ref.clone()]);
        }
    } else {
        out.issues.push(
            "registered post-signal markout absent; no modeled fill/adverse-selection claim".into(),
        );
    }
    out.status = if out.issues.is_empty() {
        MicrostructureStatus::Available
    } else {
        MicrostructureStatus::Partial
    };
    out.source_refs.sort();
    out.source_refs.dedup();
    Ok(out)
}

fn invalid_book(
    input: &MicrostructureInput,
    b: &MicrostructureBook,
    at: DateTime<Utc>,
    fresh: bool,
) -> Option<&'static str> {
    if b.instrument_id != input.instrument_id || b.venue != input.venue {
        return Some("mismatched book instrument/venue");
    }
    if b.epoch.trim().is_empty()
        || b.book_version.trim().is_empty()
        || b.source_ref.trim().is_empty()
        || b.sequence == 0
    {
        return Some("missing book epoch/version/sequence/reference");
    }
    if matches!(
        b.state,
        MicrostructureBookState::SequenceGap | MicrostructureBookState::Invalidated
    ) {
        return Some("sequence gap or feed invalidation; a resynchronized book is required");
    }
    if b.received_at > b.available_at || b.available_at > at || b.exchange_time > at {
        return Some("book timestamps are inconsistent or not available yet");
    }
    if (b.received_at - b.exchange_time).abs()
        > chrono::TimeDelta::milliseconds(input.max_clock_skew_milliseconds as i64)
    {
        return Some("exchange/receive latency or clock skew exceeds the verified limit");
    }
    if fresh
        && (at - b.exchange_time
            > chrono::TimeDelta::milliseconds(input.max_age_milliseconds as i64)
            || at - b.received_at
                > chrono::TimeDelta::milliseconds(input.max_age_milliseconds as i64))
    {
        return Some("stale book; displayed liquidity metrics invalidated");
    }
    None
}

fn validate_levels(input: &MicrostructureInput, book: &MicrostructureBook) -> Result<()> {
    require(
        book.bids.len() <= 1024 && book.asks.len() <= 1024,
        "book exceeds bounded depth",
    )?;
    for levels in [&book.bids, &book.asks] {
        for l in levels {
            require(
                l.price > Decimal::ZERO
                    && l.size_shares > Decimal::ZERO
                    && l.price.checked_rem(input.tick_size) == Some(Decimal::ZERO)
                    && l.size_shares.checked_rem(input.size_increment) == Some(Decimal::ZERO),
                "nonpositive/off-grid book price or size",
            )?;
        }
    }
    require(
        book.bids.windows(2).all(|p| p[0].price > p[1].price)
            && book.asks.windows(2).all(|p| p[0].price < p[1].price),
        "book prices must be strictly sorted/aggregated by level",
    )?;
    require(
        book.bids[0].price < book.asks[0].price,
        "locked/crossed continuous book",
    )
}

fn metrics(input: &MicrostructureInput, b: &MicrostructureBook) -> Result<MicrostructureMetrics> {
    let bids = &b.bids[..b.bids.len().min(input.depth_limit)];
    let asks = &b.asks[..b.asks.len().min(input.depth_limit)];
    let bid = &bids[0];
    let ask = &asks[0];
    let mid = div(add(bid.price, ask.price)?, Decimal::from(2))?;
    let spread = sub(ask.price, bid.price)?;
    let (bid_size, bid_notional) = sum(bids)?;
    let (ask_size, ask_notional) = sum(asks)?;
    let top_size = add(bid.size_shares, ask.size_shares)?;
    let microprice = div(
        add(
            mul(ask.price, bid.size_shares)?,
            mul(bid.price, ask.size_shares)?,
        )?,
        top_size,
    )?;
    let band = mul(input.tick_size, Decimal::from(input.capacity_band_ticks))?;
    let lower = sub(bid.price, band)?;
    let upper = add(ask.price, band)?;
    let mut bid_capacity = Decimal::ZERO;
    let mut ask_capacity = Decimal::ZERO;
    for l in bids.iter().filter(|l| l.price >= lower) {
        bid_capacity = add(bid_capacity, mul(l.price, l.size_shares)?)?;
    }
    for l in asks.iter().filter(|l| l.price <= upper) {
        ask_capacity = add(ask_capacity, mul(l.price, l.size_shares)?)?;
    }
    let partial = b.state == MicrostructureBookState::ValidPartialDepth;
    Ok(MicrostructureMetrics {
        mid_price: mid,
        spread,
        spread_ticks: div(spread, input.tick_size)?,
        spread_basis_points: mul(div(spread, mid)?, Decimal::from(10000))?,
        displayed_imbalance_fraction: div(sub(bid_size, ask_size)?, add(bid_size, ask_size)?)?,
        microprice,
        bid_depth_shares: bid_size,
        ask_depth_shares: ask_size,
        bid_notional,
        ask_notional,
        bid_capacity_notional: bid_capacity,
        ask_capacity_notional: ask_capacity,
        bid_capacity_truncated: (partial || b.bids.len() > bids.len())
            && bids.last().unwrap().price > lower,
        ask_capacity_truncated: (partial || b.asks.len() > asks.len())
            && asks.last().unwrap().price < upper,
        bid_slope_shares_per_tick: slope(bids, bid_size, input.tick_size)?,
        ask_slope_shares_per_tick: slope(asks, ask_size, input.tick_size)?,
        bid_levels: bids.len(),
        ask_levels: asks.len(),
    })
}
fn sum(levels: &[MicrostructureLevel]) -> Result<(Decimal, Decimal)> {
    let (mut size, mut notional) = (Decimal::ZERO, Decimal::ZERO);
    for l in levels {
        size = add(size, l.size_shares)?;
        notional = add(notional, mul(l.price, l.size_shares)?)?;
    }
    Ok((size, notional))
}
fn slope(levels: &[MicrostructureLevel], size: Decimal, tick: Decimal) -> Result<Option<Decimal>> {
    if levels.len() < 2 {
        return Ok(None);
    }
    let distance = sub(levels[0].price, levels.last().unwrap().price)?.abs();
    Ok(Some(div(
        sub(size, levels[0].size_shares)?,
        div(distance, tick)?,
    )?))
}

fn dynamics(
    input: &MicrostructureInput,
    book: &MicrostructureBook,
) -> Result<MicrostructureDynamics> {
    let first = &input.history[0];
    let first_metrics = metrics(input, first)?;
    let last_metrics = metrics(input, book)?;
    let initial_depth = add(
        first_metrics.bid_depth_shares,
        first_metrics.ask_depth_shares,
    )?;
    let mut minimum_depth = initial_depth;
    let mut updates = 0_u64;
    let mut previous = first;
    for b in input.history.iter().skip(1).chain(std::iter::once(book)) {
        let m = metrics(input, b)?;
        minimum_depth = minimum_depth.min(add(m.bid_depth_shares, m.ask_depth_shares)?);
        if b.bids[0] != previous.bids[0] || b.asks[0] != previous.asks[0] {
            updates += 1;
        }
        previous = b;
    }
    let elapsed = (book.exchange_time - first.exchange_time)
        .num_nanoseconds()
        .ok_or_else(|| {
            Error("snapshot history duration exceeds bounded nanosecond arithmetic".into())
        })?;
    require(
        elapsed > 0,
        "snapshot dynamics require positive elapsed history",
    )?;
    Ok(MicrostructureDynamics {
        window_start: first.exchange_time,
        window_end: book.exchange_time,
        snapshots: input.history.len() + 1,
        spread_change: sub(last_metrics.spread, first_metrics.spread)?,
        imbalance_change: sub(
            last_metrics.displayed_imbalance_fraction,
            first_metrics.displayed_imbalance_fraction,
        )?,
        displayed_depth_persistence_fraction: div(minimum_depth, initial_depth)?,
        top_quote_updates_per_second: div(
            mul(Decimal::from(updates), Decimal::from(1_000_000_000))?,
            Decimal::from(elapsed),
        )?,
    })
}

#[cfg(test)]
mod microstructure_acceptance {
    use super::*;
    use chrono::Duration;
    use trady_domain::Currency;
    fn time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T14:30:01.025Z")
            .unwrap()
            .to_utc()
    }
    fn fixture() -> MicrostructureInput {
        let at = time();
        MicrostructureInput {
            instrument_id: "equity-42".into(),
            venue: "XNAS".into(),
            currency: Currency::new("USD").unwrap(),
            tick_size: Decimal::new(1, 2),
            size_increment: Decimal::ONE,
            max_age_milliseconds: 100,
            max_clock_skew_milliseconds: 20,
            depth_limit: 10,
            capacity_band_ticks: 5,
            routing: MicrostructureRouting::Unknown,
            book: Some(MicrostructureBook {
                instrument_id: "equity-42".into(),
                venue: "XNAS".into(),
                sequence: 95002,
                epoch: "session-1".into(),
                book_version: "depth-v1".into(),
                feed_kind: MicrostructureFeedKind::SequencedBook,
                state: MicrostructureBookState::ValidPartialDepth,
                exchange_time: at - Duration::milliseconds(15),
                received_at: at,
                available_at: at,
                source_ref: "snapshot-95002".into(),
                bids: vec![
                    MicrostructureLevel {
                        price: Decimal::from(100),
                        size_shares: Decimal::from(800),
                    },
                    MicrostructureLevel {
                        price: Decimal::new(9998, 2),
                        size_shares: Decimal::from(200),
                    },
                ],
                asks: vec![
                    MicrostructureLevel {
                        price: Decimal::new(10001, 2),
                        size_shares: Decimal::from(450),
                    },
                    MicrostructureLevel {
                        price: Decimal::new(10003, 2),
                        size_shares: Decimal::from(550),
                    },
                ],
            }),
            history: vec![],
            auction: None,
            markout: None,
        }
    }
    #[test]
    fn exact_displayed_metrics_and_truncated_capacity() {
        let out = assess(&fixture(), time()).unwrap();
        let m = out.metrics.unwrap();
        assert_eq!(m.spread, Decimal::new(1, 2));
        assert_eq!(m.spread_ticks, Decimal::ONE);
        assert_eq!(m.displayed_imbalance_fraction, Decimal::ZERO);
        assert_eq!(m.microprice, Decimal::new(1000064, 4));
        assert_eq!(m.bid_depth_shares, Decimal::from(1000));
        assert_eq!(m.bid_notional, Decimal::from(99996));
        assert_eq!(m.bid_slope_shares_per_tick, Some(Decimal::from(100)));
        assert!(m.bid_capacity_truncated && m.ask_capacity_truncated);
        assert!(out.issues.iter().any(|s| s.contains("routing")));
    }
    #[test]
    fn zero_sizes_locked_crossed_and_off_grid_are_errors() {
        for mode in 0..4 {
            let mut i = fixture();
            let b = i.book.as_mut().unwrap();
            match mode {
                0 => b.bids[0].size_shares = Decimal::ZERO,
                1 => b.asks[0].price = b.bids[0].price,
                2 => b.asks[0].price = Decimal::from(99),
                _ => b.bids[0].price = Decimal::new(100001, 3),
            }
            assert!(assess(&i, time()).is_err());
        }
    }
    #[test]
    fn stale_gap_clock_and_venue_invalidate_without_zero_depth() {
        for mode in 0..4 {
            let mut i = fixture();
            match mode {
                0 => {
                    let b = i.book.as_mut().unwrap();
                    b.exchange_time -= Duration::seconds(1);
                    b.received_at -= Duration::seconds(1);
                    b.available_at = b.received_at;
                }
                1 => i.book.as_mut().unwrap().state = MicrostructureBookState::SequenceGap,
                2 => i.book.as_mut().unwrap().exchange_time -= Duration::milliseconds(50),
                _ => i.book.as_mut().unwrap().venue = "XNYS".into(),
            }
            let out = assess(&i, time()).unwrap();
            assert_eq!(out.status, MicrostructureStatus::Invalidated);
            assert!(out.metrics.is_none());
        }
    }
    #[test]
    fn absent_book_is_unavailable() {
        let mut i = fixture();
        i.book = None;
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.status, MicrostructureStatus::Unavailable);
        assert!(out.metrics.is_none());
    }
    #[test]
    fn auction_is_separate_from_continuous_metrics() {
        let mut i = fixture();
        i.book.as_mut().unwrap().state = MicrostructureBookState::Auction;
        i.auction = Some(MicrostructureAuction {
            indicative_price: Decimal::from(101),
            imbalance_shares: Decimal::from(-500),
            observed_at: time(),
            available_at: time(),
            source_ref: "auction".into(),
        });
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.status, MicrostructureStatus::Auction);
        assert!(out.metrics.is_none());
        assert_eq!(out.auction.unwrap().imbalance_shares, Decimal::from(-500));
    }
    #[test]
    fn snapshot_dynamics_measure_aggregate_persistence() {
        let mut i = fixture();
        let mut previous = i.book.as_ref().unwrap().clone();
        previous.sequence -= 1;
        previous.exchange_time -= Duration::milliseconds(100);
        previous.received_at -= Duration::milliseconds(100);
        previous.available_at = previous.received_at;
        previous.source_ref = "previous-snapshot".into();
        previous.bids[0].size_shares = Decimal::from(1800);
        i.history.push(previous);
        let out = assess(&i, time()).unwrap();
        let d = out.dynamics.unwrap();
        assert_eq!(
            d.displayed_depth_persistence_fraction,
            Decimal::new(2, 0) / Decimal::new(3, 0)
        );
        assert_eq!(d.top_quote_updates_per_second, Decimal::from(10));
        assert_eq!(d.snapshots, 2);
    }
    #[test]
    fn regressive_sequence_invalidates_current_book() {
        let mut i = fixture();
        let mut previous = i.book.as_ref().unwrap().clone();
        previous.sequence += 1;
        previous.exchange_time -= Duration::milliseconds(1);
        previous.received_at -= Duration::milliseconds(1);
        previous.available_at = previous.received_at;
        i.history.push(previous);
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.status, MicrostructureStatus::Invalidated);
        assert!(out.metrics.is_none());
    }
    #[test]
    fn history_from_different_feed_epoch_cannot_infer_cancellations() {
        let mut i = fixture();
        let mut previous = i.book.as_ref().unwrap().clone();
        previous.sequence -= 1;
        previous.exchange_time -= Duration::milliseconds(1);
        previous.received_at -= Duration::milliseconds(1);
        previous.available_at = previous.received_at;
        previous.epoch = "old-session".into();
        i.history.push(previous);
        let out = assess(&i, time()).unwrap();
        assert!(out.metrics.is_some());
        assert!(out.dynamics.is_none());
    }
    #[test]
    fn markout_uses_only_completed_registered_window() {
        let mut i = fixture();
        i.markout = Some(MicrostructureMarkout {
            side: MicrostructureMarkoutSide::Buy,
            signal_at: time() - Duration::seconds(1),
            signal_price: Decimal::from(100),
            signal_source_ref: "signal".into(),
            mark_at: time(),
            mark_mid: Decimal::from(99),
            mark_source_ref: "mark".into(),
            available_at: time(),
            registered_window_milliseconds: 1000,
            signal_price_is_confirmed_fill: false,
        });
        let out = assess(&i, time()).unwrap();
        assert_eq!(
            out.post_signal_adverse_selection_basis_points,
            Some(Decimal::from(100))
        );
        assert_eq!(out.markout_is_confirmed_fill, Some(false));
        i.markout.as_mut().unwrap().available_at += Duration::seconds(1);
        assert!(
            assess(&i, time())
                .unwrap()
                .post_signal_adverse_selection_basis_points
                .is_none()
        );
    }
    #[test]
    fn missing_side_and_one_level_slope_do_not_fabricate_data() {
        let mut i = fixture();
        i.book.as_mut().unwrap().bids.clear();
        assert!(assess(&i, time()).unwrap().metrics.is_none());
        i = fixture();
        i.depth_limit = 1;
        let out = assess(&i, time()).unwrap();
        assert!(out.metrics.unwrap().bid_slope_shares_per_tick.is_none());
    }
    #[test]
    fn capacity_respects_registered_band_and_observed_depth() {
        let mut i = fixture();
        i.capacity_band_ticks = 1;
        let m = assess(&i, time()).unwrap().metrics.unwrap();
        assert_eq!(m.bid_capacity_notional, Decimal::from(80000));
        assert_eq!(m.ask_capacity_notional, Decimal::new(450045, 1));
        assert!(!m.bid_capacity_truncated && !m.ask_capacity_truncated);
    }
    #[test]
    fn checked_notional_overflow_is_rejected() {
        let mut i = fixture();
        i.book.as_mut().unwrap().bids[0].size_shares = Decimal::MAX;
        assert!(assess(&i, time()).is_err());
    }
    #[test]
    fn strict_json_cannot_add_order_authority() {
        let mut v = serde_json::to_value(fixture()).unwrap();
        v["book"]["market_order_fallback"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MicrostructureInput>(v).is_err());
    }

    #[test]
    fn nanosecond_beyond_freshness_limit_is_not_rounded_down() {
        let mut i = fixture();
        let book = i.book.as_mut().unwrap();
        book.exchange_time = time() - Duration::milliseconds(100) - Duration::nanoseconds(1);
        book.received_at = book.exchange_time + Duration::milliseconds(15);
        book.available_at = book.received_at;
        assert_eq!(
            assess(&i, time()).unwrap().status,
            MicrostructureStatus::Invalidated
        );
    }
}
