// All type-level wrappers compile correctly when used properly.

use wayward::prelude::*;
use wayward_server::typed::*;
use wayward_server::typed_response::Strict;

wayward_path!(type UsersPath = "users");
wayward_path!(type TagsPath = "tags");

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
}

#[derive(serde::Deserialize)]
struct CreateUser {
    name: String,
}

// --- Validated ---

struct CreateUserValidator;
impl Validate<CreateUser> for CreateUserValidator {
    fn validate(body: &CreateUser) -> Result<(), String> {
        if body.name.is_empty() {
            return Err("name required".into());
        }
        Ok(())
    }
}

// --- Versioned ---

struct V1;
impl ApiVersion for V1 {
    const PREFIX: &'static str = "v1";
}

// --- RateLimited ---

struct StandardRate;
impl RateLimit for StandardRate {
    const MAX_REQUESTS: u32 = 100;
    const WINDOW_SECS: u64 = 60;
}

// --- API type with all wrappers ---

type API = (
    // Plain endpoint
    GetEndpoint<TagsPath, Vec<String>>,
    // Validated body
    Validated<CreateUserValidator, PostEndpoint<UsersPath, CreateUser, User>>,
    // Versioned
    Versioned<V1, GetEndpoint<UsersPath, Vec<User>>>,
    // Content-type enforced
    ContentType<JsonContent, PostEndpoint<UsersPath, CreateUser, User>>,
    // Rate limited
    RateLimited<StandardRate, GetEndpoint<UsersPath, Vec<User>>>,
);

// Verify ApiSpec
fn _check() {
    fn _assert_api<T: wayward_core::ApiSpec>() {}
    _assert_api::<API>();
}

fn main() {}
