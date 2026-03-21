# Static Files & SPA Fallback

Typeway can serve static files alongside your API, and support
single-page applications with client-side routing.

## Static files

Serve files from a directory under a URL prefix:

```rust
Server::<API>::new(handlers)
    .with_static_files("/static", "./public")
    .serve(addr)
    .await?;
```

This maps:
- `GET /static/style.css` → `./public/style.css`
- `GET /static/js/app.js` → `./public/js/app.js`
- `GET /static/` → `./public/index.html` (auto-index)

MIME types are inferred from file extensions:

| Extension | Content-Type |
|-----------|-------------|
| `.html` | `text/html; charset=utf-8` |
| `.css` | `text/css; charset=utf-8` |
| `.js`, `.mjs` | `application/javascript; charset=utf-8` |
| `.json` | `application/json` |
| `.png` | `image/png` |
| `.svg` | `image/svg+xml` |
| `.woff2` | `font/woff2` |

Directory traversal (`..`) is blocked — returns 403 Forbidden.

## SPA fallback

For single-page apps with client-side routing (React, Vue, Elm):

```rust
Server::<API>::new(handlers)
    .with_static_files("/assets", "./dist/assets")
    .with_spa_fallback("./dist/index.html")
    .serve(addr)
    .await?;
```

How it works:
1. API routes are matched first (`/api/users`, etc.)
2. Static files are matched next (`/assets/style.css`)
3. Unmatched routes serve `index.html` (the SPA entry point)

This means `/dashboard`, `/settings/profile`, and any other
client-side route all serve the same `index.html`, letting the
JavaScript router handle navigation.

Paths with file extensions (`.js`, `.css`, `.png`) that don't match
a static file return 404 instead of the SPA fallback — this prevents
broken asset requests from serving HTML.

## Full example

```rust
type API = (
    GetEndpoint<ApiUsersPath, Vec<User>>,
    PostEndpoint<ApiUsersPath, CreateUser, User>,
);

Server::<API>::new((
    bind!(list_users),
    bind!(create_user),
))
.with_state(db)
.nest("/api")                              // API under /api prefix
.with_static_files("/assets", "./frontend/dist/assets")
.with_spa_fallback("./frontend/dist/index.html")
.serve("0.0.0.0:3000".parse()?)
.await?;
```

Result:
- `GET /api/users` → handler response (JSON)
- `GET /assets/app.js` → static file
- `GET /dashboard` → `index.html` (SPA takes over)
- `GET /` → `index.html`

## Combining with gRPC

Static files and gRPC work together:

```rust
Server::<API>::new(handlers)
    .with_grpc("MyService", "my.v1")
    .with_static_files("/static", "./public")
    .serve(addr)
    .await?;
```

The multiplexer routes by content type:
- `application/grpc*` → gRPC dispatch
- `GET /static/*` → file serving
- Everything else → REST handlers
