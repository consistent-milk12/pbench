//! Duplicate args

#[pbench::bench(sample_count = 10, sample_count = 20)]
fn duplicate_option(b: &pbench::bencher::Bencher<'_>) {
    b.bench_refs(|| {});
}

fn main() {}
