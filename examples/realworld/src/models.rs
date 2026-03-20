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
#[allow(dead_code)]
pub struct LoginRequest {
    pub user: LoginUser,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[derive(Debug, Serialize)]
pub struct TagsResponseV2 {
    pub tags: Vec<TagWithCount>,
}

#[derive(Debug, Serialize)]
pub struct TagWithCount {
    pub tag: String,
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

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
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
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub users: i64,
    pub articles: i64,
    pub comments: i64,
}

// ---------------------------------------------------------------------------
// User V3 — extended user response with created_at and article count
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UserResponseV3 {
    pub user: UserBodyV3,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserBodyV3 {
    pub email: String,
    pub token: String,
    pub username: String,
    pub bio: Option<String>,
    pub image: Option<String>,
    pub created_at: DateTime<Utc>,
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

// ---------------------------------------------------------------------------
// gRPC: ToProtoType impls for proto generation
// ---------------------------------------------------------------------------

use typeway_grpc::ToProtoType;

impl ToProtoType for UserResponse {
    fn proto_type_name() -> &'static str { "UserResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UserResponse {\n  UserBody user = 1;\n}".to_string())
    }
}

impl ToProtoType for UserBody {
    fn proto_type_name() -> &'static str { "UserBody" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UserBody {\n  string email = 1;\n  string token = 2;\n  string username = 3;\n  optional string bio = 4;\n  optional string image = 5;\n}".to_string())
    }
}

impl ToProtoType for UserResponseV3 {
    fn proto_type_name() -> &'static str { "UserResponseV3" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UserResponseV3 {\n  UserBodyV3 user = 1;\n}".to_string())
    }
}

impl ToProtoType for UserBodyV3 {
    fn proto_type_name() -> &'static str { "UserBodyV3" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UserBodyV3 {\n  string email = 1;\n  string token = 2;\n  string username = 3;\n  optional string bio = 4;\n  optional string image = 5;\n  string created_at = 6;\n  int64 articles_count = 7;\n}".to_string())
    }
}

impl ToProtoType for NewUserRequest {
    fn proto_type_name() -> &'static str { "NewUserRequest" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message NewUserRequest {\n  NewUser user = 1;\n}".to_string())
    }
}

impl ToProtoType for NewUser {
    fn proto_type_name() -> &'static str { "NewUser" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message NewUser {\n  string username = 1;\n  string email = 2;\n  string password = 3;\n}".to_string())
    }
}

impl ToProtoType for LoginRequest {
    fn proto_type_name() -> &'static str { "LoginRequest" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message LoginRequest {\n  LoginUser user = 1;\n}".to_string())
    }
}

impl ToProtoType for LoginUser {
    fn proto_type_name() -> &'static str { "LoginUser" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message LoginUser {\n  string email = 1;\n  string password = 2;\n}".to_string())
    }
}

impl ToProtoType for UpdateUserRequest {
    fn proto_type_name() -> &'static str { "UpdateUserRequest" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UpdateUserRequest {\n  UpdateUser user = 1;\n}".to_string())
    }
}

impl ToProtoType for UpdateUser {
    fn proto_type_name() -> &'static str { "UpdateUser" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UpdateUser {\n  optional string email = 1;\n  optional string username = 2;\n  optional string password = 3;\n  optional string bio = 4;\n  optional string image = 5;\n}".to_string())
    }
}

impl ToProtoType for ProfileResponse {
    fn proto_type_name() -> &'static str { "ProfileResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message ProfileResponse {\n  ProfileBody profile = 1;\n}".to_string())
    }
}

impl ToProtoType for ProfileBody {
    fn proto_type_name() -> &'static str { "ProfileBody" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message ProfileBody {\n  string username = 1;\n  optional string bio = 2;\n  optional string image = 3;\n  bool following = 4;\n}".to_string())
    }
}

impl ToProtoType for ArticleResponse {
    fn proto_type_name() -> &'static str { "ArticleResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message ArticleResponse {\n  ArticleBody article = 1;\n}".to_string())
    }
}

