//! Bounded retries for read-only broker observations. Order dispatch never uses this path.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use trady_adapters::trading212::Trading212Reader;
use trady_application::broker::{BrokerFailureKind, BrokerResult};
use trady_domain::{AccountId, AccountSnapshot, Error, Result};

const MAX_ATTEMPTS: usize = 3;
const MAX_WAIT_SECONDS: u64 = 30;

pub(crate) fn snapshot(
    reader: &mut Trading212Reader,
    account: &AccountId,
    stopped: &AtomicBool,
) -> Result<AccountSnapshot> {
    retry(
        || reader.read_snapshot(account),
        |seconds| {
            for _ in 0..seconds * 10 {
                if stopped.load(Ordering::Relaxed) {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            !stopped.load(Ordering::Relaxed)
        },
        || stopped.load(Ordering::Relaxed),
    )
}

fn retry<T>(
    mut read: impl FnMut() -> BrokerResult<T>,
    mut wait: impl FnMut(u64) -> bool,
    stopped: impl Fn() -> bool,
) -> Result<T> {
    let mut budget = MAX_WAIT_SECONDS;
    for attempt in 0..MAX_ATTEMPTS {
        if stopped() {
            return Err(Error("broker read interrupted by shutdown".into()));
        }
        match read() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let delay = error.retry_after_seconds.unwrap_or(5).max(1);
                if error.kind != BrokerFailureKind::RateLimited
                    || error.outcome_unknown
                    || attempt + 1 == MAX_ATTEMPTS
                    || delay > budget
                {
                    return Err(Error(error.to_string()));
                }
                budget -= delay;
                if !wait(delay) {
                    return Err(Error("broker read interrupted by shutdown".into()));
                }
            }
        }
    }
    unreachable!("the last read attempt always returns")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use trady_application::broker::BrokerFailure;

    fn throttled(seconds: u64) -> BrokerFailure {
        let mut error = BrokerFailure::new(BrokerFailureKind::RateLimited);
        error.retry_after_seconds = Some(seconds);
        error
    }

    #[test]
    fn throttled_reads_wait_for_the_broker_and_retry_within_a_fixed_budget() {
        let mut replies = [Err(throttled(6)), Err(throttled(7)), Ok("observed")].into_iter();
        let mut waits = vec![];
        let value = retry(
            || replies.next().unwrap(),
            |seconds| {
                waits.push(seconds);
                true
            },
            || false,
        )
        .unwrap();
        assert_eq!(value, "observed");
        assert_eq!(waits, [6, 7]);
    }

    #[test]
    fn persistent_throttles_stop_after_three_attempts() {
        let attempts = Cell::new(0);
        let mut waits = vec![];
        assert!(
            retry::<()>(
                || {
                    attempts.set(attempts.get() + 1);
                    Err(throttled(0))
                },
                |seconds| {
                    waits.push(seconds);
                    true
                },
                || false,
            )
            .is_err()
        );
        assert_eq!(attempts.get(), 3);
        assert_eq!(waits, [1, 1]);
    }

    #[test]
    fn long_delays_and_other_errors_are_not_retried() {
        for error in [
            throttled(31),
            BrokerFailure::new(BrokerFailureKind::Transport),
            BrokerFailure::new(BrokerFailureKind::Rejected),
            BrokerFailure {
                outcome_unknown: true,
                ..throttled(1)
            },
        ] {
            let attempts = Cell::new(0);
            assert!(
                retry::<()>(
                    || {
                        attempts.set(attempts.get() + 1);
                        Err(error.clone())
                    },
                    |_| panic!("must not wait"),
                    || false,
                )
                .is_err()
            );
            assert_eq!(attempts.get(), 1);
        }
        let mut replies = [Err(throttled(15)), Err(throttled(16))].into_iter();
        let mut waits = vec![];
        assert!(
            retry::<()>(
                || replies.next().unwrap(),
                |seconds| {
                    waits.push(seconds);
                    true
                },
                || false,
            )
            .is_err()
        );
        assert_eq!(waits, [15]);
    }

    #[test]
    fn shutdown_prevents_reads_and_interrupts_backoff() {
        assert!(
            retry::<()>(
                || panic!("must not read"),
                |_| panic!("must not wait"),
                || true
            )
            .is_err()
        );
        let attempts = Cell::new(0);
        assert!(
            retry::<()>(
                || {
                    attempts.set(attempts.get() + 1);
                    Err(throttled(6))
                },
                |_| false,
                || false,
            )
            .is_err()
        );
        assert_eq!(attempts.get(), 1);
    }
}
