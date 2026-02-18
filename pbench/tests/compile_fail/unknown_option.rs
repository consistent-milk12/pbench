//! Unknown Options

#[pbench::bench(unknown_option = 5)]
fn unknown_option(b: &pbench::bencher::Bencher<'_>) {
    b.bench_refs(|| {});
}

fn main() {}
