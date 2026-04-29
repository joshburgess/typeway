//! Database access layer using tokio-postgres via deadpool.

use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use uuid::Uuid;

use typeway_server::error::JsonError;

pub use crate::models::ArticleRow;
use crate::models::*;

pub type Db = Pool;

pub async fn create_pool() -> Pool {
    let mut cfg = Config::new();
    cfg.host = Some(std::env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".into()));
    cfg.port = Some(
        std::env::var("DATABASE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432),
    );
    cfg.dbname = Some(std::env::var("DATABASE_NAME").unwrap_or_else(|_| "realworld".into()));
    cfg.user = Some(std::env::var("DATABASE_USER").unwrap_or_else(|_| "postgres".into()));
    cfg.password = Some(std::env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "postgres".into()));

    // Use Verified recycling: runs SELECT 1 before handing out a recycled
    // connection. The default (Fast) only checks is_closed() which misses
    // half-open TCP sockets and silently dead connections.
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Verified,
    });

    cfg.pool = Some(deadpool_postgres::PoolConfig {
        max_size: 16,
        timeouts: deadpool_postgres::Timeouts {
            wait: Some(std::time::Duration::from_secs(5)),
            create: Some(std::time::Duration::from_secs(5)),
            recycle: Some(std::time::Duration::from_secs(5)),
        },
        ..Default::default()
    });

    // TCP keepalive prevents firewalls/LBs from killing idle connections.
    cfg.keepalives = Some(true);
    cfg.keepalives_idle = Some(std::time::Duration::from_secs(30));
    cfg.connect_timeout = Some(std::time::Duration::from_secs(5));

    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("failed to create database pool");

    // Fail fast at startup if the database is unreachable.
    let _ = pool
        .get()
        .await
        .expect("failed to connect to database — check DATABASE_* env vars");

    pool
}

