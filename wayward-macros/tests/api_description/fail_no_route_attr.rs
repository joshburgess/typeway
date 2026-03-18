use wayward_macros::api_description;

#[api_description]
trait BadAPI {
    async fn missing_route() -> String;
}

fn main() {}
