# Design: Bidirectional Axum ↔ Typeway Migration Tool

A `syn`-based CLI tool that reads Rust source files and mechanically translates between Axum and Typeway idioms in both directions. The goal is to make trying Typeway zero-cost for existing Axum users — and to make leaving Typeway zero-cost if they decide it's not for them.

---

## Motivation

The biggest barrier to adopting a new web framework isn't technical — it's migration cost. Even if Typeway is strictly better for certain use cases, rewriting working Axum code by hand is tedious and error-prone. A tool that automates 80–90% of the translation eliminates the practical barrier.

The reverse direction matters equally. If a team tries Typeway and decides it's not the right fit — wrong trade-offs for their use case, compile times too slow for their API size, team prefers Axum's imperative style — they should be able to mechanically convert back. This makes Typeway a genuinely risk-free experiment.

---

## Tool Overview

```
typeway-migrate axum-to-typeway [--dir src/] [--dry-run] [--file api.rs]
typeway-migrate typeway-to-axum [--dir src/] [--dry-run] [--file api.rs]
```

**Crate:** `typeway-migrate` (standalone binary crate in the workspace, not a library dependency)

**Core dependency:** `syn` (full features) + `quote` + `proc-macro2` for Rust parsing and code generation. Also `prettyplease` for formatting the output.

**Mode of operation:** Reads `.rs` files, parses them into `syn::File` ASTs, applies transformations, and writes the result back (or to stdout with `--dry-run`). Not a proc macro — a standalone CLI tool.

---

## Direction 1: Axum → Typeway

### What Axum Code Looks Like

```rust
use axum::{
    Router,
    routing::{get, post, delete},
    extract::{Path, State, Json},
    response::IntoResponse,
    http::StatusCode,
};

#[derive(Clone)]
struct AppState { db: DbPool }

async fn list_users(
    State(state): State<AppState>,
) -> Json<Vec<User>> {
    let users = state.db.all_users().await;
    Json(users)
}

async fn get_user(
    Path(id): Path<u32>,
    State(state): State<AppState>,
) -> Result<Json<User>, StatusCode> {
    state.db.find(id).await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let user = state.db.insert(body).await;
    (StatusCode::CREATED, Json(user))
}

async fn delete_user(
    Path(id): Path<u32>,
    State(state): State<AppState>,
) -> StatusCode {
    state.db.delete(id).await;
    StatusCode::NO_CONTENT
}

fn app() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user).delete(delete_user))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(AppState { db: pool })
}
```

### What the Output Looks Like

```rust
use typeway::prelude::*;
use typeway::tower_http::trace::TraceLayer;
use typeway::tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState { db: DbPool }

typeway_path!(type UsersPath = "users");
typeway_path!(type UsersByIdPath = "users" / u32);

type API = (
    GetEndpoint<UsersPath, Json<Vec<User>>>,
    PostEndpoint<UsersPath, Json<CreateUser>, (StatusCode, Json<User>)>,
    GetEndpoint<UsersByIdPath, Result<Json<User>, StatusCode>>,
    DeleteEndpoint<UsersByIdPath, StatusCode>,
);

async fn list_users(
    state: State<AppState>,
) -> Json<Vec<User>> {
    let users = state.0.db.all_users().await;
    Json(users)
}

async fn get_user(
    path: Path<UsersByIdPath>,
    state: State<AppState>,
) -> Result<Json<User>, StatusCode> {
    let (id,) = path.0;
    state.0.db.find(id).await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_user(
    state: State<AppState>,
    body: Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let user = state.0.db.insert(body.0).await;
    (StatusCode::CREATED, Json(user))
}

async fn delete_user(
    path: Path<UsersByIdPath>,
    state: State<AppState>,
) -> StatusCode {
    let (id,) = path.0;
    state.0.db.delete(id).await;
    StatusCode::NO_CONTENT
}

fn app() -> impl Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> {
    Server::<API>::new((
        bind!(list_users),
        bind!(create_user),
        bind!(get_user),
        bind!(delete_user),
    ))
    .layer(TraceLayer::new_for_http())
    .layer(CorsLayer::permissive())
    .with_state(AppState { db: pool })
    .serve(addr)
}
```

### Parsing Strategy (Axum → Typeway)

The tool needs to extract three things from Axum code:

#### 1. Route Table Extraction