impl ToProtoType for ArticlesResponse {
    fn proto_type_name() -> &'static str { "ArticlesResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message ArticlesResponse {\n  repeated ArticleBody articles = 1;\n  int64 articles_count = 2;\n}".to_string())
    }
}

impl ToProtoType for ArticleBody {
    fn proto_type_name() -> &'static str { "ArticleBody" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message ArticleBody {\n  string slug = 1;\n  string title = 2;\n  string description = 3;\n  string body = 4;\n  repeated string tag_list = 5;\n  string created_at = 6;\n  string updated_at = 7;\n  bool favorited = 8;\n  int64 favorites_count = 9;\n  ProfileBody author = 10;\n}".to_string())
    }
}

impl ToProtoType for NewArticleRequest {
    fn proto_type_name() -> &'static str { "NewArticleRequest" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message NewArticleRequest {\n  NewArticle article = 1;\n}".to_string())
    }
}

impl ToProtoType for NewArticle {
    fn proto_type_name() -> &'static str { "NewArticle" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message NewArticle {\n  string title = 1;\n  string description = 2;\n  string body = 3;\n  repeated string tag_list = 4;\n}".to_string())
    }
}

impl ToProtoType for UpdateArticleRequest {
    fn proto_type_name() -> &'static str { "UpdateArticleRequest" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UpdateArticleRequest {\n  UpdateArticle article = 1;\n}".to_string())
    }
}

impl ToProtoType for UpdateArticle {
    fn proto_type_name() -> &'static str { "UpdateArticle" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message UpdateArticle {\n  optional string title = 1;\n  optional string description = 2;\n  optional string body = 3;\n}".to_string())
    }
}

impl ToProtoType for CommentResponse {
    fn proto_type_name() -> &'static str { "CommentResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message CommentResponse {\n  CommentBody comment = 1;\n}".to_string())
    }
}

impl ToProtoType for CommentsResponse {
    fn proto_type_name() -> &'static str { "CommentsResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message CommentsResponse {\n  repeated CommentBody comments = 1;\n}".to_string())
    }
}

impl ToProtoType for CommentBody {
    fn proto_type_name() -> &'static str { "CommentBody" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message CommentBody {\n  int32 id = 1;\n  string created_at = 2;\n  string updated_at = 3;\n  string body = 4;\n  ProfileBody author = 5;\n}".to_string())
    }
}

impl ToProtoType for NewCommentRequest {
    fn proto_type_name() -> &'static str { "NewCommentRequest" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message NewCommentRequest {\n  NewComment comment = 1;\n}".to_string())
    }
}

impl ToProtoType for NewComment {
    fn proto_type_name() -> &'static str { "NewComment" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message NewComment {\n  string body = 1;\n}".to_string())
    }
}

impl ToProtoType for TagsResponse {
    fn proto_type_name() -> &'static str { "TagsResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message TagsResponse {\n  repeated string tags = 1;\n}".to_string())
    }
}

impl ToProtoType for TagsResponseV2 {
    fn proto_type_name() -> &'static str { "TagsResponseV2" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message TagsResponseV2 {\n  repeated TagWithCount tags = 1;\n}".to_string())
    }
}

impl ToProtoType for TagWithCount {
    fn proto_type_name() -> &'static str { "TagWithCount" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message TagWithCount {\n  string tag = 1;\n  int64 count = 2;\n}".to_string())
    }
}

impl ToProtoType for HealthResponse {
    fn proto_type_name() -> &'static str { "HealthResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message HealthResponse {\n  string status = 1;\n  string version = 2;\n}".to_string())
    }
}

impl ToProtoType for StatsResponse {
    fn proto_type_name() -> &'static str { "StatsResponse" }
    fn is_message() -> bool { true }
    fn message_definition() -> Option<String> {
        Some("message StatsResponse {\n  int64 users = 1;\n  int64 articles = 2;\n  int64 comments = 3;\n}".to_string())
    }
}