pub async fn run_migrations(pool: &Pool) {
    let client = pool.get().await.expect("failed to get db connection");
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            username TEXT UNIQUE NOT NULL,
            email TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            bio TEXT,
            image TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS follows (
            follower_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            followed_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            PRIMARY KEY (follower_id, followed_id)
        );

        CREATE TABLE IF NOT EXISTS articles (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            slug TEXT UNIQUE NOT NULL,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            body TEXT NOT NULL,
            author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS tags (
            article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
            tag TEXT NOT NULL,
            PRIMARY KEY (article_id, tag)
        );

        CREATE TABLE IF NOT EXISTS favorites (
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
            PRIMARY KEY (user_id, article_id)
        );

        CREATE TABLE IF NOT EXISTS comments (
            id SERIAL PRIMARY KEY,
            body TEXT NOT NULL,
            author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
        )
        .await
        .expect("failed to run migrations");
}

pub async fn seed_data(pool: &Pool) {
    let client = pool.get().await.expect("failed to get db connection");

    // Check if seed data already exists.
    let count: i64 = client
        .query_one(
            "SELECT COUNT(*) as cnt FROM users WHERE username = 'typelevel'",
            &[],
        )
        .await
        .expect("failed to check seed data")
        .get("cnt");

    if count > 0 {
        eprintln!("Seed data already present, skipping.");
        return;
    }

    eprintln!("Seeding database with sample articles...");

    // Create a seed author. Password: "typelevel123"
    let password_hash = "$argon2id$v=19$m=19456,t=2,p=1$salt1234salt1234$dummyhashforseeding000000000000000000000000";
    let author_id: uuid::Uuid = client
        .query_one(
            "INSERT INTO users (username, email, password_hash, bio) \
             VALUES ('typelevel', 'typelevel@example.com', $1, 'Writing about type-level programming, Rust, and Haskell.') \
             RETURNING id",
            &[&password_hash],
        )
        .await
        .expect("failed to create seed author")
        .get("id");

    let articles: Vec<(&str, &str, &str, &[&str])> = vec![
        (
            "Phantom Types Are More Useful Than You Think",
            "How zero-sized type parameters can enforce invariants at compile time",
            "Phantom types carry no runtime data but constrain what operations are valid. \
             In Rust, PhantomData<T> lets you tie a type parameter to a struct without storing T. \
             This is the foundation of typeway's Endpoint<M, P, Req, Res> — the method, path, \
             request body, and response types exist only at compile time.\n\n\
             Consider a state machine encoded as types: struct Locked; struct Unlocked; \
             struct Door<State>(PhantomData<State>). The open() method only exists on \
             Door<Unlocked>. No runtime check needed — the compiler enforces it.\n\n\
             This pattern scales to web frameworks. An API type is a phantom-typed description \
             of your entire HTTP surface. The server verifies handlers match. The client derives \
             calls. The OpenAPI spec is generated. All from one type.",
            &["rust", "type-theory", "phantom-types"],
        ),
        (
            "HLists: Heterogeneous Lists for Type-Level Programming",
            "Why recursive type-level lists beat flat tuples for path encoding",
            "An HList (heterogeneous list) is a type-level linked list: HCons<Head, Tail> with \
             HNil as the terminator. Unlike tuples, HLists support structural recursion — you can \
             write one trait impl for HCons and one for HNil, and it works for any length.\n\n\
             This matters for URL paths. /users/:id/posts is naturally recursive: match 'users', \
             then recurse on /:id/posts. An HList encodes this directly: \
             HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HNil>>>.\n\n\
             Flat tuples would need a separate impl for every possible combination of literal \
             and capture segments. HLists give O(n) impls via recursion. This is why Servant \
             in Haskell and typeway in Rust both use list-like structures for paths.",
            &["rust", "haskell", "hlists", "type-level"],
        ),
        (
            "The Catamorphism Hiding in Your Web Framework",
            "Path parsing as a fold over type-level structure",
            "A catamorphism is a fold — the fundamental way to consume an inductive data structure. \
             When typeway's PathSpec trait computes the capture tuple from a path HList, it's \
             performing a type-level catamorphism.\n\n\
             Base case: HNil has Captures = (). Recursive case: HCons<Capture<T>, Tail> prepends T \
             to Tail::Captures. HCons<Lit<S>, Tail> passes through Tail::Captures unchanged.\n\n\
             The runtime parser mirrors this exactly: extract() recurses over path segments, \
             parsing captures and matching literals. The type-level fold and the value-level fold \
             are the same algorithm at different levels of abstraction. This is the Curry-Howard \
             correspondence in action.",
            &["type-theory", "catamorphism", "curry-howard"],
        ),
        (
            "Type Erasure: The Bridge Between Static Types and Dynamic Dispatch",
            "How typeway stores heterogeneous handlers in a flat vector",
            "Type-level programming gives you compile-time guarantees, but at some point you need \
             runtime dispatch. typeway's handlers are all different types — an async fn(Path<P>) -> Json<T> \
             is a different type from async fn(State<S>) -> String. Yet they must live in the same Vec.\n\n\
             The solution: type erasure via Box<dyn Fn(Parts, Bytes) -> Pin<Box<dyn Future>>>. \
             The bind() function captures the handler's specific type, verifies it against the \
             endpoint at compile time, then erases it for storage. The type check happens once \
             at compilation. The erasure adds ~878ns per dispatch — less than 0.1% of typical \
             handler execution time.\n\n\
             This is the fundamental trade-off of type-level web frameworks: pay a small runtime \
             cost for strong compile-time guarantees.",
            &["rust", "type-erasure", "performance"],
        ),
        (
            "Servant vs Typeway: Type-Level Web Frameworks Across Languages",
            "Comparing Haskell's pioneer with Rust's new contender",
            "Haskell's Servant pioneered the idea that an HTTP API can be a type. Typeway brings \
             that idea to Rust. Both derive server, client, and OpenAPI from one type. But the \
             implementations differ due to language constraints.\n\n\
             Servant uses GHC's type-level strings and type operators (:<|>, :>). Typeway uses \
             HLists with marker types and trait-level computation — achieving similar results \
             without nightly Rust features.\n\n\
             Servant's ecosystem is fragmented across 10+ packages. Typeway ships everything in \
             one workspace. Servant has no middleware story; typeway inherits Tower's entire \
             ecosystem. Servant's compile times were a sore spot in its early years and have \
             improved meaningfully since; typeway leans on flat tuple impls from day one to \
             stay linear in API surface.\n\n\
             The biggest difference: typeway integrates with Axum bidirectionally. You can adopt \
             it incrementally. Servant is all-or-nothing.",
            &["haskell", "rust", "servant", "comparison"],
        ),
        (
            "Zero-Cost Abstractions and the Lies We Tell Ourselves",
            "Measuring the actual overhead of type-level programming in Rust",
            "Rust promises zero-cost abstractions, but type-level frameworks add real overhead. \
             How much? We measured it.\n\n\
             Direct async fn call: 0.79 ns. Through typeway's BoxedHandler: 878 ns. That's two \
             heap allocations (closure box + future box) plus a virtual call. Each extractor adds \
             ~300 ns for a TypeId hashmap lookup.\n\n\
             Is this zero-cost? No. But for an API handler that takes 1-100ms to query a database \
             and serialize a response, 878 ns is 0.001-0.09% of the total time. You literally \
             cannot measure it in production.\n\n\
             The real cost is compile time, not runtime. Type-level programming makes rustc do \
             more work. typeway mitigates this with flat tuple impls instead of recursive trait \
             chains, keeping compile times linear.",
            &["rust", "performance", "benchmarks", "zero-cost"],
        ),
        (
            "Dependent Types in Disguise: How Trait Bounds Simulate Pi Types",
            "Rust's trait system is more powerful than you think",
            "A Pi type (dependent function type) says: 'for this specific input type, the output \
             must have this specific shape.' Rust can't express Pi types directly, but trait \
             bounds approximate them.\n\n\
             Handler<Route<GET, Path, NoBody, User>> is approximately a Pi type: for this specific \
             route, the handler must accept Path::Captures as arguments and return something that \
             implements IntoResponse. The compiler checks this at every call site.\n\n\
             The Serves<API> trait is another approximation: for this specific API type (a tuple \
             of routes), the handler tuple must have exactly one handler per route, each matching \
             its route's type signature. Miss one, and the compiler rejects it.\n\n\
             We're encoding dependent types in Rust's trait solver. It works. The error messages \
             need help (hence #[diagnostic::on_unimplemented]), but the guarantees are real.",
            &["type-theory", "dependent-types", "rust", "pi-types"],
        ),
        (
            "Tower of Abstractions: Why Middleware Matters for Type-Safe Frameworks",
            "How typeway inherits years of production-hardened middleware for free",
            "Most type-safe web frameworks reinvent middleware from scratch. Servant has no standard \
             middleware story. Dropshot has limited layer support. typeway takes a different approach: \
             it implements tower::Service and gets the entire Tower ecosystem for free.\n\n\
             CorsLayer, TraceLayer, TimeoutLayer, CompressionLayer, RateLimitLayer — all battle-tested \
             in production by Axum, Tonic, and thousands of other services. typeway's .layer() method \
             accepts any Tower layer. Write a custom layer once, use it with typeway, Axum, and gRPC.\n\n\
             This isn't just convenience — it's a network effect. Every Tower middleware written by \
             anyone in the Rust ecosystem is automatically available to typeway users. No other \
             type-safe framework offers this.",
            &["rust", "tower", "middleware", "ecosystem"],
        ),
        (
            "From Types to OpenAPI: Deriving Documentation From Code",
            "How typeway generates OpenAPI 3.1 specs without annotations",
            "Most frameworks require you to annotate your code with documentation metadata — \
             #[openapi(description = '...')], YAML files, or separate spec documents. These \
             inevitably drift from the actual implementation.\n\n\
             typeway takes a different approach: the API type IS the spec. GetEndpoint<UsersPath, Json<Vec<User>>> \
             tells us the method (GET), the path (/users), and the response schema (array of User). \
             The OpenAPI generator walks the type at startup and produces a complete spec.\n\n\
             Path parameters come from Capture<T> segments. Request bodies come from the Req type parameter. \
             Response schemas come from the Res type parameter. The EndpointDoc trait adds optional \
             descriptions, tags, and operation IDs.\n\n\
             The result: your docs are always in sync with your code. Change the type, the spec updates \
             automatically. No YAML to forget.",
            &["openapi", "documentation", "type-level", "rust"],
        ),
        (
            "Compile-Time Completeness: Why Missing Handlers Should Be Errors",
            "The case for making your compiler verify your API contract",
            "In Axum, if you forget to register a handler for /users/:id, you get a 404 at runtime. \
             Maybe in production. Maybe at 3 AM. In typeway, you get a compile error.\n\n\
             The Serves<API> trait checks that the handler tuple has exactly the right number of \
             BoundHandler entries, one per endpoint in the API type. The compiler does this check at \
             build time with zero runtime cost.\n\n\
             This matters more than it sounds. API surfaces grow. Endpoints get added in one PR and \
             forgotten in another. Integration tests might not cover every route. A type-level check \
             catches the gap immediately, in every build, before any code runs.\n\n\
             The trade-off is ergonomics — you write bind!(handler) instead of .route('/path', handler). \
             But you never ship a 404 for a route you thought you implemented.",
            &["rust", "type-safety", "compile-time", "correctness"],
        ),
    ];

    for (title, description, body, article_tags) in &articles {
        let article_slug = slug::slugify(title);
        let article_id: uuid::Uuid = client
            .query_one(
                "INSERT INTO articles (slug, title, description, body, author_id) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
                &[&article_slug, title, description, body, &author_id],
            )
            .await
            .expect("failed to insert seed article")
            .get("id");

        for tag in *article_tags {
            client
                .execute(
                    "INSERT INTO tags (article_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    &[&article_id, tag],
                )
                .await
                .expect("failed to insert seed tag");
        }
    }

    eprintln!("Seeded 10 articles about type-level programming.");
}

// ---------------------------------------------------------------------------
// User queries
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub async fn find_user_by_email(pool: &Pool, email: &str) -> Result<UserRow, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT id, username, email, password_hash, bio, image FROM users WHERE email = $1",
            &[&email],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("user not found"))?;

    Ok(UserRow {
        id: row.get("id"),
        username: row.get("username"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        bio: row.get("bio"),
        image: row.get("image"),
    })
}

