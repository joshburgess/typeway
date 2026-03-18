//! API type definition — the single source of truth for the RealWorld spec.

use crate::models::*;
use wayward_macros::wayward_path;

// ---------------------------------------------------------------------------
// Path types
// ---------------------------------------------------------------------------

wayward_path!(pub type UsersPath = "api" / "users");
wayward_path!(pub type UsersLoginPath = "api" / "users" / "login");
wayward_path!(pub type UserPath = "api" / "user");
wayward_path!(pub type ProfilePath = "api" / "profiles" / String);
wayward_path!(pub type ProfileFollowPath = "api" / "profiles" / String / "follow");
wayward_path!(pub type ArticlesPath = "api" / "articles");
wayward_path!(pub type ArticlesFeedPath = "api" / "articles" / "feed");
wayward_path!(pub type ArticlePath = "api" / "articles" / String);
wayward_path!(pub type ArticleFavoritePath = "api" / "articles" / String / "favorite");
wayward_path!(pub type ArticleCommentsPath = "api" / "articles" / String / "comments");
wayward_path!(pub type ArticleCommentPath = "api" / "articles" / String / "comments" / i32);
wayward_path!(pub type TagsPath = "api" / "tags");

// ---------------------------------------------------------------------------
// API type
// ---------------------------------------------------------------------------

use wayward_core::{DeleteEndpoint, GetEndpoint, PostEndpoint, PutEndpoint};
use wayward_server::auth::Protected;

use crate::auth::AuthUser;

// Protected<Auth, E> declares at the type level that an endpoint requires
// authentication. The compiler enforces that the handler's first argument
// is the Auth type — omitting it is a compile error.

pub type RealWorldAPI = (
    // Auth (public)
    PostEndpoint<UsersPath, NewUserRequest, UserResponse>,
    PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,
    // Auth (protected)
    Protected<AuthUser, GetEndpoint<UserPath, UserResponse>>,
    Protected<AuthUser, PutEndpoint<UserPath, UpdateUserRequest, UserResponse>>,
    // Profiles (public read, protected write)
    GetEndpoint<ProfilePath, ProfileResponse>,
    Protected<AuthUser, PostEndpoint<ProfileFollowPath, (), ProfileResponse>>,
    Protected<AuthUser, DeleteEndpoint<ProfileFollowPath, ProfileResponse>>,
    // Articles (public read, protected write)
    GetEndpoint<ArticlesPath, ArticlesResponse>,
    Protected<AuthUser, GetEndpoint<ArticlesFeedPath, ArticlesResponse>>,
    GetEndpoint<ArticlePath, ArticleResponse>,
    Protected<AuthUser, PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>>,
    Protected<AuthUser, PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticlePath, ()>>,
    // Favorites (protected)
    Protected<AuthUser, PostEndpoint<ArticleFavoritePath, (), ArticleResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleFavoritePath, ArticleResponse>>,
    // Comments (public read, protected write)
    GetEndpoint<ArticleCommentsPath, CommentsResponse>,
    Protected<AuthUser, PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>>,
    Protected<AuthUser, DeleteEndpoint<ArticleCommentPath, ()>>,
    // Tags (public)
    GetEndpoint<TagsPath, TagsResponse>,
);
