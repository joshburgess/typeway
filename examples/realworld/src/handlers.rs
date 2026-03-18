//! Request handlers for the RealWorld API.

use argon2::{password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use uuid::Uuid;

use wayward_server::error::JsonError;
use wayward_server::extract::{Path, State};
use wayward_server::response::Json;

use crate::api::*;
use crate::auth::{create_token, AuthUser, OptionalAuth};
use crate::db::{self, Db};
use crate::models::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hash_password(password: &str) -> Result<String, JsonError> {
    let salt = SaltString::generate(rand_core::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| JsonError::internal(format!("password hashing failed: {e}")))
}

fn verify_password(password: &str, hash: &str) -> Result<(), JsonError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| JsonError::internal(format!("invalid password hash: {e}")))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| JsonError::unauthorized("invalid email or password"))
}

fn user_response(user: &UserRow, token: String) -> UserResponse {
    UserResponse {
        user: UserBody {
            email: user.email.clone(),
            token,
            username: user.username.clone(),
            bio: user.bio.clone(),
            image: user.image.clone(),
        },
    }
}

async fn build_profile(
    pool: &Db,
    user: &UserRow,
    viewer_id: Option<Uuid>,
) -> Result<ProfileBody, JsonError> {
    let following = match viewer_id {
        Some(vid) => db::is_following(pool, vid, user.id).await?,
        None => false,
    };
    Ok(ProfileBody {
        username: user.username.clone(),
        bio: user.bio.clone(),
        image: user.image.clone(),
        following,
    })
}

async fn build_article(
    pool: &Db,
    row: &db::ArticleRow,
    viewer_id: Option<Uuid>,
) -> Result<ArticleBody, JsonError> {
    let author = db::find_user_by_id(pool, row.author_id).await?;
    let profile = build_profile(pool, &author, viewer_id).await?;
    let tags = db::get_tags_for_article(pool, row.id).await?;
    let fav_count = db::favorites_count(pool, row.id).await?;
    let favorited = match viewer_id {
        Some(vid) => db::is_favorited(pool, vid, row.id).await?,
        None => false,
    };

    Ok(ArticleBody {
        slug: row.slug.clone(),
        title: row.title.clone(),
        description: row.description.clone(),
        body: row.body.clone(),
        tag_list: tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
        favorited,
        favorites_count: fav_count,
        author: profile,
    })
}

/// Build a list of articles from a single joined query (avoids N+1).
async fn build_articles_from_query(
    pool: &Db,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    viewer_id: Option<Uuid>,
) -> Result<Vec<ArticleBody>, JsonError> {
    let client = pool
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let rows = client
        .query(query, params)
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let mut articles = Vec::with_capacity(rows.len());
    for row in &rows {
        let tags_str: Option<String> = row.get("tags");
        let tag_list: Vec<String> = tags_str
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let fav_count: i64 = row.get("favorites_count");
        let is_fav: bool = row.try_get("is_favorited").unwrap_or(false);

        articles.push(ArticleBody {
            slug: row.get("slug"),
            title: row.get("title"),
            description: row.get("description"),
            body: row.get("body"),
            tag_list,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            favorited: is_fav,
            favorites_count: fav_count,
            author: ProfileBody {
                username: row.get("author_username"),
                bio: row.get("author_bio"),
                image: row.get("author_image"),
                following: row.try_get("is_following").unwrap_or(false),
            },
        });
    }
    Ok(articles)
}

const ARTICLES_QUERY_BASE: &str = "\
    SELECT a.slug, a.title, a.description, a.body, a.author_id, \
           a.created_at, a.updated_at, \
           u.username AS author_username, u.bio AS author_bio, u.image AS author_image, \
           COALESCE(STRING_AGG(DISTINCT t.tag, ',' ORDER BY t.tag), '') AS tags, \
           COUNT(DISTINCT f.user_id) AS favorites_count, \
           FALSE AS is_favorited, \
           FALSE AS is_following \
    FROM articles a \
    JOIN users u ON u.id = a.author_id \
    LEFT JOIN tags t ON t.article_id = a.id \
    LEFT JOIN favorites f ON f.article_id = a.id";

const ARTICLES_QUERY_TAIL: &str = " GROUP BY a.id, u.id ORDER BY a.created_at DESC LIMIT 20";

// Full query (no WHERE filter).
fn articles_query() -> String {
    format!("{ARTICLES_QUERY_BASE}{ARTICLES_QUERY_TAIL}")
}

