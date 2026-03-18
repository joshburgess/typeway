//! The type-safe [`Client`] for calling API endpoints.

use url::Url;

use crate::call::CallEndpoint;
use crate::error::ClientError;

/// A type-safe HTTP client.
///
/// Calls are made via `client.call::<EndpointType>(args)`, which is fully
/// type-checked against the endpoint's path captures, request body, and
/// response type.
///
/// # Example
///
/// ```ignore
/// let client = Client::new("http://localhost:3000").unwrap();
///
/// // GET /users/42 — returns Json<User>
/// let user = client.call::<GetEndpoint<UserByIdPath, Json<User>>>((42u32,)).await?;
///
/// // POST /users — sends CreateUser, returns Json<User>
/// let new_user = client.call::<PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>>(
///     ((), CreateUser { name: "Alice".into(), email: "a@b.com".into() })
/// ).await?;
/// ```
pub struct Client {
    base_url: Url,
    inner: reqwest::Client,
}

impl Client {
    /// Create a new client pointing at the given base URL.
    pub fn new(base_url: &str) -> Result<Self, ClientError> {
        let base_url = Url::parse(base_url)?;
        Ok(Client {
            base_url,
            inner: reqwest::Client::new(),
        })
    }

    /// Create a client with a custom `reqwest::Client`.
    pub fn with_reqwest(base_url: &str, client: reqwest::Client) -> Result<Self, ClientError> {
        let base_url = Url::parse(base_url)?;
        Ok(Client {
            base_url,
            inner: client,
        })
    }

    /// Call an endpoint with the given arguments.
    ///
    /// The endpoint type `E` determines the HTTP method, URL path, request
    /// body, and response type. All of these are verified at compile time.
    pub async fn call<E: CallEndpoint>(&self, args: E::Args) -> Result<E::Response, ClientError> {
        let path = E::build_path(&args);
        let url = self.base_url.join(&path)?;
        let method = E::method();

        let mut request = self.inner.request(method, url);

        if let Some(body_result) = E::request_body(&args) {
            let body = body_result?;
            request = request
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body);
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::Status { status, body });
        }

        let bytes = response.bytes().await?;
        E::parse_response(&bytes)
    }
}
