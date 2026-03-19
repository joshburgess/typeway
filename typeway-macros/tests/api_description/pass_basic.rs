use typeway_macros::api_description;
use typeway_server::{Json, Path};

#[derive(Clone, serde::Serialize)]
struct User {
    id: u32,
    name: String,
}

#[api_description]
trait UserAPI {
    #[get("users")]
    async fn list_users() -> Json<Vec<User>>;

    #[get("users" / u32)]
    async fn get_user(path: Path<GetUserPath>) -> Json<User>;
}

// Verify the generated spec type exists.
fn _check_spec_type() {
    fn _assert_api_spec<T: typeway_core::ApiSpec>() {}
    _assert_api_spec::<UserAPISpec>();
}

fn main() {}
