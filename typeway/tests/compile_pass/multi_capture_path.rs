// Paths with multiple captures compile and extract correctly.

use typeway::prelude::*;

typeway_path!(type UserPostPath = "users" / u32 / "posts" / u64);
typeway_path!(type OrgTeamPath = "orgs" / String / "teams" / String);

#[derive(serde::Serialize)]
struct Post { id: u64 }

// Handler with multi-capture path.
async fn get_post(path: Path<UserPostPath>) -> Json<Post> {
    let (user_id, post_id) = path.0;
    let _ = user_id;
    Json(Post { id: post_id })
}

// String captures.
async fn get_team(path: Path<OrgTeamPath>) -> String {
    let (org, team) = path.0;
    format!("{}/{}", org, team)
}

type API = (
    GetEndpoint<UserPostPath, Post>,
    GetEndpoint<OrgTeamPath, String>,
);

fn main() {
    let _ = Server::<API>::new((bind!(get_post), bind!(get_team)));
}
