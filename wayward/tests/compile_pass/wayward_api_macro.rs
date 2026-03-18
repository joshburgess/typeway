// The wayward_api! macro compiles.
use wayward::prelude::*;

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    id: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CreateUser {
    name: String,
}

wayward_api! {
    type UsersAPI = {
        GET "users" => Json<Vec<User>>,
        GET "users" / u32 => Json<User>,
        POST "users" [Json<CreateUser>] => Json<User>,
        DELETE "users" / u32 => http::StatusCode,
    };
}

fn _check() {
    fn _assert_api<T: wayward_core::ApiSpec>() {}
    _assert_api::<UsersAPI>();
}

fn main() {}
