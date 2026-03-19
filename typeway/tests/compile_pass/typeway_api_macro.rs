// The typeway_api! macro compiles.
use typeway::prelude::*;

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    id: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CreateUser {
    name: String,
}

typeway_api! {
    type UsersAPI = {
        GET "users" => Json<Vec<User>>,
        GET "users" / u32 => Json<User>,
        POST "users" [Json<CreateUser>] => Json<User>,
        DELETE "users" / u32 => http::StatusCode,
    };
}

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<UsersAPI>();
}

fn main() {}
