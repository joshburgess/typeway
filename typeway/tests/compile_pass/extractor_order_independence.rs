// Extractor order doesn't matter — State before Path, or Path before State.

use typeway::prelude::*;

typeway_path!(type UserPath = "users" / u32);

#[derive(Clone)]
struct Db;

#[derive(serde::Serialize)]
struct User { id: u32 }

// State before Path.
async fn state_then_path(_state: State<Db>, path: Path<UserPath>) -> Json<User> {
    let (id,) = path.0;
    Json(User { id })
}

// Path before State.
async fn path_then_state(path: Path<UserPath>, _state: State<Db>) -> Json<User> {
    let (id,) = path.0;
    Json(User { id })
}

type API = (
    GetEndpoint<UserPath, User>,
    GetEndpoint<UserPath, User>,
);

fn main() {
    let _ = Server::<API>::new((
        bind!(state_then_path),
        bind!(path_then_state),
    ));
}
