# Typeway Word -- RealWorld Example App

A full [RealWorld](https://github.com/gothinkster/realworld) (Medium clone) implementation showcasing every advanced feature of the typeway web framework.

This is not a toy example. It implements the complete RealWorld API spec -- user registration, authentication, articles, comments, favorites, tags, profiles, and following -- plus seven additional features that demonstrate what type-level web programming makes possible.

## What This Example Demonstrates

1. **The API Is a Type** -- The entire 22-endpoint API is defined as a single Rust type alias. Add an endpoint to the type, and the compiler forces you to add a handler.
2. **Compile-Time Handler Completeness** -- Forget a handler and the server does not compile. No runtime 404s for routes you thought you registered.
3. **Middleware Effects System** -- CORS requirements are declared in the API type. Comment out `.provide::<CorsRequired>()` and the build fails with a clear error.
4. **Content Negotiation** -- Tags and article endpoints return JSON, plain text, or XML based on the `Accept` header. XML support uses an explicit `RenderAsXml` trait.
5. **API Versioning (V1 -> V2 -> V3)** -- Three API versions with typed deltas: additions, replacements, deprecations, and removals. Backward compatibility is checked at compile time.
6. **Session-Typed WebSocket Protocol** -- The live article feed protocol is encoded as a session type. Each `.send()` transitions the channel state. Calling `.recv()` in a send state is a compile error.
7. **Request Body Validation** -- Registration and article creation use `Validated<V, E>` wrappers that reject invalid JSON with 422 before the handler runs.
8. **JWT Authentication** -- `Protected<AuthUser, E>` endpoints enforce that the handler accepts `AuthUser` as its first argument.
9. **Dual-Protocol gRPC** -- A single `.with_grpc("RealWorldService", "realworld.v1")` call enables gRPC (grpc+json) on the same port, reusing all existing handlers.

## Architecture

```
examples/realworld/src/
  api.rs       -- The API type: paths, endpoints, versioning, validators
  handlers.rs  -- Async handler functions for every endpoint
  models.rs    -- Request/response types, Display/RenderAsXml impls
  auth.rs      -- JWT token creation/verification, AuthUser extractor
  db.rs        -- PostgreSQL with migrations and seed data (10 articles)
  main.rs      -- Server construction with effects, state, and middleware
```

The flow is: `api.rs` defines the shape (a Rust type). `main.rs` constructs an `EffectfulServer<RealWorldAPI>` and passes a tuple of bound handlers. The compiler verifies that every endpoint in the API type has a corresponding handler with the correct signature. At runtime, the server matches incoming requests by method + path and dispatches to the appropriate handler.

## Feature Deep Dives

### 1. The API Is a Type

The entire V3 API is a single type alias -- a 22-element tuple of endpoint types:

```rust
pub type RealWorldV3Resolved = (
    Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
    Protected<AuthUser, GetEndpoint<UserPath, UserResponseV3>>,
    Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
    Requires<CorsRequired, GetEndpoint<ProfilePath, ProfileResponse>>,
    // ... 18 more endpoints
);
```

Each element describes one endpoint completely:
- `GetEndpoint<ArticlePath, ArticleResponse>` -- HTTP method, URL path, and response type
- `PostEndpoint<UsersPath, NewUserRequest, UserResponse>` -- includes request body type
- `Protected<AuthUser, _>` -- requires authentication
- `Requires<CorsRequired, _>` -- requires CORS middleware
- `Validated<V, _>` -- requires request body validation

Path types are defined using the `typeway_path!` macro:

```rust
typeway_path!(pub type ArticlePath = "api" / "articles" / String);
typeway_path!(pub type ArticleCommentPath = "api" / "articles" / String / "comments" / i32);
```

### 2. Compile-Time Handler Completeness

The server constructor takes a tuple of handlers that must match the API type exactly:

```rust
let server = EffectfulServer::<RealWorldAPI>::new((
    bind_validated!(handlers::register),
    bind_auth!(handlers::get_current_user_v3),
    bind_auth!(handlers::update_user),
    bind!(handlers::get_profile),
    // ... one handler per endpoint, in order
));
```

Three macros connect handlers to endpoints:
- `bind!(handler)` -- for public endpoints
- `bind_auth!(handler)` -- for `Protected<AuthUser, _>` endpoints (verifies the handler's first arg is `AuthUser`)
- `bind_validated!(handler)` -- for `Validated<V, _>` endpoints

If you add an endpoint to the API type and forget the handler, the compiler produces an error pointing at the handler tuple. If you provide a handler with the wrong argument types, the compiler catches that too.

### 3. Middleware Effects System

Some endpoints need CORS headers for browser access. Rather than hoping someone remembers to add CORS middleware, the API type declares the requirement:

```rust
// In api.rs: this endpoint REQUIRES CORS middleware
Requires<CorsRequired, GetEndpoint<ArticlesPath, ArticlesResponse>>,
```

In `main.rs`, the server must satisfy all declared effects before it can start:

```rust
let server = EffectfulServer::<RealWorldAPI>::new(handlers)
    .provide::<CorsRequired>()  // Mark CorsRequired as satisfied
    .layer(CorsLayer::permissive())  // Apply the actual middleware
    .ready();  // Only compiles if ALL effects are provided
```

If you comment out `.provide::<CorsRequired>()`, the call to `.ready()` fails with:

> "effect `CorsRequired` has not been provided"

This is a compile-time guarantee. You cannot start the server without providing every declared effect.

### 4. Content Negotiation

The tags and article endpoints support three content types: JSON, plain text, and XML.

**Handler signature:**

```rust
pub async fn get_tags_v2(
    accept: AcceptHeader,
    state: State<Db>,
) -> Result<NegotiatedResponse<TagsResponseV2, (JsonFormat, TextFormat, XmlFormat)>, JsonError> {
    // ...
    Ok(NegotiatedResponse::new(TagsResponseV2 { tags }, accept.0))
}
```

The `NegotiatedResponse<T, (F1, F2, F3)>` type examines the `Accept` header and picks the best format:

- **JsonFormat** -- uses the blanket `impl<T: Serialize> RenderAs<JsonFormat> for T`. Every `Serialize` type gets JSON for free.
- **TextFormat** -- uses the blanket `impl<T: Display> RenderAs<TextFormat> for T`. Requires a `Display` impl.
- **XmlFormat** -- requires an explicit `RenderAsXml` impl per type (no standard XML serialization trait in Rust).

**Try it:**

```sh
# JSON (default)
curl http://localhost:4000/api/tags

# Plain text (comma-separated with counts)
curl -H "Accept: text/plain" http://localhost:4000/api/tags

# XML
curl -H "Accept: application/xml" http://localhost:4000/api/tags
# Returns: <?xml version="1.0"?><tags><tag count="3">rust</tag>...</tags>

# Article as plain text
curl -H "Accept: text/plain" http://localhost:4000/api/articles/phantom-types-are-more-useful-than-you-think
# Returns: Phantom Types Are More Useful Than You Think by typelevel -- How zero-sized type parameters...

# Article as XML
curl -H "Accept: application/xml" http://localhost:4000/api/articles/phantom-types-are-more-useful-than-you-think
```

**How XML rendering works:**

Unlike JSON and text which have blanket impls, XML requires explicit `RenderAsXml` impls because there is no standard XML serialization trait in Rust:

```rust
impl RenderAsXml for TagsResponseV2 {
    fn to_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n<tags>\n");
        for t in &self.tags {
            xml.push_str(&format!("  <tag count=\"{}\">{}</tag>\n", t.count, t.tag));
        }
        xml.push_str("</tags>");
        xml
    }
}
```

The framework bridges this via a blanket `impl<T: RenderAsXml> RenderAs<XmlFormat> for T`, so any type with a `RenderAsXml` impl automatically works with `XmlFormat` in `NegotiatedResponse`.

### 5. API Versioning (V1 -> V2 -> V3)

This is the most involved feature. The API evolves across three versions, each expressed as a set of typed deltas.

**V1 -- The Baseline (19 endpoints)**

The original RealWorld spec: registration, login, CRUD for articles/comments/tags, profiles, favorites.

**V2 -- Significant Evolution (21 endpoints)**

Four changes from V1:

```rust
type V2Changes = (
    Added<GetEndpoint<HealthPath, HealthResponse>>,           // New: health check
    Added<GetEndpoint<ArticlesSearchPath, ArticlesResponse>>, // New: article search
    Replaced<                                                  // Tags now include counts
        Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponse>>,
        Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponseV2>>,
    >,
    Deprecated<PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>>, // Login deprecated
);
```

The change markers (`Added`, `Replaced`, `Deprecated`) are machine-readable documentation:
- `Added` -- new endpoint not in V1
- `Replaced` -- old endpoint type replaced with new type (same path, different response)
- `Deprecated` -- endpoint still works but clients should migrate away

V2Resolved is the full 21-endpoint tuple after applying these changes. The `VersionedApi` type carries the lineage:

```rust
pub type RealWorldV2 = VersionedApi<RealWorldV1, V2Changes, RealWorldV2Resolved>;
```

**V2 backward compatibility check:**

```rust
// Every V1 endpoint (except the replaced tags endpoint) must exist in V2.
// The compiler verifies this at build time.
typeway_core::assert_api_compatible!(
    (/* 18 preserved V1 endpoints */),
    RealWorldV2Resolved
);
```

If someone accidentally removes an endpoint during the V1->V2 evolution, this assertion produces a compile error.

**V3 -- Breaking Change (22 endpoints)**

Four changes from V2:

```rust
type V3Changes = (
    Added<GetEndpoint<StatsPath, StatsResponse>>,                // New: site statistics
    Added<Protected<AuthUser, DeleteEndpoint<UserPath, ()>>>,    // New: account deletion
    Replaced<                                                     // User response upgraded
        Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,
        Protected<AuthUser, GetEndpoint<UserPath, UserResponseV3>>,
    >,
    Removed<PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>>, // Login removed
);
```

V3 is a **breaking change** -- the login endpoint is gone. This means V3 is NOT backward compatible with V1:

```rust
// This would cause a compile error (login endpoint missing):
// typeway_core::assert_api_compatible!(
//     (PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,),
//     RealWorldV3Resolved
// );
```

But V3 does preserve all non-removed V2 endpoints (19 of them):

```rust
typeway_core::assert_api_compatible!(
    (/* 19 preserved V2 endpoints, excluding login and old UserResponse */),
    RealWorldV3Resolved
);
```

**New V3 types:**

- `StatsResponse { users: i64, articles: i64, comments: i64 }` -- site-wide counts
- `UserResponseV3 { user: UserBodyV3 }` where `UserBodyV3` adds `created_at: DateTime<Utc>` and `articles_count: i64`

**Why this matters:**

API versioning is usually a runtime concern -- version headers, URL prefixes, or config flags. Typeway makes it a compile-time concern. The type system records what changed between versions, and `assert_api_compatible!` catches unintentional breaking changes before the code ships.

### 6. Session-Typed WebSocket Protocol

The live article feed uses a session type to enforce protocol ordering:

```rust
type FeedProtocol = Send<ArticleUpdate, Rec<Send<ArticleUpdate, Var>>>;
```

This reads as: "Send one ArticleUpdate, then loop forever sending ArticleUpdates."

The handler implements this protocol step by step:

```rust
pub async fn ws_feed(upgrade: WebSocketUpgrade) -> Response<BoxBody> {
    upgrade.on_upgrade_typed::<FeedProtocol, _, _>(|ws| async move {
        // Step 1: Send welcome (Send<ArticleUpdate, ...> -> Rec<...>)
        let ws = ws.send(ArticleUpdate { event: "connected", ... }).await?;

        // Step 2: Enter the loop (Rec<Send<...>> -> Send<...>)
        let mut ws_loop = ws.enter();

        loop {
            // Step 3: Send an update (Send<ArticleUpdate, Var> -> Var)
            let ws_var = ws_loop.send(update).await?;

            // Step 4: Recurse (Var -> Rec -> Send)
            ws_loop = ws_var.recurse::<Send<ArticleUpdate, Var>>().enter();
        }
    })
}
```

Each `.send()` consumes the channel and returns it in the next state. If you tried to call `.recv()` when the protocol says `Send`, you would get a compile error. The type system enforces protocol ordering at every step.

### 7. Request Body Validation

Registration and article creation use `Validated<V, E>` wrappers:

```rust
// In the API type:
Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
```

The validator is a struct that implements the `Validate<T>` trait:

```rust
pub struct NewUserValidator;

impl Validate<NewUserRequest> for NewUserValidator {
    fn validate(body: &NewUserRequest) -> Result<(), String> {
        if body.user.username.is_empty() {
            return Err("username is required".into());
        }
        if body.user.password.len() < 6 {
            return Err("password must be at least 6 characters".into());
        }
        Ok(())
    }
}
```

When a request arrives, the framework deserializes the JSON body, runs the validator, and returns a 422 response if validation fails -- all before the handler function is called. The handler can assume its input is valid.

Validated endpoints use `bind_validated!()` instead of `bind!()` in the handler tuple.

### 8. Authentication

Protected endpoints use the `Protected<AuthUser, E>` wrapper:

```rust
Protected<AuthUser, GetEndpoint<UserPath, UserResponseV3>>,
```

`AuthUser` is a custom extractor that reads the `Authorization: Bearer <token>` header, verifies the JWT, and extracts the user ID. Protected handlers receive `AuthUser` as their first argument:

```rust
pub async fn get_current_user_v3(
    auth: AuthUser,        // Extracted from JWT -- compiler enforces this arg
    state: State<Db>,
) -> Result<Json<UserResponseV3>, JsonError> {
    // auth.0 is the user's UUID
}
```

`OptionalAuth` is also available for endpoints that work with or without authentication (e.g., article listing shows different data for authenticated users).

Protected endpoints use `bind_auth!()` in the handler tuple, which verifies at compile time that the handler's first argument is the auth type.

### 9. Dual-Protocol gRPC

A single method call enables gRPC (using grpc+json encoding) on the same port as REST:

```rust
server
    .with_grpc("RealWorldService", "realworld.v1")
    .layer(CorsLayer::permissive())
    .serve(addr)
    .await?;
```

All 22 REST endpoints are automatically available as gRPC methods. The same handler functions serve both protocols -- incoming `application/grpc*` requests are translated to REST calls by the gRPC bridge, routed through the same handlers, and the response is translated back to gRPC framing.

**Generate a `.proto` file:**

```sh
GENERATE_PROTO=1 cargo run -p typeway-realworld
# Writes realworld.proto with all service and message definitions
```

The proto file is derived from the API type and the `ToProtoType` impls on the domain models. No separate `.proto` source is needed -- the Rust types are the source of truth.

## Running with Docker (recommended)

```sh
cd examples/realworld
docker compose up
```

Open http://localhost:4000. Docker handles Postgres, builds the Rust backend, compiles the Elm frontend, seeds the database, and serves everything.

To reset the database:
```sh
docker compose down -v && docker compose up
```

## Running locally (without Docker)

Requires: Rust, Elm, PostgreSQL.

```sh
# 1. Create the database
createdb realworld

# 2. Build the Elm frontend
cd examples/realworld/frontend
elm make src/Main.elm --output=public/elm.js
cd ../../..

# 3. Run (from the workspace root)
cargo run -p typeway-realworld

# With custom database config:
DATABASE_HOST=localhost DATABASE_PORT=5432 DATABASE_USER=postgres \
  DATABASE_PASSWORD=postgres DATABASE_NAME=realworld \
  cargo run -p typeway-realworld
```

## API Endpoints (V3 -- 22 endpoints)

| Method | Path | Auth | Version | Features | Description |
|--------|------|------|---------|----------|-------------|
| POST | /api/users | No | V1 | Validated | Register |
| GET | /api/user | Yes | V3 | Protected | Current user (V3: + created_at, articles_count) |
| PUT | /api/user | Yes | V1 | Protected | Update user |
| DELETE | /api/user | Yes | V3 | Protected | Delete account |
| GET | /api/profiles/:username | Optional | V1 | CORS | Get profile |
| POST | /api/profiles/:username/follow | Yes | V1 | Protected | Follow user |
| DELETE | /api/profiles/:username/follow | Yes | V1 | Protected | Unfollow user |
| GET | /api/articles | Optional | V1 | CORS | List articles (?author=) |
| GET | /api/articles/feed | Yes | V1 | Protected | Feed (followed authors) |
| GET | /api/articles/search | Optional | V2 | -- | Search articles (?q=) |
| GET | /api/articles/:slug | Optional | V1 | CORS, Negotiated | Get article (JSON/text/XML) |
| POST | /api/articles | Yes | V1 | Protected, Validated | Create article |
| PUT | /api/articles/:slug | Yes | V1 | Protected | Update article |
| DELETE | /api/articles/:slug | Yes | V1 | Protected | Delete article |
| POST | /api/articles/:slug/favorite | Yes | V1 | Protected | Favorite |
| DELETE | /api/articles/:slug/favorite | Yes | V1 | Protected | Unfavorite |
| GET | /api/articles/:slug/comments | Optional | V1 | CORS | List comments |
| POST | /api/articles/:slug/comments | Yes | V1 | Protected | Add comment |
| DELETE | /api/articles/:slug/comments/:id | Yes | V1 | Protected | Delete comment |
| GET | /api/tags | No | V2 | CORS, Negotiated | Tags with counts (JSON/text/XML) |
| GET | /api/health | No | V2 | -- | Health check |
| GET | /api/stats | No | V3 | -- | Site statistics |

**Removed in V3:** `POST /api/users/login` (was deprecated in V2, removed in V3).

**Negotiated** means the endpoint supports content negotiation -- send `Accept: text/plain` or `Accept: application/xml` to get alternative formats.

## Try Breaking Things

These experiments demonstrate the compile-time guarantees. Try each one and observe the compiler output.

**1. Remove a CORS effect:**
In `main.rs`, comment out `.provide::<CorsRequired>()`. Run `cargo check -p typeway-realworld`. The compiler reports that the CorsRequired effect has not been provided.

**2. Remove a handler:**
In `main.rs`, comment out any `bind!()` line from the handler tuple. Run `cargo check`. The compiler reports a tuple size mismatch -- the API has 22 endpoints but you only provided 21 handlers.

**3. Add an endpoint without a handler:**
In `api.rs`, add a new endpoint to `RealWorldV3Resolved` (e.g., `GetEndpoint<StatsPath, HealthResponse>`). Don't add a handler. Run `cargo check`. The compiler forces you to add the handler.

**4. Return the wrong type from a handler:**
Change `get_stats` to return `Json<HealthResponse>` instead of `Json<StatsResponse>`. Run `cargo check`. The trait bounds fail because the handler return type does not match the endpoint.

**5. Test content negotiation:**
```sh
# JSON (default)
curl -s http://localhost:4000/api/tags | head -c 200

# Plain text
curl -s -H "Accept: text/plain" http://localhost:4000/api/tags

# XML
curl -s -H "Accept: application/xml" http://localhost:4000/api/tags

# Article as plain text
curl -s -H "Accept: text/plain" http://localhost:4000/api/articles/phantom-types-are-more-useful-than-you-think

# Article as XML
curl -s -H "Accept: application/xml" http://localhost:4000/api/articles/phantom-types-are-more-useful-than-you-think
```

**6. Verify backward compatibility:**
In `api.rs`, uncomment the failing `assert_api_compatible!` block for V1->V3. Run `cargo check`. The compiler reports that the login endpoint is missing from V3 -- proving that V3 is not backward compatible with V1.

## Frontend

Any [RealWorld frontend](https://codebase.show/projects/realworld) works with this backend. The included Elm + Tailwind frontend is served at the root URL. Point other frontends at `http://localhost:4000/api`.
