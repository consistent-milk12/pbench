//! Invalid args

#[pbench::bench(args = "not_an_array")]
fn invalid_args(b: &pbench::bencher::Bencher<'_>, arg: &str) {
    let _ = arg;

    b.bench_refs(|| {});
}

fn main() {}
