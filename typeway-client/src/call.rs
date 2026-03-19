//! The [`CallEndpoint`] trait — type-safe endpoint invocation.

use serde::de::DeserializeOwned;
use serde::Serialize;

use typeway_core::*;

use crate::error::ClientError;

/// Describes how to call a specific endpoint: what arguments it needs,
/// how to build the URL, and how to parse the response.
pub trait CallEndpoint {
    /// The arguments needed to call this endpoint.
    type Args;

    /// The response type returned on success.
    type Response;

    /// The HTTP method.
    fn method() -> http::Method;

    /// Build the URL path by substituting captures into the pattern.
    fn build_path(args: &Self::Args) -> String;

    /// Serialize the request body, if any.
    fn request_body(args: &Self::Args) -> Option<Result<Vec<u8>, ClientError>>;

    /// Deserialize the response body.
    fn parse_response(bytes: &[u8]) -> Result<Self::Response, ClientError>;
}

// ---------------------------------------------------------------------------
// BuildPath: URL construction from capture tuples
// ---------------------------------------------------------------------------

/// Builds a URL path by substituting capture values into `{}` placeholders.
pub trait BuildPath {
    fn build_path(captures: &Self, pattern: &str) -> String;
}

impl BuildPath for () {
    fn build_path(_: &(), pattern: &str) -> String {
        pattern.to_string()
    }
}

macro_rules! impl_build_path {
    ($($idx:tt : $T:ident),+) => {
        impl<$($T: std::fmt::Display,)+> BuildPath for ($($T,)+) {
            fn build_path(captures: &Self, pattern: &str) -> String {
                let mut result = pattern.to_string();
                $(
                    result = result.replacen("{}", &captures.$idx.to_string(), 1);
                )+
                result
            }
        }
    };
}

impl_build_path!(0: A);
impl_build_path!(0: A, 1: B);
impl_build_path!(0: A, 1: B, 2: C);
impl_build_path!(0: A, 1: B, 2: C, 3: D);
impl_build_path!(0: A, 1: B, 2: C, 3: D, 4: E);
impl_build_path!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
impl_build_path!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);
impl_build_path!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H);

// ---------------------------------------------------------------------------
// Bodyless endpoints (GET, DELETE, HEAD, OPTIONS)
// ---------------------------------------------------------------------------

macro_rules! impl_call_bodyless {
    ($Method:ty) => {
        impl<P, Res, Q, Err> CallEndpoint for Endpoint<$Method, P, NoBody, Res, Q, Err>
        where
            P: PathSpec + ExtractPath,
            P::Captures: BuildPath,
            Res: DeserializeOwned,
        {
            type Args = P::Captures;
            type Response = Res;

            fn method() -> http::Method {
                <$Method as HttpMethod>::METHOD
            }

            fn build_path(args: &Self::Args) -> String {
                BuildPath::build_path(args, &P::pattern())
            }

            fn request_body(_args: &Self::Args) -> Option<Result<Vec<u8>, ClientError>> {
                None
            }

            fn parse_response(bytes: &[u8]) -> Result<Self::Response, ClientError> {
                serde_json::from_slice(bytes).map_err(|e| ClientError::Deserialize(e.to_string()))
            }
        }
    };
}

impl_call_bodyless!(Get);
impl_call_bodyless!(Delete);
impl_call_bodyless!(Head);
impl_call_bodyless!(Options);

// ---------------------------------------------------------------------------
// Body endpoints (POST, PUT, PATCH)
// Args = (Captures, RequestBody)
// ---------------------------------------------------------------------------

macro_rules! impl_call_with_body {
    ($Method:ty) => {
        impl<P, Req, Res, Q, Err> CallEndpoint for Endpoint<$Method, P, Req, Res, Q, Err>
        where
            P: PathSpec + ExtractPath,
            P::Captures: BuildPath,
            Req: Serialize,
            Res: DeserializeOwned,
        {
            type Args = (P::Captures, Req);
            type Response = Res;

            fn method() -> http::Method {
                <$Method as HttpMethod>::METHOD
            }

            fn build_path(args: &Self::Args) -> String {
                BuildPath::build_path(&args.0, &P::pattern())
            }

            fn request_body(args: &Self::Args) -> Option<Result<Vec<u8>, ClientError>> {
                Some(serde_json::to_vec(&args.1).map_err(|e| ClientError::Serialize(e.to_string())))
            }

            fn parse_response(bytes: &[u8]) -> Result<Self::Response, ClientError> {
                serde_json::from_slice(bytes).map_err(|e| ClientError::Deserialize(e.to_string()))
            }
        }
    };
}

impl_call_with_body!(Post);
impl_call_with_body!(Put);
impl_call_with_body!(Patch);
