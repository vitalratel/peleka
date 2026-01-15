// ABOUTME: Trybuild runner for compile-time type safety tests.
// ABOUTME: Verifies that invalid type usage fails to compile.

#[test]
fn id_types_not_interchangeable() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/id_not_interchangeable.rs");
}
