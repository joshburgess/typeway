# Production Hardening Checklist

Gaps identified between the current state (Phases 0–7 complete) and production readiness.

---

## Not Yet Implemented (from original plan)

- [x] **MSRV testing in CI** — Rust 1.80, CI job verifies `cargo check --workspace --all-features`
- [ ] **Compile-time regression tracking** — benchmarks exist but CI doesn't fail on regressions or track build times with `cargo build --timings` / `hyperfine`
- [ ] **Benchmark regression gating** — criterion benchmarks exist but no baseline comparison in CI that fails on >10% throughput drops

## Production Hardening Gaps

- [x] **Panic safety** — `RouterService::call` wraps handler futures in `catch_unwind`, returns 500 on panic
- [x] **Fuzz testing** — proptest-based tests for path parsing, JSON deserialization, query strings, and raw body handling
- [x] **Dependency auditing in CI** — `cargo-deny` (advisories + licenses) and `cargo-audit` (CVEs) added to CI
- [x] **Client retries/backoff** — `RetryPolicy` + `ClientConfig` with exponential backoff, jitter, and configurable timeouts
- [x] **Security headers** — `SecureHeadersLayer` with 6 defaults + builder pattern for HSTS, CSP overrides, custom headers
- [ ] **Request size limits adversarial testing** — body size limits exist but need testing against slowloris, chunked encoding edge cases, etc.
- [ ] **Graceful degradation docs** — graceful shutdown exists but no guidance on health checks, readiness probes, or load balancer draining

## Polish

- [ ] **`todo!()` in macro doc examples** — 4 instances in `typeway-macros/src/lib.rs`
- [ ] **No `CHANGELOG.md`** or versioning strategy
