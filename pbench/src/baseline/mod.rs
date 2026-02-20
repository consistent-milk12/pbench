//! Baseline storage for benchmark results.
//!
//! Provides save/load functionality for benchmark baselines, enabling
//! comparison across runs. Baseline files are JSON, stored in
//! `target/pbench/baselines/<name>.json`.

mod regression;

#[cfg(test)]
mod unit_tests;

pub(crate) use regression::{RegressionChecker, RegressionResult};

use serde::{Deserialize, Serialize};
use serde_json as SJSON;

use crate::stats::PercentileStats;
use crate::time::fine_duration::FineDuration;

use std::error as StdError;
use std::fmt as StdFmt;
use std::fs as StdFs;
use std::io as StdIo;
use std::path::Path;
use std::path::PathBuf;

/// Current schema version for baseline files.
const SCHEMA_VERSION: u32 = 1;

/// Top-level wrapper for a baseline JSON file.
#[derive(Serialize, Deserialize)]
pub(crate) struct BaselineFile {
    /// Schema version for forward compatibility.
    schema_version: u32,

    /// Benchmark results stored in this baseline.
    results: Vec<BaselineEntry>,
}

/// A single benchmark result within a baseline file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct BaselineEntry {
    /// Benchmark name (fully qualified path).
    pub name: String,

    /// Serializable statistics snapshot.
    pub stats: SerializableStats,
}

/// Serializable version of [`PercentileStats`].
///
/// All duration fields are stored as `u64` picoseconds (same truncation
/// as the JSON renderer — benchmark durations never exceed ~213 days).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct SerializableStats {
    /// Number of samples collected.
    pub sample_count: u32,

    /// Total iterations across all samples.
    pub iter_count: u64,

    /// Minimum observed duration in picoseconds.
    pub min_picos: u64,

    /// Maximum observed duration in picoseconds.
    pub max_picos: u64,

    /// Arithmetic mean duration in picoseconds.
    pub mean_picos: u64,

    /// Population standard deviation in picoseconds.
    pub std_dev_picos: u64,

    /// 50th percentile in picoseconds.
    pub p50_picos: u64,

    /// 95th percentile in picoseconds.
    pub p95_picos: u64,

    /// 99th percentile in picoseconds.
    pub p99_picos: u64,

    /// 99.9th percentile in picoseconds.
    pub p99_9_picos: u64,

    /// 99.99th percentile in picoseconds.
    pub p99_99_picos: u64,
}

/// Errors that can occur when loading a baseline.
#[derive(Debug)]
pub(crate) enum BaselineError {
    /// Filesystem I/O error.
    Io(StdIo::Error),

    /// Baseline file has an incompatible schema version.
    IncompatibleVersion {
        /// Version found in the file.
        found: u32,

        /// Version expected by this build.
        expected: u32,
    },

    /// JSON parse error.
    Parse(SJSON::Error),
}

impl StdFmt::Display for BaselineError {
    fn fmt(&self, f: &mut StdFmt::Formatter<'_>) -> StdFmt::Result {
        match self {
            Self::Io(err) => write!(f, "baseline I/O error: {err}"),

            Self::IncompatibleVersion { found, expected } => {
                write!(
                    f,
                    "incompatible baseline schema version: found {found}, expected {expected}"
                )
            }

            Self::Parse(err) => write!(f, "baseline parse error: {err}"),
        }
    }
}

impl StdError::Error for BaselineError {
    fn source(&self) -> Option<&(dyn StdError::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),

            Self::IncompatibleVersion { .. } => None,

            Self::Parse(err) => Some(err),
        }
    }
}

impl From<StdIo::Error> for BaselineError {
    fn from(err: StdIo::Error) -> Self {
        Self::Io(err)
    }
}

impl From<SJSON::Error> for BaselineError {
    fn from(err: SJSON::Error) -> Self {
        Self::Parse(err)
    }
}

