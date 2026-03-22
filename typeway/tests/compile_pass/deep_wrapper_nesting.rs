// Deep nesting of wrapper types compiles and satisfies ApiSpec.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::typed::*;

typeway_path!(type UsersPath = "users");

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct User { name: String }

struct V1;
impl ApiVersion for V1 { const PREFIX: &'static str = "v1"; }

struct UserValidator;
impl Validate<User> for UserValidator {
    fn validate(body: &User) -> Result<(), String> {
        if body.name.is_empty() { return Err("empty".into()); }
        Ok(())
    }
}

struct StandardRate;
impl RateLimit for StandardRate {
    const MAX_REQUESTS: u32 = 100;
    const WINDOW_SECS: u64 = 60;
}

// Four levels of nesting: Requires + Versioned + RateLimited + Validated
type DeepEndpoint = Requires<
    AuthRequired,
    Versioned<
        V1,
        RateLimited<
            StandardRate,
            Validated<
                UserValidator,
                PostEndpoint<UsersPath, User, User>,
            >,
        >,
    >,
>;

type API = (DeepEndpoint,);

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<API>();
}

fn main() {}
