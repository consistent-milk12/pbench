//! Terminal table output with tree-structured benchmark names and percentile columns.
//!
//! Renders benchmark results as an aligned table with `├─`/`╰─` tree
//! prefixes and six right-padded columns: p50, p95, p99, p99.9, p99.99, mean.
//!
//! The [`TablePainter`] receives pre-computed column widths and writes to
//! any [`std::io::Write`] destination. The runner computes widths
//! by pre-scanning all results before constructing the painter.
//!
//! NOTE: Design completely stolen from divan with some divergences to
//! adjust for the limited scope (no allocation tracking) and several
//! changes where I thought they were appropriate and matched my style.

use std::io::{self as StdIo, Write};
use std::iter as StdIter;

use crate::cli::BytesFormat;
use crate::counter::{CounterCollection, CounterKind};
use crate::stats::PercentileStats;
use crate::time::FineDuration;

use super::fmt::DisplayThroughput;

/// Column buffer spacing between name and first column.
const COL_BUF: usize = 2;

// =========================================================================
//  TableColumn
// =========================================================================

/// Columns displayed in the benchmark table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableColumn {
    /// 50th percentile (median).
    P50,

    /// 95th percentile.
    P95,

    /// 99th percentile.
    P99,

    /// 99.9th percentile.
    P99_9,

    /// 99.99th percentile.
    P99_99,

    /// Arithmetic mean.
    Mean,
}

impl TableColumn {
    /// Number of columns.
    pub(crate) const COUNT: usize = 6;

    /// All columns in display order.
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::P50,
        Self::P95,
        Self::P99,
        Self::P99_9,
        Self::P99_99,
        Self::Mean,
    ];

    /// Column header label.
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::P50 => "p50",

            Self::P95 => "p95",

            Self::P99 => "p99",

            Self::P99_9 => "p99.9",

            Self::P99_99 => "p99.99",

            Self::Mean => "mean",
        }
    }

    /// Extract the duration for this column from percentile stats.
    #[must_use]
    pub(crate) const fn get_stat(self, stats: &PercentileStats) -> FineDuration {
        match self {
            Self::P50 => stats.percentiles.p50,

            Self::P95 => stats.percentiles.p95,

            Self::P99 => stats.percentiles.p99,

            Self::P99_9 => stats.percentiles.p99_9,

            Self::P99_99 => stats.percentiles.p99_99,

            Self::Mean => stats.mean,
        }
    }
}

// =========================================================================
//  TableColumnData
// =========================================================================

#[derive(Default)]
struct TableColumnData<T>([T; TableColumn::COUNT]);

impl<T> TableColumnData<T> {
    /// Craete column data from a mapping function over all columns.
    fn from_fn<F>(f: F) -> Self
    where
        F: FnMut(TableColumn) -> T,
    {
        Self(TableColumn::ALL.map(f))
    }

    /// Create data with a value in the first column and defaults elsewhere.
    fn from_first(value: T) -> Self
    where
        Self: Default,
    {
        let mut data: Self = Self::default();
        data.0[0] = value;

        data
    }
}

impl TableColumnData<&str> {
    /// Write column values into `buf` with `│` separators and right-padding.
    ///
    /// Column widths are read-only. Each value is right-padded to the
    /// pre-computed width for its column.
    fn write(&self, buf: &mut String, column_widths: &[usize; TableColumn::COUNT]) {
        for (col_idx, value) in self.0.iter().enumerate() {
            let is_first: bool = col_idx == 0;
            let is_last: bool = col_idx == (TableColumn::COUNT - 1);
            let value_width: usize = value.chars().count();

            // Column Seperator
            if !is_first {
                let sep: &str = if is_last && (value_width == 0) {
                    " \u{2502}"
                } else {
                    " \u{2502} "
                };

                buf.push_str(sep);
            }

            buf.push_str(value);

            // Right-pad to pre-computed column width.
            if !is_last {
                let pad: usize = column_widths[col_idx].saturating_sub(value_width);

                buf.extend(StdIter::repeat_n(' ', pad));
            }
        }
    }
}

impl<T> TableColumnData<T> {
    /// Convert to a [`TableColumnData<&str>`] for writing.
    fn as_ref<U: ?Sized>(&self) -> TableColumnData<&U>
    where
        T: AsRef<U>,
    {
        TableColumnData::from_fn(|col: TableColumn| self.0[col as usize].as_ref())
    }
}