const FEED_QUERY: &str = "\
    SELECT a.slug, a.title, a.description, a.body, a.author_id, \
           a.created_at, a.updated_at, \
           u.username AS author_username, u.bio AS author_bio, u.image AS author_image, \
           COALESCE(STRING_AGG(DISTINCT t.tag, ',' ORDER BY t.tag), '') AS tags, \
           COUNT(DISTINCT fav.user_id) AS favorites_count, \
           FALSE AS is_favorited, \
           TRUE AS is_following \
    FROM articles a \
    JOIN users u ON u.id = a.author_id \
    JOIN follows fo ON fo.followed_id = a.author_id AND fo.follower_id = $1 \
    LEFT JOIN tags t ON t.article_id = a.id \
    LEFT JOIN favorites fav ON fav.article_id = a.id \
    GROUP BY a.id, u.id \
    ORDER BY a.created_at DESC \
    LIMIT 20";

// ---------------------------------------------------------------------------
// Auth handlers
// ---------------------------------------------------------------------------

pub async fn register(
    state: State<Db>,
    body: Json<NewUserRequest>,
) -> Result<Json<UserResponse>, JsonError> {
    let input = &body.0.user;
    if input.username.is_empty() || input.email.is_empty() || input.password.is_empty() {
        return Err(JsonError::unprocessable(
            "username, email, and password are required",
        ));
    }

    let pw_hash = hash_password(&input.password)?;
    let user = db::create_user(&state.0, &input.username, &input.email, &pw_hash).await?;
    let token = create_token(user.id)?;
    Ok(Json(user_response(&user, token)))
}

pub async fn login(
    state: State<Db>,
    body: Json<LoginRequest>,
) -> Result<Json<UserResponse>, JsonError> {
    let input = &body.0.user;
    let user = db::find_user_by_email(&state.0, &input.email)
        .await
        .map_err(|_| JsonError::unauthorized("invalid email or password"))?;
    verify_password(&input.password, &user.password_hash)?;
    let token = create_token(user.id)?;
    Ok(Json(user_response(&user, token)))
}

pub async fn get_current_user(
    auth: AuthUser,
    state: State<Db>,
) -> Result<Json<UserResponse>, JsonError> {
    let user = db::find_user_by_id(&state.0, auth.0).await?;
    let token = create_token(user.id)?;
    Ok(Json(user_response(&user, token)))
}

pub async fn update_user(
    auth: AuthUser,
    state: State<Db>,
    body: Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, JsonError> {
    let pw_hash = match &body.0.user.password {
        Some(pw) => Some(hash_password(pw)?),
        None => None,
    };
    let user = db::update_user(&state.0, auth.0, &body.0.user, pw_hash.as_deref()).await?;
    let token = create_token(user.id)?;
    Ok(Json(user_response(&user, token)))
}

// ---------------------------------------------------------------------------
// Profile handlers
// ---------------------------------------------------------------------------

pub async fn get_profile(
    path: Path<ProfilePath>,
    opt_auth: OptionalAuth,
    state: State<Db>,
) -> Result<Json<ProfileResponse>, JsonError> {
    let (username,) = path.0;
    let user = db::find_user_by_username(&state.0, &username).await?;
    let profile = build_profile(&state.0, &user, opt_auth.0).await?;
    Ok(Json(ProfileResponse { profile }))
}

pub async fn follow_profile(
    auth: AuthUser,
    path: Path<ProfileFollowPath>,
    state: State<Db>,
) -> Result<Json<ProfileResponse>, JsonError> {
    let (username,) = path.0;
    let target = db::find_user_by_username(&state.0, &username).await?;
    db::follow_user(&state.0, auth.0, target.id).await?;
    let profile = build_profile(&state.0, &target, Some(auth.0)).await?;
    Ok(Json(ProfileResponse { profile }))
}

pub async fn unfollow_profile(
    auth: AuthUser,
    path: Path<ProfileFollowPath>,
    state: State<Db>,
) -> Result<Json<ProfileResponse>, JsonError> {
    let (username,) = path.0;
    let target = db::find_user_by_username(&state.0, &username).await?;
    db::unfollow_user(&state.0, auth.0, target.id).await?;
    let profile = build_profile(&state.0, &target, Some(auth.0)).await?;
    Ok(Json(ProfileResponse { profile }))
}

// ---------------------------------------------------------------------------
// Article handlers
// ---------------------------------------------------------------------------

pub async fn list_articles(
    uri: http::Uri,
    _opt_auth: OptionalAuth,
    state: State<Db>,
) -> Result<Json<ArticlesResponse>, JsonError> {
    // Parse optional ?author=username query param.
    let author_filter: Option<String> = uri.query().and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("author=").map(|v| v.to_string()))
    });

    let articles = if let Some(ref author) = author_filter {
        build_articles_from_query(
            &state.0,
            &format!("{ARTICLES_QUERY_BASE} WHERE u.username = $1 {ARTICLES_QUERY_TAIL}"),
            &[author],
            None,
        )
        .await?
    } else {
        build_articles_from_query(&state.0, &articles_query(), &[], None).await?
    };

    let count = articles.len();
    Ok(Json(ArticlesResponse {
        articles,
        articles_count: count,
    }))
}

