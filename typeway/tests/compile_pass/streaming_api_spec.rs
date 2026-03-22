// Streaming wrappers satisfy ApiSpec when wrapping valid endpoints.
// This test validates the pattern used by typeway-grpc's streaming types.

use typeway::prelude::*;
use std::marker::PhantomData;

typeway_path!(type FeedPath = "feed");
typeway_path!(type UploadPath = "upload");
typeway_path!(type ChatPath = "chat");

#[derive(serde::Serialize, serde::Deserialize)]
struct User { name: String }
#[derive(serde::Serialize, serde::Deserialize)]
struct Chunk { data: Vec<u8> }

// Recreate the streaming wrapper pattern (same as typeway-grpc).
struct TestServerStream<E>(PhantomData<E>);
impl<E: typeway_core::ApiSpec> typeway_core::ApiSpec for TestServerStream<E> {}

struct TestClientStream<E>(PhantomData<E>);
impl<E: typeway_core::ApiSpec> typeway_core::ApiSpec for TestClientStream<E> {}

struct TestBidiStream<E>(PhantomData<E>);
impl<E: typeway_core::ApiSpec> typeway_core::ApiSpec for TestBidiStream<E> {}

// All streaming modes in one API type.
type API = (
    GetEndpoint<FeedPath, Vec<User>>,
    TestServerStream<GetEndpoint<FeedPath, Vec<User>>>,
    TestClientStream<PostEndpoint<UploadPath, Chunk, String>>,
    TestBidiStream<GetEndpoint<ChatPath, String>>,
);

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    _assert_api::<API>();
    _assert_api::<TestServerStream<GetEndpoint<FeedPath, Vec<User>>>>();
    _assert_api::<TestClientStream<PostEndpoint<UploadPath, Chunk, String>>>();
    _assert_api::<TestBidiStream<GetEndpoint<ChatPath, String>>>();
}

fn main() {}
