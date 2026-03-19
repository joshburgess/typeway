//! Request extraction traits and built-in extractors.
//!
//! Extractors pull typed data from incoming HTTP requests. Each handler
//! argument is an extractor that implements [`FromRequestParts`] (for
//! metadata like path captures, headers, query strings) or [`FromRequest`]
//! (for the request body).

use std::sync::Arc;

use bytes::Bytes;
use http::request::Parts;
use http::StatusCode;
use serde::de::DeserializeOwned;

use wayward_core::{ExtractPath, PathSpec};

use crate::response::IntoResponse;

/// Extract a value from request metadata (URI, headers, extensions).
///
/// Implementors can be used as handler arguments. Multiple `FromRequestParts`
/// extractors can appear in a single handler since they don't consume the body.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be extracted from request metadata",
    label = "does not implement `FromRequestParts`",
    note = "valid extractors: `Path<P>`, `State<T>`, `Query<T>`, `HeaderMap`"
)]
pub trait FromRequestParts: Sized + Send {
    /// The error type returned when extraction fails.
    type Error: IntoResponse;

    /// Extract this type from the request parts.
    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error>;
}

/// Extract a value by consuming the request body.
///
/// At most one `FromRequest` extractor can appear per handler (as the last
/// argument), since it consumes the body.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be extracted from the request body",
    label = "does not implement `FromRequest`",
    note = "valid body extractors: `Json<T>`, `Bytes`, `String`, `()`"
)]
pub trait FromRequest: Sized + Send {
    /// The error type returned when extraction fails.
    type Error: IntoResponse;

    /// Extract this type from the request parts and pre-collected body bytes.
    ///
    /// This is async for interface consistency, though body bytes are already
    /// collected by the router before dispatch.
    fn from_request(
        parts: &Parts,
        body: bytes::Bytes,
    ) -> impl std::future::Future<Output = Result<Self, Self::Error>> + Send;
}

// ---------------------------------------------------------------------------
// Path extractor
// ---------------------------------------------------------------------------

/// Extracts typed path captures from the URL.
///
/// The path segments are stored in request extensions by the router
/// before the handler is called.
///
/// # Example
///
/// ```ignore
/// async fn get_user(Path((id,)): Path<path!("users" / u32)>) -> Json<User> {
///     // id: u32, extracted from /users/42
/// }
/// ```
pub struct Path<P: PathSpec>(pub P::Captures);

/// Raw path segments stored in request extensions by the router.
#[derive(Clone)]
pub struct PathSegments(pub Arc<Vec<String>>);

impl<P> FromRequestParts for Path<P>
where
    P: PathSpec + ExtractPath + Send,
    P::Captures: Send,
{
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        let segments = parts.extensions.get::<PathSegments>().ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "missing path segments in request extensions".to_string(),
            )
        })?;

        let seg_refs: Vec<&str> = segments.0.iter().map(|s| s.as_str()).collect();
        P::extract(&seg_refs).map(Path).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!(
                    "failed to parse path segments for pattern: {}",
                    P::pattern()
                ),
            )
        })
    }
}

// ---------------------------------------------------------------------------
// State extractor
// ---------------------------------------------------------------------------

/// Extracts shared application state.
///
/// State must be added to the server via [`Server::with_state`](crate::server::Server::with_state)
/// and is injected into request extensions.
///
/// # Example
///
/// ```
/// use wayward_server::State;
///
/// #[derive(Clone)]
/// struct DbPool;
///
/// async fn list_users(State(db): State<DbPool>) -> &'static str {
///     let _ = db;
///     "users"
/// }
/// ```
pub struct State<T>(pub T);

impl<T: Clone + Send + Sync + 'static> FromRequestParts for State<T> {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        parts
            .extensions
            .get::<T>()
            .cloned()
            .map(State)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "state of type `{}` not found — did you call .with_state()?",
                        std::any::type_name::<T>()
                    ),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Query extractor
// ---------------------------------------------------------------------------

/// Extracts typed query string parameters.
///
/// # Example
///
/// ```
/// use wayward_server::Query;
///
/// #[derive(serde::Deserialize)]
/// struct Pagination { page: u32, per_page: u32 }
///
/// async fn list_users(Query(p): Query<Pagination>) -> String {
///     format!("page={}, per_page={}", p.page, p.per_page)
/// }
/// ```
pub struct Query<T>(pub T);

impl<T: DeserializeOwned + Send> FromRequestParts for Query<T> {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        let query = parts.uri.query().unwrap_or("");
        serde_urlencoded::from_str::<T>(query)
            .map(Query)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("failed to parse query string: {e}"),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// HeaderMap extractor
// ---------------------------------------------------------------------------

impl FromRequestParts for http::HeaderMap {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        Ok(parts.headers.clone())
    }
}

// ---------------------------------------------------------------------------
// Extension extractor
// ---------------------------------------------------------------------------

/// Extracts a value from request extensions.
///
/// Use this to access arbitrary types injected by middleware or other
/// infrastructure. Unlike [`State`], extensions are per-request.
///
/// # Example
///
/// ```
/// use wayward_server::Extension;
///
/// #[derive(Clone)]
/// struct RequestId(String);
///
/// async fn handler(Extension(id): Extension<RequestId>) -> String {
///     format!("Request: {}", id.0)
/// }
/// ```
pub struct Extension<T>(pub T);

impl<T: Clone + Send + Sync + 'static> FromRequestParts for Extension<T> {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        parts
            .extensions
            .get::<T>()
            .cloned()
            .map(Extension)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "extension of type `{}` not found in request",
                        std::any::type_name::<T>()
                    ),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Cookie extractor
// ---------------------------------------------------------------------------