Parse the `Router::new().route(...)` chain to build a list of `(path_pattern, method, handler_fn_name)` triples.

**AST pattern to match:**

```
MethodCall {
    receiver: <Router chain>,
    method: "route",
    args: [
        Lit(Str(path_pattern)),
        MethodCall { method: "get" | "post" | "put" | "delete" | "patch", args: [handler_ident] }
        // or chained: get(h1).post(h2)
    ]
}
```

**Complications:**
- Axum allows chaining methods on a `MethodRouter`: `.route("/users", get(list).post(create))` registers two handlers on one path. The tool must split these into separate endpoint entries.
- `.route("/users/{id}", ...)` uses `{id}` syntax for captures. The tool must parse the path string and extract capture names.
- `.nest("/api", sub_router)` must be tracked to reconstruct full path prefixes.
- Routes may be defined across multiple functions that return `Router` fragments and merge them. The tool handles single-function router definitions first; multi-function composition is a future enhancement marked with a `// TODO: manual review needed` comment.

#### 2. Handler Signature Analysis

Parse each handler function's signature to determine:
- **Path capture types:** From `Path<u32>` or `Path<(u32, String)>` extractors.
- **Body type:** From `Json<T>` extractor (the `FromRequest` argument, typically last).
- **Response type:** The return type, which becomes the endpoint's `Res` parameter.
- **Other extractors:** `State<T>`, `Query<T>`, `HeaderMap`, etc. — these transfer directly.

**AST pattern for destructuring extractors:**

Axum handlers use destructuring patterns: `Path(id): Path<u32>`. Typeway handlers use the wrapper type directly: `path: Path<UsersByIdPath>`, then destructure with `let (id,) = path.0;`.

The tool must:
1. Identify `Path` extractor arguments.
2. Replace `Path<T>` with `Path<GeneratedPathType>` where `GeneratedPathType` is the `typeway_path!` type for this route's path pattern.
3. Insert a `let` binding at the start of the function body that destructures `path.0` into the original variable names.
4. Remove the destructuring pattern from the function signature.

**Example transformation:**

```rust
// Input (Axum):
async fn get_user(
    Path(id): Path<u32>,
    State(state): State<AppState>,
) -> Json<User> {
    let user = state.db.find(id).await;
    Json(user)
}

// Output (Typeway):
async fn get_user(
    path: Path<UsersByIdPath>,
    state: State<AppState>,
) -> Json<User> {
    let (id,) = path.0;
    let state = state.0;
    let user = state.db.find(id).await;
    Json(user)
}
```

#### 3. Path Pattern → typeway_path! Generation

Parse Axum path strings like `"/users/{id}/posts/{post_id}"` and generate:

1. A `typeway_path!` declaration with a generated type name.
2. A mapping from capture names to their types (inferred from handler signatures).

**Algorithm:**

```
input:  "/users/{id}/posts/{post_id}"
handler: async fn f(Path((id, post_id)): Path<(u32, u64)>, ...) -> ...

1. Split path on "/" → ["users", "{id}", "posts", "{post_id}"]
2. For each segment:
   - If it starts with "{" → Capture segment. Look up type from handler's Path<T> extractor.
   - Otherwise → Literal segment.
3. Zip captures with types from the Path<(T1, T2, ...)> tuple: id → u32, post_id → u64
4. Generate type name from path: "users" + "by" + "id" + "posts" + "by" + "post_id" → UsersByIdPostsByPostIdPath
   (or simpler heuristic: use the handler function name + "Path")
5. Emit: typeway_path!(type UsersByIdPostsByPostIdPath = "users" / u32 / "posts" / u64);
```

**Path name deduplication:** Multiple handlers may share the same path (e.g., GET and DELETE on `/users/{id}`). The tool deduplicates `typeway_path!` declarations — one per unique path pattern.

#### 4. API Type Assembly

Once all routes are extracted, the tool assembles the `type API = (...)` tuple:

```
For each (path_pattern, method, handler) triple:
    1. Look up the generated typeway_path type for this path_pattern
    2. Determine Req type (from body extractor, or NoBody)
    3. Determine Res type (from handler return type)
    4. Emit: MethodEndpoint<PathType, Req, Res>  (or MethodEndpoint<PathType, Res> for bodyless methods)
```

