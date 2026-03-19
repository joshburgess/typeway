# Production Hardening Checklist

Gaps identified between the current state (Phases 0–7 complete) and production readiness.

---

## Not Yet Implemented (from original plan)

- [x] **MSRV testing in CI** — Rust 1.80, CI job verifies `cargo check --workspace --all-features`
- [x] **Compile-time regression tracking** — CI job captures `cargo build --timings` artifacts (release + dev)
- [ ] **Benchmark regression gating** — criterion benchmarks exist but no baseline comparison in CI that fails on >10% throughput drops. Options: `bencher.dev`, `github-action-benchmark`, or custom artifact diff. Planned for future setup.

## Production Hardening Gaps

- [x] **Panic safety** — `RouterService::call` wraps handler futures in `catch_unwind`, returns 500 on panic
- [x] **Fuzz testing** — proptest-based tests for path parsing, JSON deserialization, query strings, and raw body handling
- [x] **Dependency auditing in CI** — `cargo-deny` (advisories + licenses) and `cargo-audit` (CVEs) added to CI
- [x] **Client retries/backoff** — `RetryPolicy` + `ClientConfig` with exponential backoff, jitter, and configurable timeouts
- [x] **Security headers** — `SecureHeadersLayer` with 6 defaults + builder pattern for HSTS, CSP overrides, custom headers
- [x] **Request size limits adversarial testing** — 9 tests covering boundary conditions, chunked encoding, mismatched Content-Length, custom limits
- [x] **Graceful degradation docs** — `production` module with health checks, shutdown, draining, recommended middleware stack, panic recovery

## Polish

- [x] **`todo!()` in macro doc examples** — replaced with realistic code
- [x] **Per-crate changelogs** — Keep a Changelog format, independent semver per crate
