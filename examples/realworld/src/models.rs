//! Domain types matching the RealWorld API spec.
//!
//! All model types use `#[derive(ToProtoType)]` with `#[proto(tag = N)]` for
//! stable protobuf field numbering. This replaces ~230 lines of manual
//! `impl ToProtoType` blocks with derive attributes on the struct definitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use typeway_macros::ToProtoType;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToProtoType)]
pub struct UserResponse {
    #[proto(tag = 1)]
    pub user: UserBody,
}

#[derive(Debug, Serialize, ToProtoType)]
pub struct UserBody {
    #[proto(tag = 1)]
    pub email: String,
    #[proto(tag = 2)]
    pub token: String,
    #[proto(tag = 3)]
    pub username: String,
    #[proto(tag = 4)]
    pub bio: Option<String>,
    #[proto(tag = 5)]
    pub image: Option<String>,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct NewUserRequest {
    #[proto(tag = 1)]
    pub user: NewUser,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct NewUser {
    #[proto(tag = 1)]
    pub username: String,
    #[proto(tag = 2)]
    pub email: String,
    #[proto(tag = 3)]
    pub password: String,
}

#[derive(Debug, Deserialize, ToProtoType)]
#[allow(dead_code)]
pub struct LoginRequest {
    #[proto(tag = 1)]
    pub user: LoginUser,
}

#[derive(Debug, Deserialize, ToProtoType)]
#[allow(dead_code)]
pub struct LoginUser {
    #[proto(tag = 1)]
    pub email: String,
    #[proto(tag = 2)]
    pub password: String,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct UpdateUserRequest {
    #[proto(tag = 1)]
    pub user: UpdateUser,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct UpdateUser {
    #[proto(tag = 1)]
    pub email: Option<String>,
    #[proto(tag = 2)]
    pub username: Option<String>,
    #[proto(tag = 3)]
    pub password: Option<String>,
    #[proto(tag = 4)]
    pub bio: Option<String>,
    #[proto(tag = 5)]
    pub image: Option<String>,
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToProtoType)]
pub struct ProfileResponse {
    #[proto(tag = 1)]
    pub profile: ProfileBody,
}

#[derive(Debug, Serialize, ToProtoType)]
pub struct ProfileBody {
    #[proto(tag = 1)]
    pub username: String,
    #[proto(tag = 2)]
    pub bio: Option<String>,
    #[proto(tag = 3)]
    pub image: Option<String>,
    #[proto(tag = 4)]
    pub following: bool,
}

// ---------------------------------------------------------------------------
// Article
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToProtoType)]
pub struct ArticleResponse {
    #[proto(tag = 1)]
    pub article: ArticleBody,
}

impl std::fmt::Display for ArticleResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} by {} — {}",
            self.article.title, self.article.author.username, self.article.description
        )
    }
}

impl typeway_server::negotiate::RenderAsXml for ArticleResponse {
    fn to_xml(&self) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<article>\n  <slug>{}</slug>\n  <title>{}</title>\n  <description>{}</description>\n  <body>{}</body>\n  <author>{}</author>\n</article>",
            self.article.slug, self.article.title, self.article.description, self.article.body, self.article.author.username
        )
    }
}

#[derive(Debug, Serialize, ToProtoType)]
#[serde(rename_all = "camelCase")]
pub struct ArticlesResponse {
    #[proto(tag = 1)]
    pub articles: Vec<ArticleBody>,
    #[proto(tag = 2)]
    pub articles_count: usize,
}