/// Baseline storage operations.
pub(crate) struct BaselineStore;

impl BaselineStore {
    /// Save benchmark results as a named baseline.
    ///
    /// Writes to `dir/<name>.json`, creating the directory if needed.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if directory creation or file writing fails.
    pub(crate) fn save(
        dir: &Path,
        name: &str,
        results: &[(&str, &PercentileStats)],
    ) -> StdIo::Result<()> {
        StdFs::create_dir_all(dir)?;

        let entries: Vec<BaselineEntry> = results
            .iter()
            .map(
                |&(bench_name, stats): &(&str, &PercentileStats)| BaselineEntry {
                    name: bench_name.to_owned(),
                    stats: SerializableStats::from_percentile_stats(stats),
                },
            )
            .collect();

        let file: BaselineFile = BaselineFile {
            schema_version: SCHEMA_VERSION,
            results: entries,
        };

        let json: String = SJSON::to_string_pretty(&file).expect("BaselineFile always serializes");

        let path: PathBuf = dir.join(format!("{name}.json"));
        StdFs::write(&path, json)?;

        Ok(())
    }

    /// Load a named baseline from disk.
    ///
    /// Reads from `dir/<name>.json` and validates the schema version.
    ///
    /// # Errors
    ///
    /// Returns [`BaselineError`] on I/O failure, version mismatch, or parse error.
    pub(crate) fn load(dir: &Path, name: &str) -> Result<Vec<BaselineEntry>, BaselineError> {
        let path: PathBuf = dir.join(format!("{name}.json"));
        let contents: String = StdFs::read_to_string(&path)?;
        let file: BaselineFile = SJSON::from_str(&contents)?;

        if file.schema_version != SCHEMA_VERSION {
            return Err(BaselineError::IncompatibleVersion {
                found: file.schema_version,
                expected: SCHEMA_VERSION,
            });
        }

        Ok(file.results)
    }
}

impl SerializableStats {
    /// Convert from [`PercentileStats`] to the serializable form.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "u128 picos -> u64: benchmark durations never exceed u64::MAX (~213 days)"
    )]
    #[must_use]
    pub(crate) const fn from_percentile_stats(stats: &PercentileStats) -> Self {
        Self {
            sample_count: stats.sample_count,
            iter_count: stats.iter_count,
            min_picos: stats.min.picos as u64,
            max_picos: stats.max.picos as u64,
            mean_picos: stats.mean.picos as u64,
            std_dev_picos: stats.std_dev.picos as u64,
            p50_picos: stats.percentiles.p50.picos as u64,
            p95_picos: stats.percentiles.p95.picos as u64,
            p99_picos: stats.percentiles.p99.picos as u64,
            p99_9_picos: stats.percentiles.p99_9.picos as u64,
            p99_99_picos: stats.percentiles.p99_99.picos as u64,
        }
    }

    /// Convert back to [`PercentileStats`].
    #[must_use]
    pub(crate) fn to_percentile_stats(&self) -> PercentileStats {
        PercentileStats {
            sample_count: self.sample_count,
            iter_count: self.iter_count,
            min: FineDuration {
                picos: u128::from(self.min_picos),
            },
            max: FineDuration {
                picos: u128::from(self.max_picos),
            },
            mean: FineDuration {
                picos: u128::from(self.mean_picos),
            },
            std_dev: FineDuration {
                picos: u128::from(self.std_dev_picos),
            },
            percentiles: crate::stats::PercentileSet {
                p50: FineDuration {
                    picos: u128::from(self.p50_picos),
                },
                p95: FineDuration {
                    picos: u128::from(self.p95_picos),
                },
                p99: FineDuration {
                    picos: u128::from(self.p99_picos),
                },
                p99_9: FineDuration {
                    picos: u128::from(self.p99_9_picos),
                },
                p99_99: FineDuration {
                    picos: u128::from(self.p99_99_picos),
                },
            },
        }
    }
}
