use super::FineDuration;
use std::time::Duration;

#[test]
fn from_duration_nanos() {
    let d: FineDuration = FineDuration::from(Duration::from_nanos(1));

    assert_eq!(d.picos, 1_000);
}

#[test]
fn from_duration_micros() {
    let d: FineDuration = FineDuration::from(Duration::from_micros(1));

    assert_eq!(d.picos, 1_000_000);
}

#[test]
fn from_duration_millis() {
    let d = FineDuration::from(Duration::from_millis(1));

    assert_eq!(d.picos, 1_000_000_000);
}

#[test]
fn ordering() {
    let a: FineDuration = FineDuration { picos: 100 };
    let b: FineDuration = FineDuration { picos: 200 };

    assert!(a < b);
}

#[test]
fn display_nanoseconds() {
    let d: FineDuration = FineDuration { picos: 125_200 }; // 125.2 ns
    let s: String = format!("{d}");

    assert!(s.contains("ns"), "expected ns unit, got: {s}");
}

#[test]
fn display_microseconds() {
    let d: FineDuration = FineDuration {
        picos: 1_500_000_000,
    }; // 1.5 ms actually
    let s: String = format!("{d}");

    assert!(
        s.contains("ms") || s.contains("µs"),
        "expected µs or ms unit, got: {s}"
    );
}

#[test]
fn arithmetic_add() {
    let a: FineDuration = FineDuration { picos: 100 };
    let b: FineDuration = FineDuration { picos: 200 };

    assert_eq!((a + b).picos, 300);
}

#[test]
fn arithmetic_div_u64() {
    let d: FineDuration = FineDuration { picos: 1000 };

    assert_eq!(d.div_u64(4).picos, 250);
}

#[test]
fn zero() {
    assert_eq!(FineDuration::ZERO.picos, 0);
}

#[test]
fn arithmetic_sub() {
    let a: FineDuration = FineDuration { picos: 300 };
    let b: FineDuration = FineDuration { picos: 100 };

    assert_eq!((a - b).picos, 200);
}

#[test]
fn arithmetic_mul_u64() {
    let d: FineDuration = FineDuration { picos: 100 };

    assert_eq!(d.mul_u64(5).picos, 500);
}

#[test]
fn checked_sub_some() {
    let a: FineDuration = FineDuration { picos: 300 };
    let b: FineDuration = FineDuration { picos: 100 };

    assert_eq!(a.checked_sub(b), Some(FineDuration { picos: 200 }));
}

#[test]
fn checked_sub_none() {
    let a: FineDuration = FineDuration { picos: 100 };
    let b: FineDuration = FineDuration { picos: 300 };

    assert_eq!(a.checked_sub(b), None);
}

#[test]
fn add_assign() {
    let mut a: FineDuration = FineDuration { picos: 100 };
    a += FineDuration { picos: 50 };

    assert_eq!(a.picos, 150);
}

#[test]
fn display_picoseconds() {
    // With default 4 sig figs, picoseconds are shown as nanoseconds
    let d: FineDuration = FineDuration { picos: 500 };
    let s: String = format!("{d}");

    assert!(s.contains("ns"), "expected ns unit, got: {s}");
}

#[test]
fn display_milliseconds() {
    let d: FineDuration = FineDuration {
        picos: 5_000_000_000,
    }; // 5 ms
    let s: String = format!("{d}");

    assert!(s.contains("ms"), "expected ms unit, got: {s}");
}

#[test]
fn display_seconds() {
    let d: FineDuration = FineDuration {
        picos: 2_500_000_000_000,
    }; // 2.5 s
    let s: String = format!("{d}");

    assert!(s.contains('s'), "expected s unit, got: {s}");
    assert!(!s.contains("ms"), "should not be ms, got: {s}");
}