// =========================================================================
//  TablePainter
// =========================================================================

pub struct TablePainter<W: Write> {
    /// Output destination.
    writer: W,

    /// Maximum character span of name + tree prefix.
    max_name_span: usize,

    /// Maximum character widths (read-only after construction).
    column_widths: [usize; TableColumn::COUNT],

    /// Current tree nesting depth
    depth: usize,

    /// Accumalated tree prefix string for current depth
    current_prefix: String,

    /// Stack of `current_prefix` byte lengths for restoring state
    /// in [`finish_parent`](Self::finish_parent)
    prefix_stack: Vec<usize>,

    /// Reusable line buf.
    write_buf: String,
}

impl<W: Write> TablePainter<W> {
    /// Create a new table painter.
    ///
    /// `max_name_span` is the maximum character width of benchmark names
    /// including their tree prefixes. `column_widths` are the pre-computed
    /// maximum widths per column.
    pub(crate) const fn new(
        writer: W,
        max_name_span: usize,
        column_widths: [usize; TableColumn::COUNT],
    ) -> Self {
        Self {
            writer,
            max_name_span,
            column_widths,
            depth: 0,
            current_prefix: String::new(),
            prefix_stack: Vec::new(),
            write_buf: String::new(),
        }
    }

    /// Enter a parent node (module or benchmark group).
    ///
    /// Prints the group name with tree prefix. At the top level, also
    /// prints column headers.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if writing to the underlying writer fails.
    pub(crate) fn start_parent(&mut self, name: &str, is_last: bool) -> StdIo::Result<()> {
        let is_top_level: bool = self.depth == 0;
        let buf: &mut String = &mut self.write_buf;
        buf.clear();

        let branch: &str = if is_top_level {
            ""
        } else if !is_last {
            "\u{251c}\u{2500} "
        } else {
            "\u{2570}\u{2500} "
        };

        buf.push_str(&self.current_prefix);
        buf.push_str(branch);
        buf.push_str(name);

        // Right-pad name
        Self::right_pad(buf, self.max_name_span);

        // Column headers for top-level parent.
        if is_top_level {
            let headers: TableColumnData<&str> =
                TableColumnData::from_fn(|col: TableColumn| col.name());
            headers.write(buf, &self.column_widths);
        } else {
            // Empty column spacers for nested parents.
            TableColumnData([""; TableColumn::COUNT]).write(buf, &self.column_widths);
        }

        buf.push('\n');
        self.writer.write_all(buf.as_bytes())?;

        // Save prefix length before pushing new segment.
        self.prefix_stack.push(self.current_prefix.len());
        self.depth += 1;

        if !is_top_level {
            self.current_prefix
                .push_str(if is_last { "   " } else { "\u{2502}  " });
        }

        Ok(())
    }

    /// Exit the current parent node.
    ///
    /// # Errors
    ///
    /// Returns [`StdIo::Error`] if writing to the underlying writer fails.
    ///
    /// # Panics
    ///
    /// `debug_assert`'s that `depth > 0` to catch unbalanced calls.
    pub(crate) fn finish_parent(&mut self) -> StdIo::Result<()> {
        debug_assert!(
            self.depth > 0,
            "finish_parent called with depth 0 (unbalanced calls)"
        );
        self.depth -= 1;

        // Blank line after top-level groups readability.
        if self.depth == 0 {
            self.writer.write_all(b"\n")?;
        }

        // Restore prefix to previous length via stack
        if let Some(prev_len) = self.prefix_stack.pop() {
            self.current_prefix.truncate(prev_len);
        }

        Ok(())
    }

