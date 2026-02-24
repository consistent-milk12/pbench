//! Benchmark runner: discovery, filtering, execution, and output.
//!
//! The [`Runner`] collects all registered benchmark entries, builds an
//! [`EntryTree`], applies CLI filters and sort order, then dispatches to
//! the appropriate action: list, test, or full bench with output rendering.

use std::collections::HashMap;
use std::io::{self as StdIo, Write};
use std::panic as StdPanic;
use std::thread as StdThread;

use crate::bencher::{BenchContext, Bencher};
use crate::cli::{CliArgs, OutputFormat, SortBy};
use crate::config::{BenchOptions, ResolvedBenchOptions};
use crate::entry::tree::push_path_component;
use crate::entry::{AnyBenchEntry, BENCH_ENTRIES, EntryTree, GroupEntry};
use crate::output::csv::CsvRenderer;
use crate::output::render::{BenchRecord, TableRenderer};
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
    #[allow(dead_code, reason = "Programmatic builder API for library consumers")]
    pub(crate) const fn sample_count(mut self, n: u32) -> Self {
        self.args.sample_count = Some(n);

        self
    }

    /// Override minimum benchmarking time (builder API).
    #[must_use]
    #[allow(dead_code, reason = "Programmatic builder API for library consumers")]
    pub(crate) const fn min_time(mut self, d: std::time::Duration) -> Self {
        self.args.sample_size = None; // adaptive when min_time overridden
        self.args.min_time = Some(d);

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
            RunAction::Bench => self.action_bench(&mut tree),
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
        let mut buf: String = String::new();

        Self::collect_names(tree, &mut buf, &mut |name: &str| {
            writeln!(out, "{name}").expect("write");
        });

        out.flush().expect("flush stdout");
    }

    /// Collect fully qualified benchmark names by traversing the tree.
    fn collect_names(nodes: &[EntryTree], buf: &mut String, emit: &mut dyn FnMut(&str)) {
        for node in nodes {
            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, raw_name);

                    Self::collect_names(children, buf, emit);

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );

                    buf.truncate(saved);
                }

                EntryTree::Leaf { entry } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, entry.raw_name());

                    emit(buf);

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );

                    buf.truncate(saved);
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
        let mut buf: String = String::new();

        Self::visit_entries(tree, &mut buf, &mut |name: &str, entry: &AnyBenchEntry| {
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
                    let ctx: BenchContext = BenchContext::new(resolved, 1);
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
    fn action_bench(&self, tree: &mut [EntryTree]) {
        let timer: Timer = Timer::best_available();

        // Report timer info to stderr.
        eprintln!(
            "pbench: timer={}, precision={}",
            Self::timer_name(&timer),
            timer.precision()
        );

        // Detect whether any entry uses multi-thread dispatch.
        // We check by collecting results first, then validating output.
        let mut records: Vec<BenchRecord> = Vec::new();
        let mut buf: String = String::new();

        Self::run_bench_tree(tree, &mut buf, None, &self.args, &mut records);

        if records.is_empty() {
            eprintln!("pbench: no benchmarks were executed");
            return;
        }

        // Post-collection sort for stats-based ordering.
        if matches!(self.args.sort, SortBy::P50 | SortBy::P99 | SortBy::Mean) {
            // Build stats map: for benchmarks run with multiple thread counts
            // (--threads 1,2,4), the map keeps the fastest (minimum) stat per
            // benchmark name so sorting reflects best-case performance.
            let mut stats_map: HashMap<String, FineDuration> = HashMap::new();

            for r in &records {
                let stat: FineDuration = match self.args.sort {
                    SortBy::P50 => r.stats.percentiles.p50,

                    SortBy::P99 => r.stats.percentiles.p99,

                    SortBy::Mean => r.stats.mean,

                    SortBy::Name => unreachable!(),
                };

                stats_map
                    .entry(r.name.clone())
                    .and_modify(|existing: &mut FineDuration| {
                        if stat.picos < existing.picos {
                            *existing = stat;
                        }
                    })
                    .or_insert(stat);
            }

            EntryTree::sort_by_stats(tree, self.args.sort, &stats_map, &mut buf);
        }

        // Build output slices.
        let result_pairs: Vec<(&str, u32, &PercentileStats)> = records
            .iter()
            .map(|r: &BenchRecord| (r.name.as_str(), r.thread_count, &r.stats))
            .collect();

        // Render output.
        match self.args.output_format {
            OutputFormat::Table => {
                TableRenderer::render(
                    tree,
                    &records,
                    self.args.bytes_format,
                    None,
                    self.args.threshold,
                );
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
        buf: &mut String,
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
                    let saved: usize = buf.len();
                    push_path_component(buf, raw_name);

                    // Prefer this node's group; fall back to inherited group.
                    let active_group: Option<&'static GroupEntry> = node_group.or(group);

                    Self::run_bench_tree(children, buf, active_group, args, records);

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );

                    buf.truncate(saved);
                }

                EntryTree::Leaf { entry } => {
                    Self::run_single_entry(entry, buf, group, args, records);
                }
            }
        }
    }

    /// Run a single benchmark entry and collect its result.
    fn run_single_entry(
        entry: &AnyBenchEntry,
        buf: &mut String,
        group: Option<&'static GroupEntry>,
        args: &CliArgs,
        records: &mut Vec<BenchRecord>,
    ) {
        match entry {
            AnyBenchEntry::Bench(bentry) => {
                let saved: usize = buf.len();
                push_path_component(buf, bentry.meta.raw_name);

                let full_name: String = buf.to_owned();

                debug_assert!(
                    buf.len() >= saved,
                    "buffer was modified beyond truncate point"
                );
                buf.truncate(saved);

                // resolve options
                let entry_opts: BenchOptions =
                    bentry.options.map_or_else(BenchOptions::default, |f| f());
                let group_opts: BenchOptions = group
                    .and_then(|group: &GroupEntry| group.options)
                    .map_or_else(BenchOptions::default, |f| f());
                let cli_opts: BenchOptions = Self::cli_to_bench_options(args);
                let merged: BenchOptions = cli_opts.overwrite(&entry_opts.overwrite(&group_opts));
                let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&merged);

                if !args.run_ignored.should_run(resolved.ignore) {
                    return;
                }

                for &tc in &resolved.threads {
                    let ctx: BenchContext = BenchContext::new(resolved.clone(), tc);
                    let bencher: Bencher<'_> = Bencher::new(&ctx);
                    (bentry.bench_fn)(&bencher);

                    if let Some(result) = ctx.finish() {
                        let warnings: Vec<String> = PercentileStats::check_sample_sufficiency(
                            result.stats.sample_count,
                            resolved.sample_count,
                        );
                        for w in &warnings {
                            eprintln!("pbench: warning: {full_name}: {w}");
                        }

                        records.push(BenchRecord {
                            name: full_name.clone(),
                            thread_count: tc,
                            stats: result.stats,
                            counter: result.counter,
                        });
                    }
                }
            }

            AnyBenchEntry::Generic(ge) => {
                // Resolve options
                let group_opts: BenchOptions = group
                    .and_then(|g: &GroupEntry| g.options)
                    .map_or_else(BenchOptions::default, |f| f());
                let cli_opts: BenchOptions = Self::cli_to_bench_options(args);
                let merged: BenchOptions = cli_opts.overwrite(&group_opts);
                let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&merged);

                if !args.run_ignored.should_run(resolved.ignore) {
                    return;
                }

                for &arg in ge.args {
                    let saved: usize = buf.len();
                    push_path_component(buf, ge.meta.raw_name);
                    buf.push_str("::");
                    buf.push_str(arg);

                    let full_name: String = buf.to_owned();

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );
                    buf.truncate(saved);

                    for &tc in &resolved.threads {
                        let ctx: BenchContext = BenchContext::new(resolved.clone(), tc);
                        let bencher: Bencher<'_> = Bencher::new(&ctx);
                        (ge.bench_fn)(&bencher, arg);

                        if let Some(result) = ctx.finish() {
                            let warnings: Vec<String> = PercentileStats::check_sample_sufficiency(
                                result.stats.sample_count,
                                resolved.sample_count,
                            );
                            for w in &warnings {
                                eprintln!("pbench: warning: {full_name}: {w}");
                            }

                            records.push(BenchRecord {
                                name: full_name.clone(),
                                thread_count: tc,
                                stats: result.stats,
                                counter: result.counter,
                            });
                        }
                    }
                }
            }

            AnyBenchEntry::Group(_) => {
                // Groups are structural, not runnable.
            }
        }
    }

    // =====================================================================
    //  Baseline comparison
    // =====================================================================

    /// Compare current results against a saved baseline.
    ///
    /// Matches entries by `(name, thread_count)` pair. Entries in the current
    /// run that have no baseline match are reported as "new".
    #[cfg(feature = "json")]
    fn compare_baseline(
        baseline_name: &str,
        current: &[(&str, u32, &PercentileStats)],
        threshold_pct: f64,
    ) {
        use std::path::PathBuf;

        use crate::baseline::{BaselineEntry, BaselineStore, RegressionChecker, RegressionResult};

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

        for &(name, thread_count, new_stats) in current {
            let baseline_match: Option<&BaselineEntry> = baseline_entries
                .iter()
                .find(|e: &&BaselineEntry| e.name == name && e.thread_count == thread_count);

            if let Some(old_entry) = baseline_match {
                let old_stats: PercentileStats = old_entry.stats.to_percentile_stats();
                let result: RegressionResult =
                    RegressionChecker::check(&old_stats, new_stats, threshold_pct);

                if result.is_regression {
                    regression_count += 1;
                    eprintln!(
                        "  REGRESSION {name} (t={thread_count}): p50={:+.1}% p99={:+.1}% mean={:+.1}%",
                        result.p50_delta_pct, result.p99_delta_pct, result.mean_delta_pct
                    );
                } else {
                    eprintln!(
                        "  ok {name} (t={thread_count}): p50={:+.1}% p99={:+.1}%",
                        result.p50_delta_pct, result.p99_delta_pct
                    );
                }
            } else {
                eprintln!("  new {name} (t={thread_count}, no baseline)");
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
            min_time: args.min_time,
            threads: args.threads.clone(),
            ..BenchOptions::default()
        }
    }

    /// Visit all leaf entries in the tree with their fully qualified names.
    fn visit_entries(
        nodes: &[EntryTree],
        buf: &mut String,
        visitor: &mut dyn FnMut(&str, &AnyBenchEntry),
    ) {
        for node in nodes {
            match node {
                EntryTree::Parent {
                    raw_name, children, ..
                } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, raw_name);

                    Self::visit_entries(children, buf, visitor);

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );

                    buf.truncate(saved);
                }

                EntryTree::Leaf { entry } => {
                    let saved: usize = buf.len();
                    push_path_component(buf, entry.raw_name());

                    visitor(buf, entry);

                    debug_assert!(
                        buf.len() >= saved,
                        "buffer was modified beyond truncate point"
                    );

                    buf.truncate(saved);
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
