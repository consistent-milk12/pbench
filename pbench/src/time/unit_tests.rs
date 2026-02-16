//! Tests for the timer infrastructure.

use std::thread as StdThread;
use std::time::{Duration, Instant};

use super::{FineDuration, InstantTimer, Timer, Timestamp};

// --- InstantTimer ---

#[test]
fn instant_timer_measures_time() {
    let timer: InstantTimer = InstantTimer;
    let start: Instant = timer.now();

    StdThread::sleep(Duration::from_millis(10));

    let end: Instant = timer.now();
    let elapsed: FineDuration = timer.elapsed(start, end);

    // Sleep 10ms, should measure at least 5ms (conservative for CI).
    let min_expected_picos: u128 = 5_000_000 * 1_000; // 5ms in picos
    assert!(
        elapsed.picos >= min_expected_picos,
        "elapsed {elapsed} is less than 5ms"
    );
}

#[test]
fn measure_precision_nonzero() {
    let timer: InstantTimer = InstantTimer;
    let precision: FineDuration = timer.measure_precision();
    assert!(
        !precision.is_zero(),
        "InstantTimer precision must be non-zero"
    );
    assert!(
        precision != FineDuration::MAX,
        "precision should not be MAX sentinel"
    );
}

// --- Timer enum ---

#[test]
fn timer_best_available_works() {
    let timer: Timer = Timer::best_available();
    let start: Timestamp = timer.now();
    StdThread::sleep(Duration::from_millis(5));
    let end: Timestamp = timer.now_end();
    let elapsed: FineDuration = timer.elapsed(start, end);

    let min_expected_picos: u128 = 2_000_000 * 1_000; // 2ms in picos
    assert!(
        elapsed.picos >= min_expected_picos,
        "elapsed {elapsed} is less than 2ms"
    );
}

#[test]
fn timer_precision_nonzero() {
    let timer: Timer = Timer::best_available();
    let precision: FineDuration = timer.measure_precision();
    assert!(!precision.is_zero(), "Timer precision must be non-zero");
}
