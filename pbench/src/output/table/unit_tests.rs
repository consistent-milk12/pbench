//! Unit tests for the `output::table` module.

use std::io;

use super::TableColumn;
use crate::cli::BytesFormat;
use crate::counter::{CounterCollection, CounterKind};
use crate::output::table::TablePainter;
use crate::stats::{PercentileSet, PercentileStats};
use crate::time::FineDuration;

// =========================================================================
//  Helpers
// =========================================================================

/// Create `PercentileStats` from nanosecond values for concise test setup.
#[expect(clippy::similar_names)]
fn make_stats(
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    p99_9_ns: u64,
    p99_99_ns: u64,
    mean_ns: u64,
) -> PercentileStats {
    let to_fd = |ns: u64| -> FineDuration {
        FineDuration {
            picos: u128::from(ns) * 1_000,
        }
    };
    PercentileStats {
        sample_count: 100,
        iter_count: 10_000,
        min: to_fd(p50_ns),
        max: to_fd(p99_99_ns),
        mean: to_fd(mean_ns),
        std_dev: FineDuration::ZERO,
        percentiles: PercentileSet {
            p50: to_fd(p50_ns),
            p95: to_fd(p95_ns),
            p99: to_fd(p99_ns),
            p99_9: to_fd(p99_9_ns),
            p99_99: to_fd(p99_99_ns),
        },
    }
}

/// Render table output to a `String`, propagating I/O errors.
fn render<F>(max_name_span: usize, f: F) -> io::Result<String>
where
    F: FnOnce(&mut TablePainter<&mut Vec<u8>>) -> io::Result<()>,
{
    let mut buf: Vec<u8> = Vec::new();
    let widths: [usize; TableColumn::COUNT] = [10; TableColumn::COUNT];
    {
        let mut painter: TablePainter<&mut Vec<u8>> =
            TablePainter::new(&mut buf, max_name_span, widths);
        f(&mut painter)?;
    }

    Ok(String::from_utf8(buf).expect("valid utf8"))
}

// =========================================================================
//  Tests
// =========================================================================

// --- table_renders_header ---

#[test]
fn table_renders_header() -> io::Result<()> {
    let output: String = render(20, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("my_benches", false)?;
        p.finish_parent()
    })?;

    assert!(
        output.contains("p50"),
        "expected 'p50' in header:\n{output}"
    );
    assert!(
        output.contains("p95"),
        "expected 'p95' in header:\n{output}"
    );
    assert!(
        output.contains("p99"),
        "expected 'p99' in header:\n{output}"
    );
    assert!(
        output.contains("p99.9"),
        "expected 'p99.9' in header:\n{output}"
    );
    assert!(
        output.contains("p99.99"),
        "expected 'p99.99' in header:\n{output}"
    );
    assert!(
        output.contains("mean"),
        "expected 'mean' in header:\n{output}"
    );

    Ok(())
}

// --- table_renders_row ---

#[test]
fn table_renders_row() -> io::Result<()> {
    let stats: PercentileStats = make_stats(125, 180, 250, 411, 621, 142);

    let output: String = render(20, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("group", false)?;
        p.write_leaf("bench_1", &stats, None, true)?;
        p.finish_parent()
    })?;

    assert!(
        output.contains("bench_1"),
        "expected 'bench_1' in output:\n{output}"
    );
    assert!(
        output.contains("125 ns"),
        "expected '125 ns' in output:\n{output}"
    );

    Ok(())
}

// --- table_tree_structure ---

#[test]
fn table_tree_structure() -> io::Result<()> {
    let stats: PercentileStats = make_stats(125, 180, 250, 411, 621, 142);

    let output: String = render(20, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("group", false)?;
        p.write_leaf("bench_1", &stats, None, false)?;
        p.write_leaf("bench_2", &stats, None, true)?;
        p.finish_parent()
    })?;

    assert!(
        output.contains("\u{251c}\u{2500} bench_1"),
        "expected '\u{251c}\u{2500} bench_1' in output:\n{output}"
    );
    assert!(
        output.contains("\u{2570}\u{2500} bench_2"),
        "expected '\u{2570}\u{2500} bench_2' in output:\n{output}"
    );

    Ok(())
}

