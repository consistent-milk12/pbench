//! Output formatting for benchmark results.
//!
//! Provides terminal table rendering ([`table`]), throughput display
//! formatting ([`fmt`]), and machine-readable output ([`json`], [`csv`])
//! for benchmark statistics.

pub mod fmt;
pub mod table;

#[cfg(feature = "json")]
pub(crate) mod json;

pub mod csv;