pub async fn get_feed(
    auth: AuthUser,
    state: State<Db>,
) -> Result<Json<ArticlesResponse>, JsonError> {
    let articles =
        build_articles_from_query(&state.0, FEED_QUERY, &[&auth.0], Some(auth.0)).await?;
    let count = articles.len();
    Ok(Json(ArticlesResponse {
        articles,
        articles_count: count,
    }))
}

pub async fn get_article(
    path: Path<ArticlePath>,
    opt_auth: OptionalAuth,
    state: State<Db>,
) -> Result<Json<ArticleResponse>, JsonError> {
    let (slug,) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT id, slug, title, description, body, author_id, created_at, updated_at \
             FROM articles WHERE slug = $1",
            &[&slug],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("article not found"))?;

    let ar = db::ArticleRow {
        id: row.get("id"),
        slug: row.get("slug"),
        title: row.get("title"),
        description: row.get("description"),
        body: row.get("body"),
        author_id: row.get("author_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    let article = build_article(&state.0, &ar, opt_auth.0).await?;
    Ok(Json(ArticleResponse { article }))
}

pub async fn create_article(
    auth: AuthUser,
    state: State<Db>,
    body: Json<NewArticleRequest>,
) -> Result<(http::StatusCode, Json<ArticleResponse>), JsonError> {
    let input = &body.0.article;
    let article_slug = slug::slugify(&input.title);

    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_one(
            "INSERT INTO articles (slug, title, description, body, author_id) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, slug, title, description, body, author_id, created_at, updated_at",
            &[
                &article_slug,
                &input.title,
                &input.description,
                &input.body,
                &auth.0,
            ],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let article_id: Uuid = row.get("id");

    // Insert tags
    if let Some(ref tags) = input.tag_list {
        for tag in tags {
            client
                .execute(
                    "INSERT INTO tags (article_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    &[&article_id, &tag],
                )
                .await
                .map_err(|e| JsonError::internal(e.to_string()))?;
        }
    }

    let ar = db::ArticleRow {
        id: article_id,
        slug: row.get("slug"),
        title: row.get("title"),
        description: row.get("description"),
        body: row.get("body"),
        author_id: row.get("author_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    let article = build_article(&state.0, &ar, Some(auth.0)).await?;
    Ok((http::StatusCode::CREATED, Json(ArticleResponse { article })))
}

pub async fn update_article(
    auth: AuthUser,
    path: Path<ArticlePath>,
    state: State<Db>,
    body: Json<UpdateArticleRequest>,
) -> Result<Json<ArticleResponse>, JsonError> {
    let (slug,) = path.0;
    let input = &body.0.article;
    let new_slug = input.title.as_ref().map(slug::slugify);

    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_one(
            "UPDATE articles SET \
                slug = COALESCE($2, slug), \
                title = COALESCE($3, title), \
                description = COALESCE($4, description), \
                body = COALESCE($5, body), \
                updated_at = NOW() \
             WHERE slug = $1 AND author_id = $6 \
             RETURNING id, slug, title, description, body, author_id, created_at, updated_at",
            &[
                &slug,
                &new_slug,
                &input.title,
                &input.description,
                &input.body,
                &auth.0,
            ],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let ar = db::ArticleRow {
        id: row.get("id"),
        slug: row.get("slug"),
        title: row.get("title"),
        description: row.get("description"),
        body: row.get("body"),
        author_id: row.get("author_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    let article = build_article(&state.0, &ar, Some(auth.0)).await?;
    Ok(Json(ArticleResponse { article }))
}

pub async fn delete_article(
    auth: AuthUser,
    path: Path<ArticlePath>,
    state: State<Db>,
) -> Result<http::StatusCode, JsonError> {
    let (slug,) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let n = client
        .execute(
            "DELETE FROM articles WHERE slug = $1 AND author_id = $2",
            &[&slug, &auth.0],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    if n == 0 {
        return Err(JsonError::not_found(
            "article not found or not owned by you",
        ));
    }
    Ok(http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Favorite handlers
// ---------------------------------------------------------------------------

pub async fn favorite_article(
    auth: AuthUser,
    path: Path<ArticleFavoritePath>,
    state: State<Db>,
) -> Result<Json<ArticleResponse>, JsonError> {
    let (slug,) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT id, slug, title, description, body, author_id, created_at, updated_at \
             FROM articles WHERE slug = $1",
            &[&slug],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("article not found"))?;

    let article_id: Uuid = row.get("id");
    client
        .execute(
            "INSERT INTO favorites (user_id, article_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&auth.0, &article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let ar = db::ArticleRow {
        id: article_id,
        slug: row.get("slug"),
        title: row.get("title"),
        description: row.get("description"),
        body: row.get("body"),
        author_id: row.get("author_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    let article = build_article(&state.0, &ar, Some(auth.0)).await?;
    Ok(Json(ArticleResponse { article }))
}

pub async fn unfavorite_article(
    auth: AuthUser,
    path: Path<ArticleFavoritePath>,
    state: State<Db>,
) -> Result<Json<ArticleResponse>, JsonError> {
    let (slug,) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let row = client
        .query_opt(
            "SELECT id, slug, title, description, body, author_id, created_at, updated_at \
             FROM articles WHERE slug = $1",
            &[&slug],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("article not found"))?;

    let article_id: Uuid = row.get("id");
    client
        .execute(
            "DELETE FROM favorites WHERE user_id = $1 AND article_id = $2",
            &[&auth.0, &article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let ar = db::ArticleRow {
        id: article_id,
        slug: row.get("slug"),
        title: row.get("title"),
        description: row.get("description"),
        body: row.get("body"),
        author_id: row.get("author_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    let article = build_article(&state.0, &ar, Some(auth.0)).await?;
    Ok(Json(ArticleResponse { article }))
}

// ---------------------------------------------------------------------------
// Comment handlers
// ---------------------------------------------------------------------------

pub async fn get_comments(
    path: Path<ArticleCommentsPath>,
    opt_auth: OptionalAuth,
    state: State<Db>,
) -> Result<Json<CommentsResponse>, JsonError> {
    let (slug,) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let article = client
        .query_opt("SELECT id FROM articles WHERE slug = $1", &[&slug])
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("article not found"))?;
    let article_id: Uuid = article.get("id");

    let rows = client
        .query(
            "SELECT c.id, c.body, c.author_id, c.created_at, c.updated_at \
             FROM comments c WHERE c.article_id = $1 ORDER BY c.created_at DESC",
            &[&article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let mut comments = Vec::new();
    for row in &rows {
        let author_id: Uuid = row.get("author_id");
        let author = db::find_user_by_id(&state.0, author_id).await?;
        let profile = build_profile(&state.0, &author, opt_auth.0).await?;
        comments.push(CommentBody {
            id: row.get("id"),
            body: row.get("body"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            author: profile,
        });
    }

    Ok(Json(CommentsResponse { comments }))
}

pub async fn add_comment(
    auth: AuthUser,
    path: Path<ArticleCommentsPath>,
    state: State<Db>,
    body: Json<NewCommentRequest>,
) -> Result<Json<CommentResponse>, JsonError> {
    let (slug,) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let article = client
        .query_opt("SELECT id FROM articles WHERE slug = $1", &[&slug])
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?
        .ok_or_else(|| JsonError::not_found("article not found"))?;
    let article_id: Uuid = article.get("id");

    let row = client
        .query_one(
            "INSERT INTO comments (body, author_id, article_id) VALUES ($1, $2, $3) \
             RETURNING id, body, created_at, updated_at",
            &[&body.0.comment.body, &auth.0, &article_id],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;

    let author = db::find_user_by_id(&state.0, auth.0).await?;
    let profile = build_profile(&state.0, &author, Some(auth.0)).await?;

    Ok(Json(CommentResponse {
        comment: CommentBody {
            id: row.get("id"),
            body: row.get("body"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            author: profile,
        },
    }))
}

pub async fn delete_comment(
    auth: AuthUser,
    path: Path<ArticleCommentPath>,
    state: State<Db>,
) -> Result<http::StatusCode, JsonError> {
    let (_, comment_id) = path.0;
    let client = state
        .0
        .get()
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    let n = client
        .execute(
            "DELETE FROM comments WHERE id = $1 AND author_id = $2",
            &[&comment_id, &auth.0],
        )
        .await
        .map_err(|e| JsonError::internal(e.to_string()))?;
    if n == 0 {
        return Err(JsonError::not_found(
            "comment not found or not owned by you",
        ));
    }
    Ok(http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Tags handler
// ---------------------------------------------------------------------------

pub async fn get_tags(state: State<Db>) -> Result<Json<TagsResponse>, JsonError> {
    let tags = db::get_tags(&state.0).await?;
    Ok(Json(TagsResponse { tags }))
}
