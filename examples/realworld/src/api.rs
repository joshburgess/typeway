//! API type definition — the single source of truth for the RealWorld spec.
//!
//! This file demonstrates six typeway advanced features:
//!
//! 1. **Effects system**: `Requires<CorsRequired, E>` on public endpoints
//!    declares that the server must provide CORS middleware. Forgetting
//!    `.provide::<CorsRequired>()` in main.rs is a compile error.
//!
//! 2. **Content negotiation**: `get_tags` and `get_article` return
//!    `NegotiatedResponse<T, (JsonFormat, TextFormat, XmlFormat)>`, picking
//!    JSON, plain text, or XML based on the `Accept` header.
//!
//! 3. **API versioning**: V1 is the original 19-endpoint spec. V2 adds health,
//!    search, replaces tags, and deprecates old login (21 endpoints). V3 adds
//!    stats and account deletion, upgrades the user response, and removes
//!    the deprecated login (22 endpoints). `assert_api_compatible!` verifies
//!    backward compatibility at compile time.
//!
//! 4. **Session-typed WebSocket**: A live article feed uses `Send<ArticleUpdate, Rec<...>>`
//!    to encode the push protocol. Defined in handlers.rs.
//!
//! 5. **Validation**: `Validated<V, E>` validates request bodies (registration,
//!    article creation) before the handler runs, returning 422 on failure.

use crate::models::*;
use typeway_macros::typeway_path;

// ---------------------------------------------------------------------------
// Path types
// ---------------------------------------------------------------------------

typeway_path!(pub type UsersPath = "api" / "users");
typeway_path!(pub type UsersLoginPath = "api" / "users" / "login");
typeway_path!(pub type UserPath = "api" / "user");
typeway_path!(pub type ProfilePath = "api" / "profiles" / String);
typeway_path!(pub type ProfileFollowPath = "api" / "profiles" / String / "follow");
typeway_path!(pub type ArticlesPath = "api" / "articles");
typeway_path!(pub type ArticlesFeedPath = "api" / "articles" / "feed");
typeway_path!(pub type ArticlePath = "api" / "articles" / String);
typeway_path!(pub type ArticleFavoritePath = "api" / "articles" / String / "favorite");
typeway_path!(pub type ArticleCommentsPath = "api" / "articles" / String / "comments");
typeway_path!(pub type ArticleCommentPath = "api" / "articles" / String / "comments" / i32);
typeway_path!(pub type TagsPath = "api" / "tags");

// V2 additions
typeway_path!(pub type HealthPath = "api" / "health");
typeway_path!(pub type ArticlesSearchPath = "api" / "articles" / "search");

// V3 addition
typeway_path!(pub type StatsPath = "api" / "stats");

// ---------------------------------------------------------------------------
// Imports
// ---------------------------------------------------------------------------

use typeway_core::effects::{CorsRequired, Requires};
use typeway_core::{DeleteEndpoint, GetEndpoint, PostEndpoint, PutEndpoint};
use typeway_server::auth::Protected;
use typeway_server::typed::{Validate, Validated};

use crate::auth::AuthUser;

// ---------------------------------------------------------------------------
// Validators — compile-time request body validation (Feature 5)
// ---------------------------------------------------------------------------

/// Validates new user registration requests.
///
/// Wrapped in `Validated<NewUserValidator, PostEndpoint<...>>` in the API type,
/// so the framework rejects invalid requests with 422 before the handler runs.
/// This replaces hand-written validation in the handler with a type-level
/// declaration that is visible in the API type itself.
pub struct NewUserValidator;

impl Validate<NewUserRequest> for NewUserValidator {
    fn validate(body: &NewUserRequest) -> Result<(), String> {
        if body.user.username.is_empty() {
            return Err("username is required".into());
        }
        if body.user.email.is_empty() {
            return Err("email is required".into());
        }
        if !body.user.email.contains('@') {
            return Err("email must contain @".into());
        }
        if body.user.password.len() < 6 {
            return Err("password must be at least 6 characters".into());
        }
        Ok(())
    }
}

/// Validates new article creation requests.
pub struct NewArticleValidator;

