#[test]
fn handler_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/handler/pass_*.rs");
}

#[test]
fn handler_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/handler/fail_*.rs");
}

#[test]
fn api_description_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/api_description/pass_*.rs");
}

#[test]
fn api_description_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/api_description/fail_*.rs");
}

#[test]
fn documented_handler_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/documented_handler/pass_*.rs");
}

#[test]
fn schema_derive_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/schema_derive/pass_*.rs");
}

#[test]
fn proto_derive_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/proto_derive/pass_*.rs");
}
