# Typeway Word

A Medium-style blogging platform built with typeway, implementing the [RealWorld](https://github.com/gothinkster/realworld) API spec.

This demonstrates typeway in a realistic application with:
- 20 endpoints defined as a single API type (V2, evolved from V1)
- Elm + Tailwind frontend (single-page app with client-side routing)
- JWT authentication via custom `AuthUser` extractor
- PostgreSQL via `tokio-postgres` + `deadpool`
- Password hashing with Argon2
- CORS and request ID middleware
- Structured JSON error responses
- Seed data: 10 articles about type-level programming
- Native static file serving and SPA fallback (no Axum needed)
- Docker Compose for one-command setup

## Advanced Features Showcased

### 1. Middleware Effects System (`api.rs`, `main.rs`)

Public-facing endpoints are wrapped in `Requires<CorsRequired, _>` in the API type. The server uses `EffectfulServer` which tracks provided effects at the type level. If you comment out `.provide::<CorsRequired>()` in `main.rs`, the server fails to compile with:

> "effect `CorsRequired` has not been provided"

### 2. Content Negotiation (`handlers.rs`)

The `GET /api/tags` endpoint returns `NegotiatedResponse<TagsResponse, (JsonFormat, TextFormat)>`. The framework automatically selects JSON or plain text based on the `Accept` header:

```sh
# JSON (default)
curl http://localhost:4000/api/tags

# Plain text (comma-separated list)
curl -H "Accept: text/plain" http://localhost:4000/api/tags
```

### 3. API Versioning (`api.rs`)

The API is expressed as two versions:
- `RealWorldV1`: the original 19 endpoints
- `RealWorldV2`: V1 + a health check endpoint, expressed as `VersionedApi<V1, V2Changes, V2Resolved>`

The `assert_api_compatible!` macro verifies at compile time that every V1 endpoint exists in V2. If you accidentally remove an endpoint during evolution, the compiler catches it.

### 4. Session-Typed WebSocket (`api.rs`, `handlers.rs`)

A live article feed protocol is defined as a session type:

```rust
type FeedProtocol = Send<ArticleUpdate, Rec<Send<ArticleUpdate, Var>>>;
```

The `TypedWebSocket<FeedProtocol>` channel enforces protocol ordering at the type level: each `.send()` consumes the channel and returns it in the next state. Calling `.recv()` in a `Send` state is a compile error.

### 5. Request Body Validation (`api.rs`)

Registration and article creation endpoints use `Validated<V, E>` wrappers:

```rust
Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>
```

The `NewUserValidator` checks username presence, email format, and password length. Invalid requests get a 422 response before the handler runs — no validation code needed in the handler itself.

## Running with Docker (recommended)

```sh
cd examples/realworld
docker compose up
```

Open http://localhost:4000. That's it — Docker handles Postgres, builds the Rust backend, compiles the Elm frontend, seeds the database, and serves everything.

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

## API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | /api/users | No | Register (validated) |
| POST | /api/users/login | No | Login |
| GET | /api/user | Yes | Current user |
| PUT | /api/user | Yes | Update user |
| GET | /api/profiles/:username | Optional | Get profile |
| POST | /api/profiles/:username/follow | Yes | Follow user |
| DELETE | /api/profiles/:username/follow | Yes | Unfollow user |
| GET | /api/articles | Optional | List articles |
| GET | /api/articles/feed | Yes | Feed (followed authors) |
| GET | /api/articles/:slug | Optional | Get article |
| POST | /api/articles | Yes | Create article (validated) |
| PUT | /api/articles/:slug | Yes | Update article |
| DELETE | /api/articles/:slug | Yes | Delete article |
| POST | /api/articles/:slug/favorite | Yes | Favorite |
| DELETE | /api/articles/:slug/favorite | Yes | Unfavorite |
| GET | /api/articles/:slug/comments | Optional | List comments |
| POST | /api/articles/:slug/comments | Yes | Add comment |
| DELETE | /api/articles/:slug/comments/:id | Yes | Delete comment |
| GET | /api/tags | No | List tags (content negotiation) |
| GET | /api/health | No | Health check (V2 addition) |

## Frontend

Any [RealWorld frontend](https://codebase.show/projects/realworld) works with this backend. Popular options:

- [React + Redux](https://github.com/gothinkster/react-redux-realworld-example-app)
- [Angular](https://github.com/gothinkster/angular-realworld-example-app)
- [Elm](https://github.com/rtfeldman/elm-spa-example)

Point the frontend's API URL at `http://localhost:4000/api`.
