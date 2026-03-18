// Type-level endpoint builder — demonstrates what's possible in Rust's
// type system for type-level method chaining.

use wayward::prelude::*;
use wayward_server::ep::*;

wayward_path!(type UsersPath = "users");
wayward_path!(type UserByIdPath = "users" / u32);

#[derive(serde::Serialize)]
struct User {
    name: String,
}

// --- Simple GET: Ep<Get, Path> with Build ---
type GetUsers = <GET<UsersPath, Json<Vec<User>>> as Build>::Out;

// --- Simple DELETE ---
type DeleteUser = <DELETE<UserByIdPath, ()> as Build>::Out;

// --- GET with chained error type ---
type GetUserWithErr = <
    <GET<UsersPath, Json<User>> as WithErr<wayward_server::JsonError>>::Out
    as Build
>::Out;

// --- POST with body via chaining ---
type CreateUser = <
    <
        <POST<UsersPath> as WithReq<Json<User>>>::Out
        as WithRes<Json<User>>
    >::Out
    as Build
>::Out;

// --- GET with auth (chained) ---
#[derive(Clone)]
struct AuthUser;
impl wayward_server::ep::NotUnset for AuthUser {}
impl FromRequestParts for AuthUser {
    type Error = (http::StatusCode, String);
    fn from_request_parts(_: &http::request::Parts) -> Result<Self, Self::Error> {
        Ok(AuthUser)
    }
}

type ProtectedGet = <
    <GET<UsersPath, Json<Vec<User>>> as WithAuth<AuthUser>>::Out
    as Build
>::Out;

// Verify they're all ApiSpec
fn _check() {
    fn _assert<T: wayward_core::ApiSpec>() {}
    _assert::<GetUsers>();
    _assert::<DeleteUser>();
    _assert::<GetUserWithErr>();
    _assert::<CreateUser>();
    _assert::<ProtectedGet>();
}

fn main() {}
