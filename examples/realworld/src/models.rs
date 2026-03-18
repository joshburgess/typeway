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
