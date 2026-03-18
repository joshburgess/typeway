# Wayward RealWorld Example

A full implementation of the [RealWorld](https://github.com/gothinkster/realworld) ("Conduit") API spec — a Medium clone backend — using wayward.

This demonstrates wayward in a realistic application with:
- 19 endpoints defined as a single API type
- JWT authentication via custom `AuthUser` extractor
- PostgreSQL via `tokio-postgres` + `deadpool`
- Password hashing with Argon2
- CORS and request ID middleware
- Structured JSON error responses

## Running

```sh
# 1. Create the database
createdb realworld

# 2. Run (uses default postgres://postgres:postgres@localhost:5432/realworld)
cargo run -p wayward-realworld

# Or with custom database config:
DATABASE_HOST=localhost DATABASE_NAME=realworld cargo run -p wayward-realworld
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

Point the frontend's API URL at `http://localhost:3000/api`.