    /// Write a benchmark leaf with timing statistics.
    ///
    /// If `counter` is provided, a second throughput row is written below
    /// the timing row.
    ///
    /// # Errors
    ///
    /// Returns [`StdIo::Error`] if writing to the underlying writer fails.
    pub(crate) fn write_leaf(
        &mut self,
        name: &str,
        stats: &PercentileStats,
        counter: Option<(&CounterCollection, BytesFormat)>,
        is_last: bool,
    ) -> StdIo::Result<()> {
        let buf: &mut String = &mut self.write_buf;
        buf.clear();

        // Name with tree prefix.
        let branch: &str = if is_last {
            "\u{2570}\u{2500} "
        } else {
            "\u{251c}\u{2500} "
        };

        buf.push_str(&self.current_prefix);
        buf.push_str(branch);
        buf.push_str(name);

        Self::right_pad(buf, self.max_name_span);

        // Timing columns.
        let timing: TableColumnData<String> =
            TableColumnData::from_fn(|col: TableColumn| col.get_stat(stats).to_string());

        timing.as_ref::<str>().write(buf, &self.column_widths);
        buf.push('\n');

        self.writer.write_all(buf.as_bytes())?;

        // Throughput row (if counter attached).
        if let Some((collection, bytes_format)) = counter
            && let Some(mean_count) = collection.mean_count()
        {
            self.write_throughput_row(stats, collection.kind(), mean_count, bytes_format, is_last)?;
        }

        Ok(())
    }

    /// Write a throughput row below a timing row.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if writing to the underlying writer fails.
    #[expect(
        clippy::cast_precision_loss,
        reason = "Picos-to-f64 conversion is fine for throughput display"
    )]
    fn write_throughput_row(
        &mut self,
        stats: &PercentileStats,
        kind: CounterKind,
        mean_count: f64,
        bytes_format: BytesFormat,
        is_last: bool,
    ) -> StdIo::Result<()> {
        let buf: &mut String = &mut self.write_buf;
        buf.clear();

        // Continuation prefix.
        buf.push_str(&self.current_prefix);

        if !is_last {
            buf.push('\u{2502}');
        }

        Self::right_pad(buf, self.max_name_span);

        // Throughput per column.
        let throughput: TableColumnData<String> = TableColumnData::from_fn(|col: TableColumn| {
            let picos: f64 = col.get_stat(stats).picos as f64;
            let dt: DisplayThroughput = DisplayThroughput {
                kind,
                count: mean_count,
                picos,
                bytes_format,
            };

            dt.to_string()
        });

        throughput.as_ref::<str>().write(buf, &self.column_widths);
        buf.push('\n');

        self.writer.write_all(buf.as_bytes())?;

        Ok(())
    }

    /// Write an ignored benchmark leaf.
    ///
    /// # Errors
    ///
    /// Returns [`StdIo::Error`] if writing to the underlying writer fails.
    pub(crate) fn write_ignored_leaf(&mut self, name: &str, is_last: bool) -> StdIo::Result<()> {
        let buf: &mut String = &mut self.write_buf;
        buf.clear();

        let branch: &str = if is_last {
            "\u{2570}\u{2500} "
        } else {
            "\u{251c}\u{2500} "
        };

        buf.push_str(&self.current_prefix);
        buf.push_str(branch);
        buf.push_str(name);

        Self::right_pad(buf, self.max_name_span);

        TableColumnData::from_first("(ignored)").write(buf, &self.column_widths);
        buf.push('\n');

        self.writer.write_all(buf.as_bytes())?;

        Ok(())
    }

    /// Write a comparison row with delta percentages.
    ///
    /// TODO: Currently a no-op stub, implement later with
    /// baseline regression detection. Returns `Ok(())` so callers
    /// can wire it into the output pipeline without panics.
    ///
    /// # Errors
    ///
    /// Returns [`StdIo::Error`] if writing to the underlying writer fails
    /// (once implemented).
    #[allow(dead_code, reason = "Stub for baseline comparison display")]
    #[expect(clippy::pedantic, clippy::nursery)]
    pub(crate) const fn write_comparison_row(
        &mut self,
        _name: &str,
        _current: &PercentileStats,
        _baseline: &PercentileStats,
        _threshold_pct: f64,
        _is_last: bool,
    ) -> StdIo::Result<()> {
        Ok(())
    }

    /// Right-pad `buf` to `max_span + COL_BUF` characters.
    #[inline]
    fn right_pad(buf: &mut String, max_name_span: usize) {
        let buf_len: usize = buf.chars().count();
        let pad_len: usize = COL_BUF + max_name_span.saturating_sub(buf_len);

        buf.extend(StdIter::repeat_n(' ', pad_len));
    }
}

#[cfg(test)]
mod unit_tests;
