//! Core Percentile Computation Engine
//!
//! The current approach is to use sorted arrays and linear interpolation.

pub(crate) mod exact;

use crate::time::FineDuration;

/// Complete percentile stats for a benchmark run.
#[derive(Clone, Debug)]
pub(crate) struct PercentileStats {
    pub sample_count: u32,
    pub iter_count: u64,
    pub min: FineDuration,
    pub max: FineDuration,
    pub mean: FineDuration,
    pub std_dev: FineDuration,
    pub percentiles: PercentileSet,
}

/// The five standard percentile breakpoints.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PercentileSet {
    pub p50: FineDuration,
    pub p95: FineDuration,
    pub p99: FineDuration,
    pub p99_9: FineDuration,
    pub p99_99: FineDuration,
}