// --- write_comparison_row_no_panic ---

#[test]
fn write_comparison_row_no_panic() -> io::Result<()> {
    let stats: PercentileStats = make_stats(125, 180, 250, 411, 621, 142);

    let _output: String = render(20, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("group", false)?;
        p.write_comparison_row(&stats, &stats, 5.0, true)?;
        p.finish_parent()
    })?;

    Ok(())
}

// --- ignored_leaf_placeholder ---

#[test]
fn ignored_leaf_placeholder() -> io::Result<()> {
    let output: String = render(20, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("group", false)?;
        p.write_ignored_leaf("skipped_bench", true)?;
        p.finish_parent()
    })?;

    assert!(
        output.contains("(ignored)"),
        "expected '(ignored)' in output:\n{output}"
    );
    assert!(
        output.contains("skipped_bench"),
        "expected 'skipped_bench' in output:\n{output}"
    );

    Ok(())
}

// --- nested_group_tree_structure ---

#[test]
fn nested_group_tree_structure() -> io::Result<()> {
    let stats: PercentileStats = make_stats(125, 180, 250, 411, 621, 142);

    let output: String = render(30, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("top", false)?;
        p.start_parent("nested", true)?;
        p.write_leaf("inner_bench", &stats, None, true)?;
        p.finish_parent()?;
        p.finish_parent()
    })?;

    assert!(
        output.contains("top"),
        "expected 'top' in output:\n{output}"
    );
    assert!(
        output.contains("nested"),
        "expected 'nested' in output:\n{output}"
    );
    assert!(
        output.contains("inner_bench"),
        "expected 'inner_bench' in output:\n{output}"
    );
    // After finish_parent for the nested group, prefix should be fully restored.
    // The inner bench should have tree prefix characters.
    assert!(
        output.contains("\u{2570}\u{2500} inner_bench"),
        "expected '\u{2570}\u{2500} inner_bench' in output:\n{output}"
    );

    Ok(())
}

// --- write_leaf_with_throughput ---

#[test]
fn write_leaf_with_throughput() -> io::Result<()> {
    let stats: PercentileStats = make_stats(
        1_000_000, 1_200_000, 1_500_000, 2_000_000, 3_000_000, 1_100_000,
    );

    // Build a CounterCollection with 1_000_000 bytes per iteration.
    let mut collection: CounterCollection = CounterCollection::new(CounterKind::Bytes);
    collection.push(1_000_000);
    collection.push(1_000_000);
    collection.push(1_000_000);

    let output: String = render(20, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("group", false)?;
        p.write_leaf(
            "io_bench",
            &stats,
            Some((&collection, BytesFormat::Decimal)),
            true,
        )?;
        p.finish_parent()
    })?;

    // Should have the benchmark name and a throughput line with B/s units.
    assert!(
        output.contains("io_bench"),
        "expected 'io_bench' in output:\n{output}"
    );
    assert!(
        output.contains("B/s"),
        "expected throughput 'B/s' in output:\n{output}"
    );

    Ok(())
}

// --- prefix_restores_after_nested_groups ---

#[test]
fn prefix_restores_after_nested_groups() -> io::Result<()> {
    let stats: PercentileStats = make_stats(100, 200, 300, 400, 500, 150);

    let output: String = render(30, |p: &mut TablePainter<&mut Vec<u8>>| {
        p.start_parent("root", false)?;

        // First nested group.
        p.start_parent("group_a", false)?;
        p.write_leaf("a_bench", &stats, None, true)?;
        p.finish_parent()?;

        // Second nested group — prefix should be restored correctly.
        p.start_parent("group_b", true)?;
        p.write_leaf("b_bench", &stats, None, true)?;
        p.finish_parent()?;
        p.finish_parent()
    })?;

    assert!(
        output.contains("a_bench"),
        "expected 'a_bench' in output:\n{output}"
    );
    assert!(
        output.contains("b_bench"),
        "expected 'b_bench' in output:\n{output}"
    );

    Ok(())
}