#### 5. Use Statement Rewriting

Replace:
```rust
use axum::{Router, routing::{get, post}, extract::{Path, State, Json}, ...};
```

With:
```rust
use typeway::prelude::*;
```

Axum-specific imports (`Router`, `routing::get`, etc.) are removed. Shared types (`Json`, `StatusCode`) are re-mapped to typeway's re-exports. Tower middleware imports (`tower_http::*`) stay the same but are re-rooted under `typeway::tower_http`.

#### 6. Layer/Middleware Passthrough

`.layer(...)` calls translate directly — both Axum and Typeway use Tower layers with the same API. The tool copies them unchanged.

`.with_state(state)` also translates directly.

#### 7. Router Construction → Server Construction

Replace:
```rust
Router::new()
    .route("/users", get(list_users).post(create_user))
    .route("/users/{id}", get(get_user))
    .layer(...)
    .with_state(state)
```

With:
```rust
Server::<API>::new((
    bind!(list_users),
    bind!(create_user),
    bind!(get_user),
))
.layer(...)
.with_state(state)
```

The order of `bind!()` entries must match the order of endpoints in the `type API` tuple. The tool tracks this mapping during route extraction.

---

## Direction 2: Typeway → Axum

### Parsing Strategy (Typeway → Axum)

This direction is structurally simpler because Typeway's API type is a single, parseable declaration, whereas Axum's route table is spread across imperative builder calls.

#### 1. API Type Extraction

Parse `type API = (Endpoint1, Endpoint2, ...)` to extract the list of endpoint types.

For each endpoint, extract:
- Method (from `GetEndpoint`, `PostEndpoint`, etc.)
- Path type (the first type parameter)
- Request body type (for `PostEndpoint<P, Req, Res>`, etc.)
- Response type (the last type parameter)

#### 2. Path Type → String Pattern

Look up each `typeway_path!` declaration to recover the path string:

```rust
typeway_path!(type UsersByIdPath = "users" / u32);
// → "/users/{param1}"   (or "/users/:param1" for Axum)
```

