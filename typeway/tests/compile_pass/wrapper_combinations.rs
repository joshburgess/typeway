// Wrapper types can be nested in various combinations.

use typeway::prelude::*;
use typeway_server::auth::Protected;
use typeway_server::typed::*;
use typeway_server::typed_response::Strict;

typeway_path!(type UsersPath = "users");
typeway_path!(type ItemsPath = "items");
typeway_path!(type TagsPath = "tags");
typeway_path!(type AdminPath = "admin");

#[derive(serde::Serialize, serde::Deserialize)]
struct User { name: String }
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct CreateUser { name: String }

#[derive(Clone)]
struct AuthUser(u32);
impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser(1))
    }
}

struct V1;
impl ApiVersion for V1 { const PREFIX: &'static str = "v1"; }

struct UserValidator;
impl Validate<CreateUser> for UserValidator {
    fn validate(body: &CreateUser) -> Result<(), String> {
        if body.name.is_empty() { return Err("empty".into()); }
        Ok(())
    }
}

struct StandardRate;
impl RateLimit for StandardRate {
    const MAX_REQUESTS: u32 = 100;
    const WINDOW_SECS: u64 = 60;
}

// API with various wrapper nesting combinations.
type API = (
    // Protected + Versioned
    Versioned<V1, Protected<AuthUser, GetEndpoint<UsersPath, Vec<User>>>>,
    // Validated + RateLimited
    RateLimited<StandardRate, Validated<UserValidator, PostEndpoint<ItemsPath, CreateUser, User>>>,
    // ContentType + Versioned
    Versioned<V1, ContentType<JsonContent, GetEndpoint<TagsPath, Vec<String>>>>,
    // Strict endpoint
    Strict<GetEndpoint<AdminPath, String>>,
);

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<API>();
}

fn main() {}
