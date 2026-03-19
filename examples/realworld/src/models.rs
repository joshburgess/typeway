//! Domain types matching the RealWorld API spec.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub user: UserBody,
}

#[derive(Debug, Serialize)]
pub struct UserBody {
    pub email: String,
    pub token: String,
    pub username: String,
    pub bio: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewUserRequest {
    pub user: NewUser,
}

#[derive(Debug, Deserialize)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub user: LoginUser,
}

#[derive(Debug, Deserialize)]
pub struct LoginUser {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub user: UpdateUser,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bio: Option<String>,
    pub image: Option<String>,
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub profile: ProfileBody,
}

#[derive(Debug, Serialize)]
pub struct ProfileBody {
    pub username: String,
    pub bio: Option<String>,
    pub image: Option<String>,
    pub following: bool,
}

// ---------------------------------------------------------------------------
// Article
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ArticleResponse {
    pub article: ArticleBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticlesResponse {
    pub articles: Vec<ArticleBody>,
    pub articles_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleBody {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub tag_list: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub favorited: bool,
    pub favorites_count: i64,
    pub author: ProfileBody,
}

#[derive(Debug, Deserialize)]
pub struct NewArticleRequest {
    pub article: NewArticle,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewArticle {
    pub title: String,
    pub description: String,
    pub body: String,
    pub tag_list: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateArticleRequest {
    pub article: UpdateArticle,
}

#[derive(Debug, Deserialize)]
pub struct UpdateArticle {
    pub title: Option<String>,
    pub description: Option<String>,
    pub body: Option<String>,
}

// ---------------------------------------------------------------------------
// Comment
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CommentResponse {
    pub comment: CommentBody,
}

#[derive(Debug, Serialize)]
pub struct CommentsResponse {
    pub comments: Vec<CommentBody>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentBody {
    pub id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub body: String,
    pub author: ProfileBody,
}

#[derive(Debug, Deserialize)]
pub struct NewCommentRequest {
    pub comment: NewComment,
}

#[derive(Debug, Deserialize)]
pub struct NewComment {
    pub body: String,
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TagsResponse {
    pub tags: Vec<String>,
}

// Display impl enables content negotiation: when a client sends
// Accept: text/plain, TagsResponse renders as a comma-separated list
// instead of JSON.
impl std::fmt::Display for TagsResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.tags.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Tags V2 (for API versioning demo)
// ---------------------------------------------------------------------------

/// V2 tags response includes usage counts per tag — a common API evolution.
/// Included here to show how versioned response types look alongside V1 types.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct TagsResponseV2 {
    pub tags: Vec<TagWithCount>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct TagWithCount {
    pub tag: String,
    pub count: i64,
}

#[allow(dead_code)]
impl std::fmt::Display for TagsResponseV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self
            .tags
            .iter()
            .map(|t| format!("{} ({})", t.tag, t.count))
            .collect();
        write!(f, "{}", parts.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Health check (V2 addition)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// WebSocket live feed
// ---------------------------------------------------------------------------

/// A real-time article update pushed via WebSocket.
/// Used by the session-typed WebSocket handler (`handlers::ws_feed`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ArticleUpdate {
    pub event: String,
    pub slug: String,
    pub title: String,
}

// ---------------------------------------------------------------------------
// DB row types
// ---------------------------------------------------------------------------

pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub bio: Option<String>,
    pub image: Option<String>,
}

pub struct ArticleRow {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub author_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
