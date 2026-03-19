use typeway_macros::api_description;
use typeway_server::{Json, Path, BoundHandler};

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

// Implement the trait on a concrete struct.
#[derive(Clone)]
struct MyImpl;

impl UserAPI for MyImpl {
    fn list_users(&self) -> impl std::future::Future<Output = Json<Vec<User>>> + Send {
        async { Json(vec![]) }
    }

    fn get_user(&self, _path: Path<GetUserPath>) -> impl std::future::Future<Output = Json<User>> + Send {
        async { Json(User { id: 1, name: "Alice".into() }) }
    }
}

// Verify the bridge function produces the right type.
fn _check_handlers() {
    let _handlers: (BoundHandler<_>, BoundHandler<_>) = serve_user_api(MyImpl);
}

fn main() {}
