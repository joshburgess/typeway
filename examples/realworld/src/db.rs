//! Database access layer using tokio-postgres via deadpool.

use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;
use uuid::Uuid;

use wayward_server::error::JsonError;

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
    cfg.create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("failed to create database pool")
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

// ---------------------------------------------------------------------------
// User queries
// ---------------------------------------------------------------------------

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
