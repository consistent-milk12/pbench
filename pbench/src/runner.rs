//! Benchmark runner: discovery, filtering, execution, and output.
//!
//! The [`Runner`] collects all registered benchmark entries, builds an
//! [`EntryTree`], applies CLI filters and sort order, then dispatches to
//! the appropriate action: list, test, or full bench with output rendering.

use std::io::{self as StdIo, Write};
use std::panic as StdPanic;
use std::thread as StdThread;

use crate::bencher::{BenchContext, Bencher};
use crate::cli::{BytesFormat, CliArgs, OutputFormat};
use crate::config::{BenchOptions, ResolvedBenchOptions};
use crate::counter::CounterCollection;
use crate::entry::{AnyBenchEntry, BENCH_ENTRIES, EntryTree, GroupEntry};
use crate::output::csv::CsvRenderer;
use crate::output::fmt::DisplayThroughput;
use crate::output::table::{TableColumn, TablePainter};
use crate::stats::PercentileStats;
use crate::time::{FineDuration, Timer};

/// Action to take after tree construction and filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunAction {
    /// Print benchmark names with tree structure.
    List,

    /// Print benchmark names one per line (nextest-compatible).
    ListTerse,

    /// Run each benchmark once without timing (verification mode).
    Test,

    /// Full benchmark execution with timing and output.
    Bench,
}

/// Collected result for a single benchmark.
struct BenchRecord {
    /// Fully qualified benchmark name.
    name: String,

    /// Timing statistics.
    stats: PercentileStats,

    /// Optional counter data for throughput display.
    counter: Option<CounterCollection>,
}

/// Benchmark runner.
///
/// Holds CLI arguments and the determined action. Constructed via
/// [`Runner::new`], executed via [`Runner::run`].
pub struct Runner {
    /// Parsed CLI arguments.
    args: CliArgs,

    /// Action determined from CLI flags.
    action: RunAction,
}

impl Runner {
    /// Create a new runner from parsed CLI arguments.
    #[must_use]
    pub(crate) const fn new(args: CliArgs) -> Self {
        let action: RunAction = if args.list_terse {
            RunAction::ListTerse
        } else if args.list {
            RunAction::List
        } else if args.test {
            RunAction::Test
        } else {
            RunAction::Bench
        };

        Self { args, action }
    }

    /// Override sample count (builder API for programmatic config).
    #[must_use]
    #[expect(dead_code, reason = "Programmatic builder API for library consumers")]
    pub(crate) const fn sample_count(mut self, n: u32) -> Self {
        self.args.sample_count = Some(n);

        self
    }

    /// Override minimum benchmarking time (builder API).
    #[must_use]
    #[expect(dead_code, reason = "Programmatic builder API for library consumers")]
    pub(crate) const fn min_time(mut self, d: std::time::Duration) -> Self {
        self.args.sample_size = None; // adaptive when min_time overridden
        // Store as a CLI overridem, will be applied during option resolution.
        // For now, min_time override is handled through sample_count/sample_size.
        // Full Duration-based override requires extending CliArgs (deferred).
        let _ = d;

        self
    }

    /// Main execution flow.
    ///
    /// # Panics
    ///
    /// Panics if writing to stdout/stderr fails.
    pub(crate) fn run(&self) {
        // 1. Collect all entries.
        let entries: Vec<AnyBenchEntry> = BENCH_ENTRIES.iter().copied().collect();

        if entries.is_empty() && self.action == RunAction::Bench {
            eprintln!("pbench: no benchmarks found");
            return;
        }

        // 2. Build entry tree.
        let mut tree: Vec<EntryTree> = EntryTree::from_entries(&entries);

        // 3. Filter phase.
        self.apply_filters(&mut tree);

        // 4. Sort phase.
        EntryTree::sort(&mut tree, self.args.sort);

        // 5. Action dispatch.
        match self.action {
            RunAction::List => self.action_list(&tree),
            RunAction::ListTerse => self.action_list_terse(&tree),
            RunAction::Test => self.action_test(&tree),
            RunAction::Bench => self.action_bench(&tree),
        }
    }

    // =====================================================================
    //  Filter
    // =====================================================================

    /// Apply inclusive/exclusive filters and `RunIgnored` to the tree.
    fn apply_filters(&self, tree: &mut Vec<EntryTree>) {
        let filter: &Option<String> = &self.args.filter;
        let skip: &[String] = &self.args.skip;

        EntryTree::retain(tree, |path: &str| {
            // Exclusive filters take priority.
            for skip_pattern in skip {
                if path.contains(skip_pattern.as_str()) {
                    return false;
                }
            }

            // Inclusive filter.
            if let Some(f) = filter
                && !path.contains(f.as_str())
            {
                return false;
            }

            true
        });

        // RunIgnored filtering happens at execution time in run_single_entry
        // since it requires access to each entry's resolved options (the tree
        // retain closure only receives path strings, not entry objects).
    }

