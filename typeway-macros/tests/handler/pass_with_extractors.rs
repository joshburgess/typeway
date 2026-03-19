use typeway_macros::{handler, typeway_path};
use typeway_server::{Json, Path, State};

typeway_path!(type UserByIdPath = "users" / u32);

#[derive(Clone)]
struct AppState;

#[derive(serde::Serialize)]
struct User {
    id: u32,
}

#[handler]
async fn get_user(path: Path<UserByIdPath>, state: State<AppState>) -> Json<User> {
    let _ = (path, state);
    Json(User { id: 1 })
}

fn main() {}
