// Error: providing CacheRequired<UserCache> doesn't satisfy CacheRequired<ItemCache>.

use typeway::prelude::*;
use typeway_core::effects::*;
use typeway_server::effects::EffectfulServer;
use std::marker::PhantomData;

struct CacheRequired<T>(PhantomData<T>);
impl<T: Send + Sync + 'static> Effect for CacheRequired<T> {}

struct UserCache;
struct ItemCache;

typeway_path!(type ItemsPath = "items");

type API = (
    Requires<CacheRequired<ItemCache>, GetEndpoint<ItemsPath, String>>,
);

async fn get_items() -> &'static str { "items" }

fn main() {
    // Provides CacheRequired<UserCache> but needs CacheRequired<ItemCache> — should fail.
    let _server = EffectfulServer::<API>::new((bind!(get_items),))
        .provide::<CacheRequired<UserCache>>()
        .ready();
}