impl Validate<NewArticleRequest> for NewArticleValidator {
    fn validate(body: &NewArticleRequest) -> Result<(), String> {
        if body.article.title.is_empty() {
            return Err("title is required".into());
        }
        if body.article.title.len() > 256 {
            return Err("title must be 256 characters or less".into());
        }
        if body.article.description.is_empty() {
            return Err("description is required".into());
        }
        if body.article.body.is_empty() {
            return Err("body is required".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// V1 API — the original 19-endpoint RealWorld spec
// ---------------------------------------------------------------------------
//
// Three type-level features at work:
//
// 1. Effects (Feature 1):
//    Public-facing read endpoints are wrapped in `Requires<CorsRequired, _>`.
//    The EffectfulServer in main.rs tracks that CorsRequired has been provided.
//    Comment out `.provide::<CorsRequired>()` and the server won't compile.
//
// 2. Validation (Feature 5):
//    Registration and article creation use `Validated<V, E>` wrappers.
//    Invalid JSON bodies get a 422 response before the handler is called.
//    These use `bind_validated!()` in the handler tuple.
//
// 3. Authentication:
//    `Protected<AuthUser, E>` enforces that handlers accept `AuthUser`
//    as their first argument, verified at compile time via `bind_auth!()`.

pub type RealWorldV1 = (
    // Auth (public)
    Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
    PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,
    // Auth (protected — compiler enforces AuthUser as first arg)
    Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,
    Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
    // Profiles (public read requires CORS, protected write)
    Requires<CorsRequired, GetEndpoint<ProfilePath, ProfileResponse>>,
    Protected<AuthUser, PostEndpoint<ProfileFollowPath, (), ProfileResponse>>,
    Protected<AuthUser, DeleteEndpoint<ProfileFollowPath, ProfileResponse>>,
    // Articles (public read endpoints require CORS for browser access)
    Requires<CorsRequired, GetEndpoint<ArticlesPath, ArticlesResponse>>,
    Protected<AuthUser, GetEndpoint<ArticlesFeedPath, ArticlesResponse>>,
    Requires<CorsRequired, GetEndpoint<ArticlePath, ArticleResponse>>,
    // Article creation is protected AND validated
    Protected<AuthUser, Validated<NewArticleValidator, PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>>>,
    Protected<AuthUser, PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticlePath, ()>>,
    // Favorites (protected)
    Protected<AuthUser, PostEndpoint<ArticleFavoritePath, (), ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleFavoritePath, ArticleResponse>>,
    // Comments (public read requires CORS, protected write)
    Requires<CorsRequired, GetEndpoint<ArticleCommentsPath, CommentsResponse>>,
    Protected<AuthUser, PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleCommentPath, ()>>,
    // Tags (public, handler uses content negotiation — see handlers.rs)
    Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponse>>,
);

// ---------------------------------------------------------------------------
// V2 API — significant evolution with typed deltas (Feature 3)
// ---------------------------------------------------------------------------
//
// V2 is a substantial evolution from V1 (21 endpoints total):
//   - Added:      GET /api/health (health check endpoint)
//   - Added:      GET /api/articles/search (article search, new functionality)
//   - Replaced:   Tags endpoint changes response from TagsResponse to TagsResponseV2
//                 (V2 tags include per-tag usage counts)
//   - Deprecated: POST /api/users/login (marked deprecated but still present)

use typeway_core::versioning::{Added, Deprecated, Removed, Replaced, VersionedApi};

/// Changes from V1 to V2: two additions, one replacement, one deprecation.
type V2Changes = (
    Added<GetEndpoint<HealthPath, HealthResponse>>,
    Added<GetEndpoint<ArticlesSearchPath, ArticlesResponse>>,
    Replaced<
        Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponse>>,
        Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponseV2>>,
    >,
    Deprecated<PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>>,
);

/// The resolved V2 API: 21 endpoints after applying V2Changes.
pub type RealWorldV2Resolved = (
    // Auth
    Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
    PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,  // deprecated but still present
    Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,
    Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
    // Profiles
    Requires<CorsRequired, GetEndpoint<ProfilePath, ProfileResponse>>,
    Protected<AuthUser, PostEndpoint<ProfileFollowPath, (), ProfileResponse>>,
    Protected<AuthUser, DeleteEndpoint<ProfileFollowPath, ProfileResponse>>,
    // Articles
    Requires<CorsRequired, GetEndpoint<ArticlesPath, ArticlesResponse>>,
    Protected<AuthUser, GetEndpoint<ArticlesFeedPath, ArticlesResponse>>,
    Requires<CorsRequired, GetEndpoint<ArticlePath, ArticleResponse>>,
    Protected<AuthUser, Validated<NewArticleValidator, PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>>>,
    Protected<AuthUser, PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticlePath, ()>>,
    // Favorites
    Protected<AuthUser, PostEndpoint<ArticleFavoritePath, (), ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleFavoritePath, ArticleResponse>>,
    // Comments
    Requires<CorsRequired, GetEndpoint<ArticleCommentsPath, CommentsResponse>>,
    Protected<AuthUser, PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleCommentPath, ()>>,
    // Tags — REPLACED: now returns TagsResponseV2 with counts
    Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponseV2>>,
    // V2 additions
    GetEndpoint<HealthPath, HealthResponse>,
    GetEndpoint<ArticlesSearchPath, ArticlesResponse>,
);

/// The V2 API type. Carries version lineage as type parameters.
pub type RealWorldV2 = VersionedApi<RealWorldV1, V2Changes, RealWorldV2Resolved>;

// Compile-time backward compatibility check: V1 → V2
// Every V1 endpoint must exist in V2Resolved. The tags endpoint was replaced,
// so we list only the non-replaced V1 endpoints (all 18 that were preserved
// unchanged). The old TagsResponse endpoint is intentionally omitted because
// it was replaced with TagsResponseV2.
typeway_core::assert_api_compatible!(
    (
        Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
        PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,
        Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,
        Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
        Requires<CorsRequired, GetEndpoint<ProfilePath, ProfileResponse>>,
        Protected<AuthUser, PostEndpoint<ProfileFollowPath, (), ProfileResponse>>,
        Protected<AuthUser, DeleteEndpoint<ProfileFollowPath, ProfileResponse>>,
        Requires<CorsRequired, GetEndpoint<ArticlesPath, ArticlesResponse>>,
        Protected<AuthUser, GetEndpoint<ArticlesFeedPath, ArticlesResponse>>,
        Requires<CorsRequired, GetEndpoint<ArticlePath, ArticleResponse>>,
        Protected<AuthUser, Validated<NewArticleValidator, PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>>>,
        Protected<AuthUser, PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>>,
        Protected<AuthUser, DeleteEndpoint<ArticlePath, ()>>,
        Protected<AuthUser, PostEndpoint<ArticleFavoritePath, (), ArticleResponse>>,
        Protected<AuthUser, DeleteEndpoint<ArticleFavoritePath, ArticleResponse>>,
        Requires<CorsRequired, GetEndpoint<ArticleCommentsPath, CommentsResponse>>,
        Protected<AuthUser, PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>>,
        Protected<AuthUser, DeleteEndpoint<ArticleCommentPath, ()>>,
    ),
    RealWorldV2Resolved
);

// ---------------------------------------------------------------------------
// V3 API — breaking change with typed deltas (Feature 3)
// ---------------------------------------------------------------------------
//
// V3 is a breaking evolution from V2 (22 endpoints total):
//   - Added:    GET /api/stats (site-wide statistics)
//   - Added:    DELETE /api/user (account deletion, protected)
//   - Replaced: GET /api/user response changes from UserResponse to UserResponseV3
//               (V3 adds created_at and articles_count fields)
//   - Removed:  POST /api/users/login (the deprecated endpoint is now gone)

type V3Changes = (
    Added<GetEndpoint<StatsPath, StatsResponse>>,
    Added<Protected<AuthUser, DeleteEndpoint<UserPath, ()>>>,
    Replaced<
        Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,
        Protected<AuthUser, GetEndpoint<UserPath, UserResponseV3>>,
    >,
    Removed<PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>>,
);

/// The resolved V3 API: 22 endpoints after applying V3Changes.
/// Login is removed, user response upgraded, stats and delete_account added.
pub type RealWorldV3Resolved = (
    // Auth
    Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
    // (login removed in V3)
    Protected<AuthUser, GetEndpoint<UserPath, UserResponseV3>>,  // REPLACED: now V3 response
    Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
    // Profiles
    Requires<CorsRequired, GetEndpoint<ProfilePath, ProfileResponse>>,
    Protected<AuthUser, PostEndpoint<ProfileFollowPath, (), ProfileResponse>>,
    Protected<AuthUser, DeleteEndpoint<ProfileFollowPath, ProfileResponse>>,
    // Articles
    Requires<CorsRequired, GetEndpoint<ArticlesPath, ArticlesResponse>>,
    Protected<AuthUser, GetEndpoint<ArticlesFeedPath, ArticlesResponse>>,
    Requires<CorsRequired, GetEndpoint<ArticlePath, ArticleResponse>>,
    Protected<AuthUser, Validated<NewArticleValidator, PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>>>,
    Protected<AuthUser, PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticlePath, ()>>,
    // Favorites
    Protected<AuthUser, PostEndpoint<ArticleFavoritePath, (), ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleFavoritePath, ArticleResponse>>,
    // Comments
    Requires<CorsRequired, GetEndpoint<ArticleCommentsPath, CommentsResponse>>,
    Protected<AuthUser, PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleCommentPath, ()>>,
    // Tags (V2 version with counts)
    Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponseV2>>,
    // V2 additions (carried forward)
    GetEndpoint<HealthPath, HealthResponse>,
    GetEndpoint<ArticlesSearchPath, ArticlesResponse>,
    // V3 additions
    GetEndpoint<StatsPath, StatsResponse>,
    Protected<AuthUser, DeleteEndpoint<UserPath, ()>>,
);

/// The V3 API type: evolves from V2, carries full lineage.
pub type RealWorldV3 = VersionedApi<RealWorldV2, V3Changes, RealWorldV3Resolved>;

// V2 → V3: NOT backward compatible (login endpoint removed).
// Uncommenting the line below would cause a compile error:
// typeway_core::assert_api_compatible!(
//     (PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,),
//     RealWorldV3Resolved
// );

// However, V3 preserves all non-deprecated V2 endpoints (minus login and the
// old UserResponse). We verify the 17 shared endpoints are present:
typeway_core::assert_api_compatible!(
    (
        Validated<NewUserValidator, PostEndpoint<UsersPath, NewUserRequest, UserResponse>>,
        Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
        Requires<CorsRequired, GetEndpoint<ProfilePath, ProfileResponse>>,
        Protected<AuthUser, PostEndpoint<ProfileFollowPath, (), ProfileResponse>>,
        Protected<AuthUser, DeleteEndpoint<ProfileFollowPath, ProfileResponse>>,
        Requires<CorsRequired, GetEndpoint<ArticlesPath, ArticlesResponse>>,
        Protected<AuthUser, GetEndpoint<ArticlesFeedPath, ArticlesResponse>>,
        Requires<CorsRequired, GetEndpoint<ArticlePath, ArticleResponse>>,
        Protected<AuthUser, Validated<NewArticleValidator, PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>>>,
        Protected<AuthUser, PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>>,
        Protected<AuthUser, DeleteEndpoint<ArticlePath, ()>>,
        Protected<AuthUser, PostEndpoint<ArticleFavoritePath, (), ArticleResponse>>,
        Protected<AuthUser, DeleteEndpoint<ArticleFavoritePath, ArticleResponse>>,
        Requires<CorsRequired, GetEndpoint<ArticleCommentsPath, CommentsResponse>>,
        Protected<AuthUser, PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>>,
        Protected<AuthUser, DeleteEndpoint<ArticleCommentPath, ()>>,
        Requires<CorsRequired, GetEndpoint<TagsPath, TagsResponseV2>>,
        GetEndpoint<HealthPath, HealthResponse>,
        GetEndpoint<ArticlesSearchPath, ArticlesResponse>,
    ),
    RealWorldV3Resolved
);

/// The API type used by the server. V3 is the latest version.
pub type RealWorldAPI = RealWorldV3;

// ---------------------------------------------------------------------------
// Session-typed WebSocket protocol for live article feed (Feature 4)
// ---------------------------------------------------------------------------

use typeway_core::session::{Rec, Send, Var};

/// Protocol for the live article feed WebSocket:
///
/// 1. Server sends a welcome `ArticleUpdate` (event: "connected")
/// 2. Enter a loop: server sends `ArticleUpdate` messages (event: "new_article")
///
/// The session type enforces this ordering: the handler cannot receive before
/// sending, and the recursive structure ensures the loop is well-formed.
/// See `handlers::ws_feed` for the implementation.
#[allow(dead_code)]
pub type FeedProtocol = Send<ArticleUpdate, Rec<Send<ArticleUpdate, Var>>>;