The capture parameter names are lost in the type system (they're just types, not named). The tool must either:
- Use generic names (`param1`, `param2`) and let the user rename them.
- Attempt to recover names from the handler signature's destructuring patterns.
- Use the original `typeway_path!` macro invocation's structure: `"users" / u32` → the type name `u32` becomes the capture, name it based on position or type (e.g., `id` for `u32`, `name` for `String`).

**Preferred approach:** Recover names from the handler function. If the handler has `let (id,) = path.0;`, extract `id` as the capture name. If not available, fall back to positional names with a `// TODO: rename` comment.

#### 3. Handler Signature Transformation

Reverse the Axum → Typeway handler transformation:

```rust
// Input (Typeway):
async fn get_user(
    path: Path<UsersByIdPath>,
    state: State<AppState>,
) -> Result<Json<User>, StatusCode> {
    let (id,) = path.0;
    let state = state.0;
    state.db.find(id).await.map(Json).ok_or(StatusCode::NOT_FOUND)
}

// Output (Axum):
async fn get_user(
    Path(id): Path<u32>,
    State(state): State<AppState>,
) -> Result<Json<User>, StatusCode> {
    state.db.find(id).await.map(Json).ok_or(StatusCode::NOT_FOUND)
}
```

Steps:
1. Find `Path<TypewayPathType>` arguments.
2. Look up the captures from the `typeway_path!` declaration → `(u32,)`.
3. Find the `let (name,) = path.0;` destructuring in the body.
4. Replace the argument with `Path(name): Path<u32>` (or `Path((n1, n2)): Path<(T1, T2)>` for multiple captures).
5. Remove the `let` destructuring line.

#### 4. Router Construction

Generate Axum router from the API type and `bind!()` entries:

```rust
// Input (Typeway):
Server::<API>::new((
    bind!(list_users),
    bind!(create_user),
    bind!(get_user),
    bind!(delete_user),
))
.layer(TraceLayer::new_for_http())
.with_state(state)

// Output (Axum):
Router::new()
    .route("/users", get(list_users).post(create_user))
    .route("/users/{id}", get(get_user).delete(delete_user))
    .layer(TraceLayer::new_for_http())
    .with_state(state)
```

The tool must group endpoints by path pattern and chain methods on the same `.route()` call when multiple HTTP methods share a path.

#### 5. Remove Typeway-Specific Artifacts

- Remove `typeway_path!` declarations
- Remove `type API = (...)` declaration
- Replace `use typeway::prelude::*` with specific Axum imports
- Remove `bind!()` wrappers

---

## Implementation Architecture

```
typeway-migrate/
    Cargo.toml
    src/
        main.rs              — CLI entry point (clap)
        lib.rs               — public API for programmatic use
        parse/
            mod.rs
            axum.rs           — extract route table, handlers from Axum AST
            typeway.rs        — extract API type, path types from Typeway AST
            common.rs         — shared handler signature analysis
        transform/
            mod.rs
            axum_to_typeway.rs — Axum AST → Typeway AST transformations
            typeway_to_axum.rs — Typeway AST → Axum AST transformations
        emit/
            mod.rs
            codegen.rs        — syn AST → token stream → formatted Rust source
        model.rs              — intermediate representation (IR)
```

### Intermediate Representation

Both directions parse into a shared IR before emitting code:

```rust
struct ApiModel {
    endpoints: Vec<EndpointModel>,
    state_type: Option<syn::Type>,
    layers: Vec<syn::Expr>,
}

struct EndpointModel {
    method: HttpMethod,
    path: PathModel,
    handler: HandlerModel,
    request_body: Option<syn::Type>,
    response_type: syn::Type,
}

struct PathModel {
    raw_pattern: String,             // "/users/{id}/posts"
    segments: Vec<PathSegment>,
    typeway_type_name: Option<Ident>, // UsersPath, UsersByIdPath, etc.
}

enum PathSegment {
    Literal(String),
    Capture { name: String, ty: syn::Type },
}

struct HandlerModel {
    name: Ident,
    is_async: bool,
    extractors: Vec<ExtractorModel>,
    return_type: syn::Type,
    body: Vec<syn::Stmt>,
}

struct ExtractorModel {
    kind: ExtractorKind,
    pattern: syn::Pat,          // destructuring pattern
    ty: syn::Type,              // full type including wrapper
    inner_ty: syn::Type,        // inner type (e.g., u32 inside Path<u32>)
}

enum ExtractorKind {
    Path,
    State,
    Json,
    Query,
    Header,
    Extension,
    Other,
}
```

### The Parse → Transform → Emit Pipeline

```
                     ┌─────────────┐
   Axum .rs file ──▶ │  parse::axum │──▶ ApiModel ──▶ emit as Typeway
                     └─────────────┘
                     ┌────────────────┐
Typeway .rs file ──▶ │ parse::typeway │──▶ ApiModel ──▶ emit as Axum
                     └────────────────┘
```

Both parsers produce the same `ApiModel`. The emitter takes an `ApiModel` and a target framework, and produces formatted Rust source. This means the IR is the single point of truth, and adding a third framework target in the future (e.g., Actix) only requires a new parser and emitter.

---

## Edge Cases and Limitations

### Things the tool handles automatically

- Simple route definitions: `.route("/path", get(handler))`
- Chained method routers: `.route("/path", get(h1).post(h2))`
- Common extractors: `Path`, `State`, `Json`, `Query`, `HeaderMap`
- Tower layers: `.layer(...)` (passthrough, both frameworks use Tower)
- `.with_state(state)` (direct translation)
- `.nest("/prefix", sub_router)` → `Server::nest("/prefix")`
- `StatusCode` return types
- `Result<T, E>` return types
- Tuple response types: `(StatusCode, Json<T>)`

### Things that require manual intervention (marked with `// TODO`)

- **Custom extractors.** If a handler uses a custom `FromRequestParts` impl, the tool copies the argument as-is and adds a `// TODO: verify this extractor works with typeway` comment. Most custom extractors work unchanged since both frameworks use the same `http::request::Parts` type.

- **Axum-specific middleware.** `axum::middleware::from_fn` closures can't be mechanically translated. The tool leaves them as `// TODO: convert to Tower layer` comments.

- **Router composition across functions.** If the router is built by merging sub-routers from different functions (`let app = base_router().merge(admin_router())`), the tool only handles the individual functions. The merge point requires manual assembly.

- **Handler functions that reference `axum::body::Body` directly.** Typeway pre-collects the body into `Bytes` before handler dispatch. Handlers that stream the body incrementally need restructuring.

- **Axum's `Extension<T>` vs Typeway's `Extension<T>`.** These are semantically identical but may have different import paths. The tool remaps the import.

- **Complex return types.** If a handler returns `impl IntoResponse` (opaque type), the tool can't determine the concrete response type for the API tuple. It emits `// TODO: specify concrete response type` and uses a placeholder.

- **WebSocket handlers.** Different upgrade mechanisms. Marked for manual conversion.

### Things the tool explicitly refuses to do

- **Rewrite business logic.** The tool only transforms framework-specific boilerplate (routing, extractors, bindings). Handler bodies are copied verbatim (with minimal adjustments to destructuring patterns).

- **Infer types that aren't syntactically visible.** If the response type is determined by trait resolution (e.g., a blanket `IntoResponse` impl), the tool can't recover the concrete type. It requires explicit type annotations.

- **Handle non-Rust config.** If routes are defined in YAML, JSON, or environment variables, they're outside scope.

---

## CLI Interface

```
typeway-migrate 0.1.0
Bidirectional Axum ↔ Typeway code migration

USAGE:
    typeway-migrate <COMMAND> [OPTIONS]

COMMANDS:
    axum-to-typeway    Convert Axum code to Typeway
    typeway-to-axum    Convert Typeway code to Axum
    check              Parse and report what would be converted (no changes)

OPTIONS:
    --file <PATH>      Process a single file
    --dir <PATH>       Process all .rs files in a directory (default: src/)
    --dry-run          Print output to stdout instead of writing files
    --backup           Create .bak files before overwriting (default: true)
    --verbose          Show detailed transformation log
```

### Example Session

```bash
# See what would change without modifying anything
$ typeway-migrate check --dir src/

Found 3 handler functions:
  src/api.rs:15  list_users    GET  /users
  src/api.rs:23  get_user      GET  /users/{id}
  src/api.rs:34  create_user   POST /users

1 router definition:
  src/api.rs:45  fn app() -> Router<AppState>

2 layers:
  TraceLayer::new_for_http()
  CorsLayer::permissive()

1 state type:
  AppState

Ready to convert. Run with `axum-to-typeway` to apply.

# Actually convert
$ typeway-migrate axum-to-typeway --dir src/

Converted src/api.rs:
  + 3 typeway_path! declarations
  + 1 API type (3 endpoints)
  ~ 3 handler signatures updated
  ~ 1 router → Server construction
  ~ use statements updated
  Backup: src/api.rs.bak

# Verify it compiles
$ cargo check

# Don't like it? Convert back.
$ typeway-migrate typeway-to-axum --dir src/
```

---

## Dependencies

```toml
[package]
name = "typeway-migrate"
version = "0.1.0"
edition = "2021"

[dependencies]
syn = { version = "2", features = ["full", "visit", "visit-mut", "parsing", "printing"] }
quote = "1"
proc-macro2 = "1"
prettyplease = "0.2"
clap = { version = "4", features = ["derive"] }
anyhow = "1"
walkdir = "2"
```

No dependency on `typeway` itself — the tool operates purely on syntax. It doesn't need to resolve types or compile anything. This means it works even if the project doesn't have Typeway in its `Cargo.toml` yet (the tool can add it).

---

## Phasing

### Phase 1: Core pipeline (MVP)

- Parse single-file Axum router definitions
- Extract routes, handlers, extractors
- Generate `typeway_path!`, `type API`, `Server::new`, `bind!()`
- Handle `Path`, `State`, `Json`, `Query` extractors
- `--dry-run` mode only (no file writing)
- Axum → Typeway direction only

### Phase 2: Reverse direction + file writing

- Typeway → Axum conversion
- File writing with `.bak` backups
- Multi-file directory scanning
- `check` command

### Phase 3: Advanced patterns

- `.nest()` / router composition
- `axum::middleware::from_fn` detection (with TODO comments)
- Custom extractor passthrough
- `impl IntoResponse` detection and warning
- Cargo.toml dependency update (add/remove typeway, axum)

### Phase 4: Polish

- Interactive mode (`--interactive`) for ambiguous cases
- VSCode extension integration (convert selected code)
- `--partial` flag to convert only specific routes
- Roundtrip test suite: convert Axum → Typeway → Axum and assert semantic equivalence