/// Trait for types that extract a specific named cookie.
///
/// # Example
///
/// ```
/// use wayward_server::extract::{Cookie, NamedCookie};
///
/// struct SessionId(String);
///
/// impl NamedCookie for SessionId {
///     const COOKIE_NAME: &'static str = "session_id";
///     fn from_value(value: &str) -> Result<Self, String> {
///         Ok(SessionId(value.to_string()))
///     }
/// }
///
/// async fn handler(Cookie(session): Cookie<SessionId>) -> String {
///     format!("session: {}", session.0)
/// }
/// ```
pub trait NamedCookie: Sized + Send {
    /// The cookie name to extract.
    const COOKIE_NAME: &'static str;
    /// Parse the cookie value string into this type.
    fn from_value(value: &str) -> Result<Self, String>;
}

/// Extracts a single cookie by name.
pub struct Cookie<T>(pub T);

impl<T: NamedCookie + 'static> FromRequestParts for Cookie<T> {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        let cookies = parts
            .headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        for pair in cookies.split(';') {
            let pair = pair.trim();
            if let Some(value) = pair
                .strip_prefix(T::COOKIE_NAME)
                .and_then(|s| s.strip_prefix('='))
            {
                return T::from_value(value)
                    .map(Cookie)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e));
            }
        }

        Err((
            StatusCode::BAD_REQUEST,
            format!("missing cookie: {}", T::COOKIE_NAME),
        ))
    }
}

/// Extracts all cookies as a key-value map.
///
/// ```
/// use wayward_server::extract::CookieJar;
///
/// async fn handler(cookies: CookieJar) -> String {
///     let session = cookies.get("session_id").unwrap_or("none");
///     format!("session: {session}")
/// }
/// ```
pub struct CookieJar(pub std::collections::HashMap<String, String>);

impl CookieJar {
    /// Get a cookie value by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(|s| s.as_str())
    }
}

impl FromRequestParts for CookieJar {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        let cookies = parts
            .headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let map = cookies
            .split(';')
            .filter_map(|pair| {
                let pair = pair.trim();
                let (name, value) = pair.split_once('=')?;
                Some((name.to_string(), value.to_string()))
            })
            .collect();

        Ok(CookieJar(map))
    }
}

// ---------------------------------------------------------------------------
// Method extractor
// ---------------------------------------------------------------------------

/// Extracts the HTTP method from the request.
impl FromRequestParts for http::Method {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        Ok(parts.method.clone())
    }
}

/// Extracts the request URI.
impl FromRequestParts for http::Uri {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        Ok(parts.uri.clone())
    }
}

// ---------------------------------------------------------------------------
// Header extractor
// ---------------------------------------------------------------------------

/// Extracts a single header value by name.
///
/// The header name is derived from `T::HEADER_NAME`. Implement [`NamedHeader`]
/// on your type to use this extractor.
///
/// # Example
///
/// ```ignore
/// struct ContentType(String);
///
/// impl NamedHeader for ContentType {
///     const HEADER_NAME: &'static str = "content-type";
///     fn from_value(value: &str) -> Result<Self, String> {
///         Ok(ContentType(value.to_string()))
///     }
/// }
///
/// async fn handler(Header(ct): Header<ContentType>) -> String {
///     format!("Content-Type: {}", ct.0)
/// }
/// ```
pub struct Header<T>(pub T);

/// Trait for types that can be extracted from a named HTTP header.
pub trait NamedHeader: Sized + Send {
    /// The header name (lowercase), e.g. `"content-type"`.
    const HEADER_NAME: &'static str;

    /// Parse the header value string into this type.
    fn from_value(value: &str) -> Result<Self, String>;
}

impl<T: NamedHeader + 'static> FromRequestParts for Header<T> {
    type Error = (StatusCode, String);

    fn from_request_parts(parts: &Parts) -> Result<Self, Self::Error> {
        let value = parts
            .headers
            .get(T::HEADER_NAME)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("missing required header: {}", T::HEADER_NAME),
                )
            })?
            .to_str()
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("invalid header value for: {}", T::HEADER_NAME),
                )
            })?;

        T::from_value(value)
            .map(Header)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))
    }
}

// ---------------------------------------------------------------------------
// Body extractors (FromRequest)
// ---------------------------------------------------------------------------

/// JSON request body extractor.
///
/// Parses the request body as JSON. Requires `Content-Type: application/json`.
///
/// # Example
///
/// ```ignore
/// async fn create_user(Json(body): Json<CreateUser>) -> Json<User> {
///     // body: CreateUser, deserialized from JSON
/// }
/// ```
impl<T: DeserializeOwned + Send> FromRequest for crate::response::Json<T> {
    type Error = (StatusCode, String);

    async fn from_request(_parts: &Parts, body: bytes::Bytes) -> Result<Self, Self::Error> {
        serde_json::from_slice(&body)
            .map(crate::response::Json)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")))
    }
}

impl FromRequest for Bytes {
    type Error = (StatusCode, String);

    async fn from_request(_parts: &Parts, body: bytes::Bytes) -> Result<Self, Self::Error> {
        Ok(body)
    }
}

impl FromRequest for String {
    type Error = (StatusCode, String);

    async fn from_request(_parts: &Parts, body: bytes::Bytes) -> Result<Self, Self::Error> {
        String::from_utf8(body.to_vec()).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("request body is not valid UTF-8: {e}"),
            )
        })
    }
}

/// Unit extractor — always succeeds, ignoring the body.
impl FromRequest for () {
    type Error = (StatusCode, String);

    async fn from_request(_parts: &Parts, _body: bytes::Bytes) -> Result<Self, Self::Error> {
        Ok(())
    }
}
