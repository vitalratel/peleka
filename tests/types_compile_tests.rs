// ABOUTME: Trybuild runner for compile-time type safety tests.
// ABOUTME: Verifies that invalid type usage fails to compile.

#[test]
fn id_types_not_interchangeable() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/id_not_interchangeable.rs");
}

#[test]
fn cutover_not_available_on_initialized() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/invalid_transition_cutover_on_initialized.rs");
}

#[test]
fn rollback_not_available_on_completed() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/invalid_transition_rollback_on_completed.rs");
}
