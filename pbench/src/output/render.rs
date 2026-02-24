//! Table rendering logic: tree-to-table mapping, column width computation,
//! and recursive tree painting.
//!
//! Extracted from `runner.rs` to separate orchestration (runner) from
//! presentation (render). The [`TableRenderer`] unit struct provides all
//! methods as associated functions.

use std::collections::HashMap;
use std::io::{self as StdIo, Write};

use crate::cli::BytesFormat;
use crate::counter::CounterCollection;
use crate::entry::AnyBenchEntry;
use crate::entry::tree::{EntryTree, push_path_component};
use crate::output::fmt::DisplayThroughput;
use crate::output::table::{TableColumn, TablePainter};
use crate::stats::PercentileStats;
use crate::time::FineDuration;

/// A single benchmark result record with name, thread count, and stats.
pub(crate) struct BenchRecord {
    /// Fully qualified benchmark name.
    pub name: String,

    /// Thread count used for this run.
    pub thread_count: u32,

    /// Timing statistics.
    pub stats: PercentileStats,

    /// Optional counter data for throughput display.
    pub counter: Option<CounterCollection>,
}

/// Table rendering engine.
///
/// All methods are associated functions on this unit struct.
/// Receives a tree and records, computes layout, and writes
/// the formatted table to stdout.
pub(crate) struct TableRenderer;

impl TableRenderer {
    /// Build a name-to-records index for O(1) lookup during rendering.
    pub(crate) fn build_record_index(records: &[BenchRecord]) -> HashMap<&str, Vec<usize>> {
        let mut index: HashMap<&str, Vec<usize>> = HashMap::new();

        for (i, record) in records.iter().enumerate() {
            index.entry(record.name.as_str()).or_default().push(i);
        }

        index
    }

