# Remaining Priorities

Updated status of all planned work. Checked items are complete.

---

## Research Features (from DESIGN.md §14)

- [x] **Middleware as type-level effects** — `Requires<E, Endpoint>`, `EffectfulServer`, `.provide::<E>().ready()`
- [x] **Session-typed WebSocket routes** — `Send`, `Recv`, `Offer`, `Select`, `Rec`/`Var`, `TypedWebSocket<S>`, `Dual` trait
- [x] **Content negotiation coproducts** — `NegotiatedResponse<T, (JsonFormat, TextFormat, XmlFormat)>`, `RenderAs<Format>`, `AcceptHeader`
- [x] **Type-level API versioning** — `VersionedApi<Base, Changes, Resolved>`, `Added`/`Removed`/`Replaced`/`Deprecated`, `assert_api_compatible!`

---

## Practical Improvements

- [x] **Client ergonomics** — `client_api!` macro, interceptors, cookies, streaming, per-call builder, `TypedResponse`, query params, Accept header, tracing
- [x] **OpenAPI enhancements** — `ExampleValue`, security schemes from `Protected`, auto-tag grouping, deprecated marking, `EndpointToOperation` for wrappers
- [ ] **OpenAPI: doc comment extraction** — Extract handler/struct doc comments into OpenAPI description fields. Requires proc-macro or build script to read source.
- [ ] **gRPC / Tonic interop** — Design analysis complete (see GRPC-INTEROP-DESIGN.md). Items 1-3 work today (shared middleware, side-by-side serving, dual serialization). Item 4 (API type → .proto generation) is a v0.2 feature.
- [ ] **Publish to crates.io** — On hold per user request.

---

## Migration Tool (`typeway-migrate`)

### What the tool handles (106 tests):

| Feature | Axum→Typeway | Typeway→Axum | Detection |
|---|---|---|---|
| Routes (GET/POST/PUT/DELETE/PATCH) | Full | Full | Yes |
| Path captures (single, multiple) | Full | Full | Yes |
| Json body extraction | Full | Full | Yes |
| State extraction | Full | Full | Yes |
| Query extraction | Full | Full | Yes |
| Header/HeaderMap | Passthrough | Passthrough | Yes |
| Cookie/CookieJar | Passthrough | Passthrough | Yes |
| Multipart/Form | Passthrough | Passthrough | Yes |
| WebSocket upgrade | Passthrough + warning | Passthrough | Yes |
| Tower middleware layers | Full | Full | Yes |
| `.nest()` prefixes | Full | Full | Yes |
| `.with_state()` | Full | Full | Yes |
| Auth detection (Protected) | Full | Full | Yes |
| Effects (EffectfulServer) | Full | Partial | Yes |
| Validation (Validated) | Scaffolding | Full | Yes |
| OpenAPI setup | Auto-added | N/A | N/A |
| `bind!`/`bind_auth!`/`bind_validated!` | Correct selection | Recognized | Yes |
| `from_fn` middleware | Warning | N/A | Yes |
| `impl IntoResponse` | Warning | N/A | Yes |
| Custom extractors | Warning | Passthrough | Yes |
| Cargo.toml dependencies | `--update-cargo` | `--update-cargo` | N/A |
| Roundtrip fidelity | Tested | Tested | 14 roundtrip tests |

| Client code generation | Commented-out `client_api!` | N/A | N/A |
| `Router::merge()` resolution | Full (same-file) | N/A | Yes |
| Interactive mode | `--interactive` | `--interactive` | N/A |
| Partial migration | `--partial` | N/A | N/A |
| Colored output | N/A | N/A | Yes |
| Conversion summary | Printed to stderr | Printed to stderr | N/A |

### What the tool cannot do (by design):

- **Content negotiation conversion** — Axum has no equivalent; typeway-specific opt-in
- **API versioning scaffolding** — No `VersionedApi` generation (user designs versions manually)
- **`from_fn` middleware conversion** — Warns but can't auto-convert arbitrary closures
- **`impl IntoResponse` type inference** — Warns but can't resolve opaque types without type checking
- **Cross-file `Router::merge()`** — Only resolves functions defined in the same file

### Potential future work:

- [ ] **VSCode extension** — Convert selected code in-editor

---

## Infrastructure

- [ ] **Benchmark regression gating** — Criterion benchmarks exist but no CI baseline comparison. Options: `bencher.dev`, `github-action-benchmark`, custom artifact diff.
- [x] **Pre-existing cleanup** — Doctest failure fixed, all rustdoc warnings resolved.