#[derive(Debug, Serialize, ToProtoType)]
#[serde(rename_all = "camelCase")]
pub struct ArticleBody {
    #[proto(tag = 1)]
    pub slug: String,
    #[proto(tag = 2)]
    pub title: String,
    #[proto(tag = 3)]
    pub description: String,
    #[proto(tag = 4)]
    pub body: String,
    #[proto(tag = 5)]
    pub tag_list: Vec<String>,
    #[proto(tag = 6)]
    pub created_at: DateTime<Utc>,
    #[proto(tag = 7)]
    pub updated_at: DateTime<Utc>,
    #[proto(tag = 8)]
    pub favorited: bool,
    #[proto(tag = 9)]
    pub favorites_count: i64,
    #[proto(tag = 10)]
    pub author: ProfileBody,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct NewArticleRequest {
    #[proto(tag = 1)]
    pub article: NewArticle,
}

#[derive(Debug, Deserialize, ToProtoType)]
#[serde(rename_all = "camelCase")]
pub struct NewArticle {
    #[proto(tag = 1)]
    pub title: String,
    #[proto(tag = 2)]
    pub description: String,
    #[proto(tag = 3)]
    pub body: String,
    #[proto(tag = 4)]
    pub tag_list: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct UpdateArticleRequest {
    #[proto(tag = 1)]
    pub article: UpdateArticle,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct UpdateArticle {
    #[proto(tag = 1)]
    pub title: Option<String>,
    #[proto(tag = 2)]
    pub description: Option<String>,
    #[proto(tag = 3)]
    pub body: Option<String>,
}

// ---------------------------------------------------------------------------
// Comment
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToProtoType)]
pub struct CommentResponse {
    #[proto(tag = 1)]
    pub comment: CommentBody,
}

#[derive(Debug, Serialize, ToProtoType)]
pub struct CommentsResponse {
    #[proto(tag = 1)]
    pub comments: Vec<CommentBody>,
}

#[derive(Debug, Serialize, ToProtoType)]
#[serde(rename_all = "camelCase")]
pub struct CommentBody {
    #[proto(tag = 1)]
    pub id: i32,
    #[proto(tag = 2)]
    pub created_at: DateTime<Utc>,
    #[proto(tag = 3)]
    pub updated_at: DateTime<Utc>,
    #[proto(tag = 4)]
    pub body: String,
    #[proto(tag = 5)]
    pub author: ProfileBody,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct NewCommentRequest {
    #[proto(tag = 1)]
    pub comment: NewComment,
}

#[derive(Debug, Deserialize, ToProtoType)]
pub struct NewComment {
    #[proto(tag = 1)]
    pub body: String,
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToProtoType)]
pub struct TagsResponse {
    #[proto(tag = 1)]
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

// XML rendering for TagsResponse: produces a simple <tags> document.
impl typeway_server::negotiate::RenderAsXml for TagsResponse {
    fn to_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n<tags>\n");
        for tag in &self.tags {
            xml.push_str(&format!("  <tag>{tag}</tag>\n"));
        }
        xml.push_str("</tags>");
        xml
    }
}

// ---------------------------------------------------------------------------
// Tags V2 (for API versioning demo)
// ---------------------------------------------------------------------------

/// V2 tags response includes usage counts per tag — a common API evolution.
#[derive(Debug, Serialize, ToProtoType)]
pub struct TagsResponseV2 {
    #[proto(tag = 1)]
    pub tags: Vec<TagWithCount>,
}

#[derive(Debug, Serialize, ToProtoType)]
pub struct TagWithCount {
    #[proto(tag = 1)]
    pub tag: String,
    #[proto(tag = 2)]
    pub count: i64,
}

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

#[derive(Debug, Serialize, ToProtoType)]
pub struct HealthResponse {
    #[proto(tag = 1)]
    pub status: String,
    #[proto(tag = 2)]
    pub version: String,
}

// ---------------------------------------------------------------------------
// Tags V2 XML rendering
// ---------------------------------------------------------------------------

impl typeway_server::negotiate::RenderAsXml for TagsResponseV2 {
    fn to_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n<tags>\n");
        for t in &self.tags {
            xml.push_str(&format!("  <tag count=\"{}\">{}</tag>\n", t.count, t.tag));
        }
        xml.push_str("</tags>");
        xml
    }
}

// ---------------------------------------------------------------------------
// Stats (V3 addition)
// ---------------------------------------------------------------------------

/// Site-wide statistics — total counts of users, articles, and comments.
#[derive(Debug, Serialize, ToProtoType)]
pub struct StatsResponse {
    #[proto(tag = 1)]
    pub users: i64,
    #[proto(tag = 2)]
    pub articles: i64,
    #[proto(tag = 3)]
    pub comments: i64,
}

// ---------------------------------------------------------------------------
// User V3 — extended user response with created_at and article count
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToProtoType)]
pub struct UserResponseV3 {
    #[proto(tag = 1)]
    pub user: UserBodyV3,
}

#[derive(Debug, Serialize, ToProtoType)]
#[serde(rename_all = "camelCase")]
pub struct UserBodyV3 {
    #[proto(tag = 1)]
    pub email: String,
    #[proto(tag = 2)]
    pub token: String,
    #[proto(tag = 3)]
    pub username: String,
    #[proto(tag = 4)]
    pub bio: Option<String>,
    #[proto(tag = 5)]
    pub image: Option<String>,
    #[proto(tag = 6)]
    pub created_at: DateTime<Utc>,
    #[proto(tag = 7)]
    pub articles_count: i64,
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

#[allow(dead_code)]
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
