# Typeway Word

A Medium-style blogging platform built with typeway, implementing the [RealWorld](https://github.com/gothinkster/realworld) API spec.

This demonstrates typeway in a realistic application with:
- 19 endpoints defined as a single API type
- Elm + Tailwind frontend (single-page app with client-side routing)
- JWT authentication via custom `AuthUser` extractor
- PostgreSQL via `tokio-postgres` + `deadpool`
- Password hashing with Argon2
- CORS and request ID middleware
- Structured JSON error responses
- Seed data: 10 articles about type-level programming
- Native static file serving and SPA fallback (no Axum needed)
- Docker Compose for one-command setup

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
| POST | /api/users | No | Register |
| POST | /api/users/login | No | Login |
| GET | /api/user | Yes | Current user |
| PUT | /api/user | Yes | Update user |
| GET | /api/profiles/:username | Optional | Get profile |
| POST | /api/profiles/:username/follow | Yes | Follow user |
| DELETE | /api/profiles/:username/follow | Yes | Unfollow user |
| GET | /api/articles | Optional | List articles |
| GET | /api/articles/feed | Yes | Feed (followed authors) |
| GET | /api/articles/:slug | Optional | Get article |
| POST | /api/articles | Yes | Create article |
| PUT | /api/articles/:slug | Yes | Update article |
| DELETE | /api/articles/:slug | Yes | Delete article |
| POST | /api/articles/:slug/favorite | Yes | Favorite |
| DELETE | /api/articles/:slug/favorite | Yes | Unfavorite |
| GET | /api/articles/:slug/comments | Optional | List comments |
| POST | /api/articles/:slug/comments | Yes | Add comment |
| DELETE | /api/articles/:slug/comments/:id | Yes | Delete comment |
| GET | /api/tags | No | List tags |

## Frontend

Any [RealWorld frontend](https://codebase.show/projects/realworld) works with this backend. Popular options:

- [React + Redux](https://github.com/gothinkster/react-redux-realworld-example-app)
- [Angular](https://github.com/gothinkster/angular-realworld-example-app)
- [Elm](https://github.com/rtfeldman/elm-spa-example)

Point the frontend's API URL at `http://localhost:4000/api`.