pub async fn find_user_by_id(pool: &Pool, id: Uuid) -> Result<UserRow, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT id, username, email, password_hash, bio, image FROM users WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("user not found"))?;

    Ok(UserRow {
        id: row.get("id"),
        username: row.get("username"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        bio: row.get("bio"),
        image: row.get("image"),
    })
}

pub async fn find_user_by_username(pool: &Pool, username: &str) -> Result<UserRow, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT id, username, email, password_hash, bio, image FROM users WHERE username = $1",
            &[&username],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("user not found"))?;

    Ok(UserRow {
        id: row.get("id"),
        username: row.get("username"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        bio: row.get("bio"),
        image: row.get("image"),
    })
}

pub async fn create_user(
    pool: &Pool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> Result<UserRow, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_one(
            "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) \
             RETURNING id, username, email, password_hash, bio, image",
            &[&username, &email, &password_hash],
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                JsonError::conflict("username or email already taken")
            } else {
                JsonError::internal(e.to_string())
            }
        })?;

    Ok(UserRow {
        id: row.get("id"),
        username: row.get("username"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        bio: row.get("bio"),
        image: row.get("image"),
    })
}

pub async fn update_user(
    pool: &Pool,
    id: Uuid,
    update: &UpdateUser,
    password_hash: Option<&str>,
) -> Result<UserRow, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_one(
            "UPDATE users SET \
                email = COALESCE($2, email), \
                username = COALESCE($3, username), \
                password_hash = COALESCE($4, password_hash), \
                bio = COALESCE($5, bio), \
                image = COALESCE($6, image), \
                updated_at = NOW() \
             WHERE id = $1 \
             RETURNING id, username, email, password_hash, bio, image",
            &[
                &id,
                &update.email,
                &update.username,
                &password_hash,
                &update.bio,
                &update.image,
            ],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    Ok(UserRow {
        id: row.get("id"),
        username: row.get("username"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        bio: row.get("bio"),
        image: row.get("image"),
    })
}

// ---------------------------------------------------------------------------
// Follow queries
// ---------------------------------------------------------------------------

pub async fn is_following(
    pool: &Pool,
    follower_id: Uuid,
    followed_id: Uuid,
) -> Result<bool, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT 1 FROM follows WHERE follower_id = $1 AND followed_id = $2",
            &[&follower_id, &followed_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(row.is_some())
}

pub async fn follow_user(
    pool: &Pool,
    follower_id: Uuid,
    followed_id: Uuid,
) -> Result<(), JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    client
        .execute(
            "INSERT INTO follows (follower_id, followed_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&follower_id, &followed_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(())
}

pub async fn unfollow_user(
    pool: &Pool,
    follower_id: Uuid,
    followed_id: Uuid,
) -> Result<(), JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    client
        .execute(
            "DELETE FROM follows WHERE follower_id = $1 AND followed_id = $2",
            &[&follower_id, &followed_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Article queries
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub async fn get_tags(pool: &Pool) -> Result<Vec<String>, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let rows = client
        .query("SELECT DISTINCT tag FROM tags ORDER BY tag", &[])
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(rows.iter().map(|r| r.get("tag")).collect())
}

pub async fn get_tags_for_article(pool: &Pool, article_id: Uuid) -> Result<Vec<String>, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let rows = client
        .query(
            "SELECT tag FROM tags WHERE article_id = $1 ORDER BY tag",
            &[&article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(rows.iter().map(|r| r.get("tag")).collect())
}

pub async fn favorites_count(pool: &Pool, article_id: Uuid) -> Result<i64, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_one(
            "SELECT COUNT(*) as cnt FROM favorites WHERE article_id = $1",
            &[&article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(row.get("cnt"))
}

pub async fn is_favorited(pool: &Pool, user_id: Uuid, article_id: Uuid) -> Result<bool, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT 1 FROM favorites WHERE user_id = $1 AND article_id = $2",
            &[&user_id, &article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    Ok(row.is_some())
}