    /// Collect record indices for a generic entry by prefix-matching
    /// `"{base_name}::"`. Returns `(arg_suffix, indices)` pairs in
    /// insertion order (matching the order of `ge.args`).
    fn collect_generic_indices<'a>(
        base_name: &str,
        records: &'a [BenchRecord],
    ) -> Vec<(&'a str, Vec<usize>)> {
        let prefix: String = format!("{base_name}::");
        let mut groups: Vec<(&'a str, Vec<usize>)> = Vec::new();

        for (i, record) in records.iter().enumerate() {
            if let Some(suffix) = record.name.strip_prefix(prefix.as_str()) {
                if let Some(entry) = groups
                    .iter_mut()
                    .find(|(a, _): &&mut (&str, Vec<usize>)| *a == suffix)
                {
                    entry.1.push(i);
                } else {
                    groups.push((suffix, vec![i]));
                }
            }
        }

        groups
    }

    /// Render results as a terminal table.
    ///
    /// # Panics
    ///
    /// Panics if writing to stdout fails.
    #[expect(
        clippy::cast_precision_loss,
        reason = "Picos-to-f64 conversion is fine for throughput width measurement"
    )]
    pub(crate) fn render(
        tree: &[EntryTree],
        records: &[BenchRecord],
        bytes_format: BytesFormat,
        baseline: Option<&[(String, u32, PercentileStats)]>,
        threshold_pct: f64,
    ) {
        // Pre-compute column widths by scanning all results.
        let mut column_widths: [usize; TableColumn::COUNT] =
            TableColumn::ALL.map(|col: TableColumn| col.name().chars().count());

        let mut max_name_span: usize = 0;

        // Build record index for O(1) lookups.
        let index: HashMap<&str, Vec<usize>> = Self::build_record_index(records);

        // Reusable path buffer for tree traversal.
        let mut buf: String = String::new();

        // Walk tree to compute max name span (name + prefix chars).
        Self::compute_name_spans(tree, 0, &mut buf, records, &index, &mut max_name_span);

        // Compute column widths from timing and throughput results.
        for record in records {
            for (i, col) in TableColumn::ALL.iter().enumerate() {
                // Timing width.
                let dur: FineDuration = col.get_stat(&record.stats);
                let width: usize = dur.to_string().chars().count();

                if width > column_widths[i] {
                    column_widths[i] = width;
                }

                // Throughput width (if counter attached).
                if let Some(ref collection) = record.counter
                    && let Some(mean_count) = collection.mean_count()
                {
                    let picos: f64 = col.get_stat(&record.stats).picos as f64;
                    let dt: DisplayThroughput = DisplayThroughput {
                        kind: collection.kind(),
                        count: mean_count,
                        picos,
                        bytes_format,
                    };

                    let tp_width: usize = dt.to_string().chars().count();

                    if tp_width > column_widths[i] {
                        column_widths[i] = tp_width;
                    }
                }
            }
        }

        let stdout: StdIo::Stdout = StdIo::stdout();
        let out: StdIo::BufWriter<StdIo::Stdout> = StdIo::BufWriter::new(stdout);
        let mut painter: TablePainter<StdIo::BufWriter<StdIo::Stdout>> =
            TablePainter::new(out, max_name_span, column_widths);

        // buf is empty after compute_name_spans (all truncates restore to 0).
        Self::paint_tree(
            &mut painter,
            tree,
            &mut buf,
            records,
            &index,
            bytes_format,
            baseline,
            threshold_pct,
        )
        .expect("write table");
    }

    /// Compute maximum name span including tree prefix characters.
    fn compute_name_spans(
        nodes: &[EntryTree],
        depth: usize,
        buf: &mut String,
        records: &[BenchRecord],
        index: &HashMap<&str, Vec<usize>>,
        max_span: &mut usize,
    ) {
        for node in nodes {
            let name: &str = node.raw_name();
            let name_chars: usize = name.chars().count();
            // Tree prefix adds 3 chars per nesting level ("├─ " or "╰─ ").
            let span: usize = depth * 3 + name_chars;

            if span > *max_span {
                *max_span = span;
            }

            match node {
                EntryTree::Parent { children, .. } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, name);

                    Self::compute_name_spans(children, depth + 1, buf, records, index, max_span);

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );
                    buf.truncate(saved);
                }

                EntryTree::Leaf { entry } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, name);

                    if matches!(entry, AnyBenchEntry::Generic(_)) {
                        // Generic: match by prefix, account for arg children.
                        let arg_groups: Vec<(&str, Vec<usize>)> =
                            Self::collect_generic_indices(buf, records);

                        if arg_groups.is_empty() {
                            // No records — will show as "(ignored)".
                        } else if arg_groups.len() == 1 && arg_groups[0].1.len() == 1 {
                            // Single arg, single thread — paint_tree renders
                            // this as a flat leaf with display name "name::arg".
                            let (arg, _) = &arg_groups[0];
                            let flat_name: String = format!("{name}::{arg}");
                            let flat_span: usize = depth * 3 + flat_name.chars().count();

                            if flat_span > *max_span {
                                *max_span = flat_span;
                            }
                        } else {
                            // Multiple args or multiple threads — nested.
                            for (arg, indices) in &arg_groups {
                                let arg_chars: usize = arg.chars().count();
                                let arg_span: usize = (depth + 1) * 3 + arg_chars;

                                if arg_span > *max_span {
                                    *max_span = arg_span;
                                }

                                if indices.len() > 1 {
                                    // Multiple thread counts under this arg.
                                    let max_tc: u32 = indices
                                        .iter()
                                        .map(|&i: &usize| records[i].thread_count)
                                        .max()
                                        .unwrap_or(1);
                                    let t_label_chars: usize =
                                        format!("t={max_tc}").chars().count();
                                    let t_span: usize = (depth + 2) * 3 + t_label_chars;

                                    if t_span > *max_span {
                                        *max_span = t_span;
                                    }
                                }
                            }
                        }
                    } else {
                        // Non-generic: exact match by full name.
                        let count: usize =
                            index.get(buf.as_str()).map_or(0, |v: &Vec<usize>| v.len());

                        if count > 1 {
                            // Multiple thread counts — will render t=N children.
                            let max_tc: u32 = index[buf.as_str()]
                                .iter()
                                .map(|&i: &usize| records[i].thread_count)
                                .max()
                                .unwrap_or(1);
                            let t_label_chars: usize = format!("t={max_tc}").chars().count();
                            let t_span: usize = (depth + 1) * 3 + t_label_chars;

                            if t_span > *max_span {
                                *max_span = t_span;
                            }
                        }
                    }

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );
                    buf.truncate(saved);
                }
            }
        }
    }

    /// Paint `t=N` child leaves for a set of record indices, sorted by
    /// thread count.
    fn paint_thread_children<W: Write>(
        painter: &mut TablePainter<W>,
        indices: &[usize],
        records: &[BenchRecord],
        bytes_format: BytesFormat,
        baseline: Option<&[(String, u32, PercentileStats)]>,
        threshold_pct: f64,
        full_name: &str,
    ) -> StdIo::Result<()> {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_by_key(|&i: &usize| records[i].thread_count);

        for (j, &idx) in sorted.iter().enumerate() {
            let rec: &BenchRecord = &records[idx];
            let t_label: String = format!("t={}", rec.thread_count);
            let t_is_last: bool = j == sorted.len() - 1;
            let counter_arg: Option<(&CounterCollection, BytesFormat)> = rec
                .counter
                .as_ref()
                .map(|c: &CounterCollection| (c, bytes_format));

            painter.write_leaf(&t_label, &rec.stats, counter_arg, t_is_last)?;

            // Baseline comparison row for this thread count.
            if let Some(bl) = baseline {
                let match_entry: Option<&(String, u32, PercentileStats)> =
                    bl.iter()
                        .find(|(n, tc, _): &&(String, u32, PercentileStats)| {
                            n == full_name && *tc == rec.thread_count
                        });

                if let Some((_, _, old_stats)) = match_entry {
                    painter.write_comparison_row(
                        &rec.stats,
                        old_stats,
                        threshold_pct,
                        t_is_last,
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Paint the tree using `TablePainter`, matching records to leaves by
    /// fully qualified name.
    #[expect(
        clippy::too_many_lines,
        reason = "Tree traversal with multiple entry types"
    )]
    #[expect(
        clippy::too_many_arguments,
        reason = "Internal recursive renderer needs full context"
    )]
    fn paint_tree<W: Write>(
        painter: &mut TablePainter<W>,
        nodes: &[EntryTree],
        buf: &mut String,
        records: &[BenchRecord],
        index: &HashMap<&str, Vec<usize>>,
        bytes_format: BytesFormat,
        baseline: Option<&[(String, u32, PercentileStats)]>,
        threshold_pct: f64,
    ) -> StdIo::Result<()> {
        for (i, node) in nodes.iter().enumerate() {
            let is_last: bool = i == nodes.len() - 1;

            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, raw_name);

                    painter.start_parent(raw_name, is_last)?;
                    Self::paint_tree(
                        painter,
                        children,
                        buf,
                        records,
                        index,
                        bytes_format,
                        baseline,
                        threshold_pct,
                    )?;
                    painter.finish_parent()?;

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );
                    buf.truncate(saved);
                }

                EntryTree::Leaf { entry } => {
                    let name: &str = entry.raw_name();
                    let saved: usize = buf.len();
                    push_path_component(buf, name);

                    if matches!(entry, AnyBenchEntry::Generic(_)) {
                        // Generic entry: match by prefix, render arg children.
                        let arg_groups: Vec<(&str, Vec<usize>)> =
                            Self::collect_generic_indices(buf, records);

                        if arg_groups.is_empty() {
                            painter.write_ignored_leaf(name, is_last)?;
                        } else if arg_groups.len() == 1 && arg_groups[0].1.len() == 1 {
                            // Single arg, single thread — render as simple leaf.
                            let (arg, ref indices) = arg_groups[0];
                            let rec: &BenchRecord = &records[indices[0]];
                            let display_name: String = format!("{name}::{arg}");
                            let counter_arg: Option<(&CounterCollection, BytesFormat)> = rec
                                .counter
                                .as_ref()
                                .map(|c: &CounterCollection| (c, bytes_format));

                            painter.write_leaf(&display_name, &rec.stats, counter_arg, is_last)?;

                            // Baseline comparison for single-arg generic.
                            let saved2: usize = buf.len();
                            buf.push_str("::");
                            buf.push_str(arg);

                            if let Some(bl) = baseline {
                                let match_entry: Option<&(String, u32, PercentileStats)> = bl
                                    .iter()
                                    .find(|(n, tc, _): &&(String, u32, PercentileStats)| {
                                        n.as_str() == buf.as_str() && *tc == rec.thread_count
                                    });

                                if let Some((_, _, old_stats)) = match_entry {
                                    painter.write_comparison_row(
                                        &rec.stats,
                                        old_stats,
                                        threshold_pct,
                                        is_last,
                                    )?;
                                }
                            }

                            debug_assert!(
                                buf.len() >= saved2,
                                "buffer was modified beyond truncate point"
                            );
                            buf.truncate(saved2);
                        } else {
                            // Multiple args or multiple thread counts — nest.
                            painter.start_parent(name, is_last)?;

                            for (ai, (arg, indices)) in arg_groups.iter().enumerate() {
                                let arg_is_last: bool = ai == arg_groups.len() - 1;
                                let saved2: usize = buf.len();
                                buf.push_str("::");
                                buf.push_str(arg);

                                if indices.len() == 1 {
                                    // Single thread for this arg — flat leaf.
                                    let rec: &BenchRecord = &records[indices[0]];
                                    let counter_arg: Option<(&CounterCollection, BytesFormat)> =
                                        rec.counter
                                            .as_ref()
                                            .map(|c: &CounterCollection| (c, bytes_format));

                                    painter.write_leaf(
                                        arg,
                                        &rec.stats,
                                        counter_arg,
                                        arg_is_last,
                                    )?;

                                    // Baseline comparison.
                                    if let Some(bl) = baseline {
                                        let match_entry: Option<&(String, u32, PercentileStats)> =
                                            bl.iter().find(
                                                |(n, tc, _): &&(String, u32, PercentileStats)| {
                                                    n.as_str() == buf.as_str()
                                                        && *tc == rec.thread_count
                                                },
                                            );

                                        if let Some((_, _, old_stats)) = match_entry {
                                            painter.write_comparison_row(
                                                &rec.stats,
                                                old_stats,
                                                threshold_pct,
                                                arg_is_last,
                                            )?;
                                        }
                                    }
                                } else {
                                    // Multiple threads — nest t=N under arg.
                                    painter.start_parent(arg, arg_is_last)?;
                                    Self::paint_thread_children(
                                        painter,
                                        indices,
                                        records,
                                        bytes_format,
                                        baseline,
                                        threshold_pct,
                                        buf,
                                    )?;
                                    painter.finish_parent()?;
                                }

                                debug_assert!(
                                    buf.len() >= saved2,
                                    "buffer was modified beyond truncate point"
                                );
                                buf.truncate(saved2);
                            }

                            painter.finish_parent()?;
                        }
                    } else {
                        // Non-generic: exact match by full name via index.
                        let indices: Option<&Vec<usize>> = index.get(buf.as_str());

                        match indices {
                            None => {
                                painter.write_ignored_leaf(name, is_last)?;
                            }

                            Some(v) if v.len() == 1 => {
                                // Single thread count — render as before.
                                let rec: &BenchRecord = &records[v[0]];
                                let counter_arg: Option<(&CounterCollection, BytesFormat)> = rec
                                    .counter
                                    .as_ref()
                                    .map(|c: &CounterCollection| (c, bytes_format));

                                painter.write_leaf(name, &rec.stats, counter_arg, is_last)?;

                                // Baseline comparison.
                                if let Some(bl) = baseline {
                                    let match_entry: Option<&(String, u32, PercentileStats)> = bl
                                        .iter()
                                        .find(|(n, tc, _): &&(String, u32, PercentileStats)| {
                                            n.as_str() == buf.as_str() && *tc == rec.thread_count
                                        });

                                    if let Some((_, _, old_stats)) = match_entry {
                                        painter.write_comparison_row(
                                            &rec.stats,
                                            old_stats,
                                            threshold_pct,
                                            is_last,
                                        )?;
                                    }
                                }
                            }

                            Some(v) => {
                                // Multiple thread counts — render parent + t=N.
                                painter.start_parent(name, is_last)?;
                                Self::paint_thread_children(
                                    painter,
                                    v,
                                    records,
                                    bytes_format,
                                    baseline,
                                    threshold_pct,
                                    buf,
                                )?;
                                painter.finish_parent()?;
                            }
                        }
                    }

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );
                    buf.truncate(saved);
                }
            }
        }

        Ok(())
    }
}
