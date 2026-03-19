use typeway_macros::api_description;

#[api_description]
trait BadAPI {
    #[get("hello")]
    fn not_async() -> String;
}

fn main() {}
