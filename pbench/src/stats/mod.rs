//! Core Percentile Computation Engine
//!
//! The current approach is to use sorted arrays and linear interpolation.

pub mod exact;

use crate::time::FineDuration;

/// Complete percentile stats for a benchmark run.
#[derive(Clone, Debug)]
pub struct PercentileStats {
    /// Number of samples collected.
    pub sample_count: u32,

    /// Total iterations across all samples.
    pub iter_count: u64,

    /// Minimum observed duration.
    pub min: FineDuration,

    /// Maximum observed duration.
    pub max: FineDuration,

    /// Arithmetic mean duration.
    pub mean: FineDuration,

    /// Population standard deviation.
    pub std_dev: FineDuration,

    /// Percentile breakpoints.
    pub percentiles: PercentileSet,
}

/// The five standard percentile breakpoints.
#[derive(Clone, Copy, Debug)]
pub struct PercentileSet {
    /// 50th percentile (median).
    pub p50: FineDuration,

    /// 95th percentile.
    pub p95: FineDuration,

    /// 99th percentile.
    pub p99: FineDuration,

    /// 99.9th percentile.
    pub p99_9: FineDuration,

    /// 99.99th percentile.
    pub p99_99: FineDuration,
}
