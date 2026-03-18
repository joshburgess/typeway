// The endpoint! macro desugars builder-style to nested wrapper types.

use wayward::prelude::*;
use wayward_server::auth::Protected;
use wayward_server::error::JsonError;
use wayward_server::typed::*;
use wayward_server::typed_response::Strict;

wayward_path!(type UsersPath = "users");
wayward_path!(type UserByIdPath = "users" / u32);
wayward_path!(type TagsPath = "tags");

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
}

#[derive(serde::Deserialize)]
struct CreateUser {
    name: String,
}

#[derive(Clone)]
struct AuthUser;
impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_parts: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser)
    }
}

struct CreateUserValidator;
impl Validate<CreateUser> for CreateUserValidator {
    fn validate(_body: &CreateUser) -> Result<(), String> {
        Ok(())
    }
}

struct V1;
impl ApiVersion for V1 {
    const PREFIX: &'static str = "v1";
}

// --- Simple: GET with response ---
type GetTags = endpoint!(GET TagsPath => Json<Vec<String>>);

// --- With errors ---
type GetUser = endpoint!(GET UserByIdPath => Json<User>, errors: JsonError);

// --- POST with body ---
type CreateUserEndpoint = endpoint!(
    POST UsersPath,
    body: Json<CreateUser> => Json<User>,
    auth: AuthUser,
    errors: JsonError,
);

// --- Full options ---
type FullEndpoint = endpoint!(
    POST UsersPath,
    body: Json<CreateUser> => Json<User>,
    auth: AuthUser,
    validate: CreateUserValidator,
    content_type: json,
    errors: JsonError,
    strict: true,
);

// --- Versioned ---
type VersionedGet = endpoint!(
    GET UsersPath => Json<Vec<User>>,
    version: V1,
);

// Verify they're all valid ApiSpec types
fn _check() {
    fn _assert<T: wayward_core::ApiSpec>() {}
    _assert::<GetTags>();
    _assert::<GetUser>();
    _assert::<CreateUserEndpoint>();
    _assert::<FullEndpoint>();
    _assert::<VersionedGet>();
}

fn main() {}
