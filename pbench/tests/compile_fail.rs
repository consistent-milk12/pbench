//! trybuild entry point

use trybuild::TestCases;

#[test]
fn compile_fail() {
    let t: TestCases = TestCases::new();

    t.compile_fail("tests/compile_fail/*.rs");
}
