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

pub type RealWorldAPI = (
    // Auth
    PostEndpoint<UsersPath, NewUserRequest, UserResponse>,
    PostEndpoint<UsersLoginPath, LoginRequest, UserResponse>,
    GetEndpoint<UserPath, UserResponse>,
    PutEndpoint<UserPath, UpdateUserRequest, UserResponse>,
    // Profiles
    GetEndpoint<ProfilePath, ProfileResponse>,
    PostEndpoint<ProfileFollowPath, (), ProfileResponse>,
    DeleteEndpoint<ProfileFollowPath, ProfileResponse>,
    // Articles
    GetEndpoint<ArticlesPath, ArticlesResponse>,
    GetEndpoint<ArticlesFeedPath, ArticlesResponse>,
    GetEndpoint<ArticlePath, ArticleResponse>,
    PostEndpoint<ArticlesPath, NewArticleRequest, ArticleResponse>,
    PutEndpoint<ArticlePath, UpdateArticleRequest, ArticleResponse>,
    DeleteEndpoint<ArticlePath, ()>,
    // Favorites
    PostEndpoint<ArticleFavoritePath, (), ArticleResponse>,
    DeleteEndpoint<ArticleFavoritePath, ArticleResponse>,
    // Comments
    GetEndpoint<ArticleCommentsPath, CommentsResponse>,
    PostEndpoint<ArticleCommentsPath, NewCommentRequest, CommentResponse>,
    DeleteEndpoint<ArticleCommentPath, ()>,
    // Tags
    GetEndpoint<TagsPath, TagsResponse>,
);
