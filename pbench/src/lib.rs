//! Library root.

#![deny(missing_docs)]
#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::perf)]
// Tested functions only
#![allow(clippy::inline_always)]

pub mod bencher;
pub mod config;
pub mod counter;
pub mod stats;
pub mod time;