    // =====================================================================
    //  List actions
    // =====================================================================

    /// Print benchmark names with tree structure.
    ///
    /// # Panics
    ///
    /// Panics if writing to stdout fails.
    #[expect(
        clippy::unused_self,
        reason = "method logically belongs to Runner instance"
    )]
    fn action_list(&self, tree: &[EntryTree]) {
        let stdout: StdIo::Stdout = StdIo::stdout();
        let mut out: StdIo::BufWriter<StdIo::Stdout> = StdIo::BufWriter::new(stdout);

        Self::print_tree(&mut out, tree, "", true);

        out.flush().expect("flush stdout");
    }

    /// Recursively print tree with indentation.
    fn print_tree<W: Write>(out: &mut W, nodes: &[EntryTree], prefix: &str, is_root: bool) {
        for (i, node) in nodes.iter().enumerate() {
            let is_last: bool = i == nodes.len() - 1;

            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    if is_root {
                        writeln!(out, "{raw_name}").expect("write");
                        Self::print_tree(out, children, "", false);
                    } else {
                        let branch: &str = if is_last { "╰─ " } else { "├─ " };
                        writeln!(out, "{prefix}{branch}{raw_name}").expect("write");

                        let child_prefix: String = if is_last {
                            format!("{prefix}   ")
                        } else {
                            format!("{prefix}│  ")
                        };

                        Self::print_tree(out, children, &child_prefix, false);
                    }
                }

                EntryTree::Leaf { entry } => {
                    let name: &str = entry.raw_name();

                    if is_root {
                        writeln!(out, "{name}").expect("write");
                    } else {
                        let branch: &str = if is_last { "╰─ " } else { "├─ " };
                        writeln!(out, "{prefix}{branch}{name}").expect("write");
                    }
                }
            }
        }
    }

    /// Print benchmark names one per line (nextest-compatible).
    ///
    /// # Panics
    ///
    /// Panics if writing to stdout fails.
    #[expect(
        clippy::unused_self,
        reason = "method logically belongs to Runner instance"
    )]
    fn action_list_terse(&self, tree: &[EntryTree]) {
        let stdout: StdIo::Stdout = StdIo::stdout();
        let mut out: StdIo::BufWriter<StdIo::Stdout> = StdIo::BufWriter::new(stdout);

        Self::collect_names(tree, "", &mut |name: &str| {
            writeln!(out, "{name}").expect("write");
        });

        out.flush().expect("flush stdout");
    }

    /// Collect fully qualified benchmark names by traversing the tree.
    fn collect_names(nodes: &[EntryTree], parent_path: &str, emit: &mut dyn FnMut(&str)) {
        for node in nodes {
            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    let path: String = if parent_path.is_empty() {
                        (*raw_name).to_owned()
                    } else {
                        format!("{parent_path}::{raw_name}")
                    };

                    Self::collect_names(children, &path, emit);
                }

                EntryTree::Leaf { entry } => {
                    let name: &str = entry.raw_name();
                    let full_name: String = if parent_path.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{parent_path}::{name}")
                    };

                    emit(&full_name);
                }
            }
        }
    }

    // =====================================================================
    //  Test action
    // =====================================================================

    /// Run each benchmark once without timing (verification mode).
    ///
    /// # Panics
    ///
    /// Panics if writing to stderr fails.
    #[expect(
        clippy::unused_self,
        reason = "method logically belongs to Runner instance"
    )]
    fn action_test(&self, tree: &[EntryTree]) {
        let mut pass_count: u32 = 0;
        let mut fail_count: u32 = 0;

        Self::visit_entries(tree, "", &mut |name: &str, entry: &AnyBenchEntry| {
            eprint!("test {name} ... ");

            let result: StdThread::Result<()> =
                StdPanic::catch_unwind(StdPanic::AssertUnwindSafe(|| {
                    // Minimal options: 1 sample, 1 iteration.
                    let opts: BenchOptions = BenchOptions {
                        sample_count: Some(1),
                        sample_size: Some(1),
                        ..BenchOptions::default()
                    };
                    let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&opts);
                    let ctx: BenchContext = BenchContext::new(resolved);
                    let bencher: Bencher<'_> = Bencher::new(&ctx);

                    match entry {
                        AnyBenchEntry::Bench(be) => (be.bench_fn)(&bencher),

                        AnyBenchEntry::Generic(ge) => {
                            if let Some(&first_arg) = ge.args.first() {
                                (ge.bench_fn)(&bencher, first_arg);
                            }
                        }

                        AnyBenchEntry::Group(_) => {} // groups are not runnable
                    }
                }));

            if result.is_ok() {
                eprintln!("ok");
                pass_count += 1;
            } else {
                eprintln!("FAILED");
                fail_count += 1;
            }
        });

        eprintln!();
        eprintln!(
            "test result: {}. {pass_count} passed; {fail_count} failed",
            if fail_count == 0 { "ok" } else { "FAILED" }
        );

        if fail_count > 0 {
            std::process::exit(1);
        }
    }

    // =====================================================================
    //  Bench action
    // =====================================================================

    /// Full benchmark execution with timing and output.
    ///
    /// # Panics
    ///
    /// Panics if output writing fails.
    fn action_bench(&self, tree: &[EntryTree]) {
        let timer: Timer = Timer::best_available();

        // Report timer info to stderr.
        eprintln!(
            "pbench: timer={}, precision={}",
            Self::timer_name(&timer),
            timer.precision()
        );

        // Collect results.
        let mut records: Vec<BenchRecord> = Vec::new();

        Self::run_bench_tree(tree, "", None, &self.args, &mut records);

        if records.is_empty() {
            eprintln!("pbench: no benchmarks were executed");
            return;
        }

        // Build output slices.
        let result_pairs: Vec<(&str, &PercentileStats)> = records
            .iter()
            .map(|r: &BenchRecord| (r.name.as_str(), &r.stats))
            .collect();

        // Render output.
        match self.args.output_format {
            OutputFormat::Table => {
                Self::render_table(tree, &records, self.args.bytes_format);
            }

            OutputFormat::Json => {
                #[cfg(feature = "json")]
                {
                    use crate::output::json::JsonRenderer;

                    let json: String = JsonRenderer::render(&result_pairs);
                    println!("{json}");
                }

                #[cfg(not(feature = "json"))]
                {
                    eprintln!(
                        "pbench: JSON output requires the `json` feature. \
                         Recompile with `--features json`."
                    );

                    std::process::exit(1);
                }
            }

            OutputFormat::Csv => {
                let csv: String = CsvRenderer::render(&result_pairs);

                print!("{csv}");
            }
        }

        // Baseline save.
        #[cfg(feature = "json")]
        if let Some(ref baseline_name) = self.args.save_baseline {
            use std::path::PathBuf;

            use crate::baseline::BaselineStore;

            let dir: PathBuf = PathBuf::from("target/pbench/baselines");

            if let Err(e) = BaselineStore::save(&dir, baseline_name, &result_pairs) {
                eprintln!("pbench: failed to save baseline: {e}");
            } else {
                eprintln!("pbench: saved baseline '{baseline_name}'");
            }
        }

        // Baseline comparison.
        #[cfg(feature = "json")]
        if let Some(ref baseline_name) = self.args.baseline {
            Self::compare_baseline(baseline_name, &result_pairs, self.args.threshold);
        }

        #[cfg(not(feature = "json"))]
        {
            if self.args.save_baseline.is_some() || self.args.baseline.is_some() {
                eprintln!(
                    "pbench: baseline features require the `json` feature. \
                     Recompile with `--features json`."
                );

                std::process::exit(1);
            }
        }
    }

    /// Recursively run benchmarks in the tree, collecting results.
    ///
    /// `group` carries the nearest ancestor's group entry for option
    /// inheritance. It is updated when entering a Parent that has a
    /// group attached.
    fn run_bench_tree(
        nodes: &[EntryTree],
        parent_path: &str,
        group: Option<&'static GroupEntry>,
        args: &CliArgs,
        records: &mut Vec<BenchRecord>,
    ) {
        for node in nodes {
            match node {
                EntryTree::Parent {
                    raw_name,
                    children,
                    group: node_group,
                    ..
                } => {
                    let path: String = if parent_path.is_empty() {
                        (*raw_name).to_owned()
                    } else {
                        format!("{parent_path}::{raw_name}")
                    };

                    // Prefer this node's group; fall back to inherited group.
                    let active_group: Option<&'static GroupEntry> = node_group.or(group);

                    Self::run_bench_tree(children, &path, active_group, args, records);
                }

                EntryTree::Leaf { entry } => {
                    Self::run_single_entry(entry, parent_path, group, args, records);
                }
            }
        }
    }

    /// Run a single benchmark entry and collect its result.
    fn run_single_entry(
        entry: &AnyBenchEntry,
        parent_path: &str,
        group: Option<&'static GroupEntry>,
        args: &CliArgs,
        records: &mut Vec<BenchRecord>,
    ) {
        match entry {
            AnyBenchEntry::Bench(be) => {
                let full_name: String = if parent_path.is_empty() {
                    be.meta.raw_name.to_owned()
                } else {
                    format!("{parent_path}::{}", be.meta.raw_name)
                };

                // Resolve options: CLI -> entry -> group (CLI wins).
                let entry_opts: BenchOptions =
                    be.options.map_or_else(BenchOptions::default, |f| f());
                let group_opts: BenchOptions = group
                    .and_then(|g: &GroupEntry| g.options)
                    .map_or_else(BenchOptions::default, |f| f());
                let cli_opts: BenchOptions = Self::cli_to_bench_options(args);
                let merged: BenchOptions = cli_opts.overwrite(&entry_opts.overwrite(&group_opts));
                let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&merged);

                if !args.run_ignored.should_run(resolved.ignore) {
                    return;
                }

                let ctx: BenchContext = BenchContext::new(resolved);
                let bencher: Bencher<'_> = Bencher::new(&ctx);
                (be.bench_fn)(&bencher);

                if let Some(result) = ctx.finish() {
                    records.push(BenchRecord {
                        name: full_name,
                        stats: result.stats,
                        counter: result.counter,
                    });
                }
            }

            AnyBenchEntry::Generic(ge) => {
                for &arg in ge.args {
                    let full_name: String = if parent_path.is_empty() {
                        format!("{}::{arg}", ge.meta.raw_name)
                    } else {
                        format!("{parent_path}::{}::{arg}", ge.meta.raw_name)
                    };

                    let cli_opts: BenchOptions = Self::cli_to_bench_options(args);
                    let resolved: ResolvedBenchOptions =
                        ResolvedBenchOptions::from_options(&cli_opts);

                    let ctx: BenchContext = BenchContext::new(resolved);
                    let bencher: Bencher<'_> = Bencher::new(&ctx);
                    (ge.bench_fn)(&bencher, arg);

                    if let Some(result) = ctx.finish() {
                        records.push(BenchRecord {
                            name: full_name,
                            stats: result.stats,
                            counter: result.counter,
                        });
                    }
                }
            }

            AnyBenchEntry::Group(_) => {
                // Groups are structural, not runnable.
            }
        }
    }

    // =====================================================================
    //  Table rendering
    // =====================================================================

    /// Render results as a terminal table.
    ///
    /// # Panics
    ///
    /// Panics if writing to stdout fails.
    #[expect(
        clippy::cast_precision_loss,
        reason = "Picos-to-f64 conversion is fine for throughput width measurement"
    )]
    fn render_table(tree: &[EntryTree], records: &[BenchRecord], bytes_format: BytesFormat) {
        // Pre-compute column widths by scanning all results.
        let mut column_widths: [usize; TableColumn::COUNT] =
            TableColumn::ALL.map(|col: TableColumn| col.name().len());

        let mut max_name_span: usize = 0;

        // Walk tree to compute max name span (name + prefix chars).
        Self::compute_name_spans(tree, 0, &mut max_name_span);

        // Compute column widths from timing and throughput results.
        for record in records {
            for (i, col) in TableColumn::ALL.iter().enumerate() {
                // Timing width.
                let dur: FineDuration = col.get_stat(&record.stats);
                let width: usize = dur.to_string().len();

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

                    let tp_width: usize = dt.to_string().len();

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

        Self::paint_tree(&mut painter, tree, "", records, bytes_format).expect("write table");
    }

    /// Compute maximum name span including tree prefix characters.
    fn compute_name_spans(nodes: &[EntryTree], depth: usize, max_span: &mut usize) {
        for node in nodes {
            let name_len: usize = node.raw_name().len();
            // Tree prefix adds 3 chars per nesting level ("├─ " or "╰─ ").
            let span: usize = depth * 3 + name_len;

            if span > *max_span {
                *max_span = span;
            }

            if let EntryTree::Parent { children, .. } = node {
                Self::compute_name_spans(children, depth + 1, max_span);
            }
        }
    }

    /// Paint the tree using `TablePainter`, matching records to leaves by
    /// fully qualified name.
    fn paint_tree<W: Write>(
        painter: &mut TablePainter<W>,
        nodes: &[EntryTree],
        parent_path: &str,
        records: &[BenchRecord],
        bytes_format: BytesFormat,
    ) -> StdIo::Result<()> {
        for (i, node) in nodes.iter().enumerate() {
            let is_last: bool = i == nodes.len() - 1;

            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    let path: String = if parent_path.is_empty() {
                        (*raw_name).to_owned()
                    } else {
                        format!("{parent_path}::{raw_name}")
                    };

                    painter.start_parent(raw_name, is_last)?;
                    Self::paint_tree(painter, children, &path, records, bytes_format)?;
                    painter.finish_parent()?;
                }

                EntryTree::Leaf { entry } => {
                    let name: &str = entry.raw_name();
                    let full_name: String = if parent_path.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{parent_path}::{name}")
                    };

                    // Find matching record by full name.
                    let record: Option<&BenchRecord> =
                        records.iter().find(|r: &&BenchRecord| r.name == full_name);

                    if let Some(rec) = record {
                        let counter_arg: Option<(&CounterCollection, BytesFormat)> = rec
                            .counter
                            .as_ref()
                            .map(|c: &CounterCollection| (c, bytes_format));

                        painter.write_leaf(name, &rec.stats, counter_arg, is_last)?;
                    } else {
                        painter.write_ignored_leaf(name, is_last)?;
                    }
                }
            }
        }

        Ok(())
    }

    // =====================================================================
    //  Baseline comparison
    // =====================================================================

    /// Compare current results against a saved baseline.
    #[cfg(feature = "json")]
    fn compare_baseline(
        baseline_name: &str,
        current: &[(&str, &PercentileStats)],
        threshold_pct: f64,
    ) {
        use std::path::PathBuf;

        use crate::baseline::{BaselineEntry, BaselineStore};

        let dir: PathBuf = PathBuf::from("target/pbench/baselines");

        let baseline_entries: Vec<BaselineEntry> = match BaselineStore::load(&dir, baseline_name) {
            Ok(entries) => entries,

            Err(e) => {
                eprintln!("pbench: failed to load baseline '{baseline_name}': {e}");
                return;
            }
        };

        eprintln!();
        eprintln!("pbench: comparing against baseline '{baseline_name}'");

        let mut regression_count: u32 = 0;

        for &(name, new_stats) in current {
            use crate::baseline::BaselineEntry;

            let baseline_match: Option<&BaselineEntry> = baseline_entries
                .iter()
                .find(|e: &&BaselineEntry| e.name == name);

            if let Some(old_entry) = baseline_match {
                use crate::baseline::{RegressionChecker, RegressionResult};

                let old_stats: PercentileStats = old_entry.stats.to_percentile_stats();
                let result: RegressionResult =
                    RegressionChecker::check(&old_stats, new_stats, threshold_pct);

                if result.is_regression {
                    regression_count += 1;
                    eprintln!(
                        "  REGRESSION {name}: p50={:+.1}% p99={:+.1}% mean={:+.1}%",
                        result.p50_delta_pct, result.p99_delta_pct, result.mean_delta_pct
                    );
                } else {
                    eprintln!(
                        "  ok {name}: p50={:+.1}% p99={:+.1}%",
                        result.p50_delta_pct, result.p99_delta_pct
                    );
                }
            } else {
                eprintln!("  new {name} (no baseline)");
            }
        }

        if regression_count > 0 {
            eprintln!();
            eprintln!(
                "pbench: {regression_count} regression(s) detected (threshold: {threshold_pct:.1}%)"
            );
        }
    }

    // =====================================================================
    //  Helpers
    // =====================================================================

    /// Convert CLI overrides into a [`BenchOptions`] for layered resolution.
    fn cli_to_bench_options(args: &CliArgs) -> BenchOptions {
        BenchOptions {
            sample_count: args.sample_count,
            sample_size: args.sample_size,
            ..BenchOptions::default()
        }
    }

    /// Visit all leaf entries in the tree with their fully qualified names.
    fn visit_entries(
        nodes: &[EntryTree],
        parent_path: &str,
        visitor: &mut dyn FnMut(&str, &AnyBenchEntry),
    ) {
        for node in nodes {
            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    let path: String = if parent_path.is_empty() {
                        (*raw_name).to_owned()
                    } else {
                        format!("{parent_path}::{raw_name}")
                    };

                    Self::visit_entries(children, &path, visitor);
                }

                EntryTree::Leaf { entry } => {
                    let name: &str = entry.raw_name();
                    let full_name: String = if parent_path.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{parent_path}::{name}")
                    };

                    visitor(&full_name, entry);
                }
            }
        }
    }

    /// Human-readable timer backend name.
    const fn timer_name(timer: &Timer) -> &'static str {
        match timer {
            Timer::Os(_) => "os-clock",

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Timer::Tsc(_) => "tsc",
        }
    }
}
