# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **Bidirectional Axum ↔ Typeway conversion**: `syn`-based source rewriter that translates handler functions, route registrations, and extractors in either direction
- **Routes**: GET, POST, PUT, DELETE, PATCH conversion (full)
- **Path captures**: single and multiple captures (full)
- **Json body extraction**: full
- **State extraction**: full
- **Query extraction**: full
- **Header / HeaderMap, Cookie / CookieJar, Multipart / Form**: passthrough in both directions
- **WebSocket upgrade**: passthrough with a warning, since session types don't have a direct Axum equivalent
- **Tower middleware layers**: full
- **`.nest()` prefixes**: full
- **`.with_state()`**: full
- **Auth detection (`Protected`)**: full
- **Effects (`EffectfulServer`)**: full Axum→Typeway, partial Typeway→Axum
- **Validation (`Validated`)**: scaffolding Axum→Typeway, full Typeway→Axum
- **OpenAPI setup**: auto-added on Axum→Typeway conversion
- **Bind macro selection**: `bind!`, `bind_auth!`, and `bind_validated!` are picked correctly based on detected handler shape
- **`from_fn` middleware** and **`impl IntoResponse` returns**: warns when manual review is needed
- **Custom extractors**: passthrough with warning
- **Cargo.toml dependencies**: `--update-cargo` flag rewrites the dependency manifest
- **Roundtrip fidelity**: 14 roundtrip tests verify Axum→Typeway→Axum and Typeway→Axum→Typeway preserve behavior
- **`Router::merge()` resolution**: same-file merges fully resolved
- **`--interactive`**: review and accept changes per file
- **`--partial`**: only convert detected handlers; leave the rest unchanged
- **Conversion summary** to stderr; colored output
- **VS Code extension** (`typeway-vscode`): wraps the CLI with Convert, Preview, and Check commands
