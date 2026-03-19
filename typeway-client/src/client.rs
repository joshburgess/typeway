//! The type-safe [`Client`] for calling API endpoints.

use url::Url;

use crate::call::CallEndpoint;
use crate::config::ClientConfig;
use crate::error::ClientError;
use crate::retry::RetryPolicy;

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
    config: ClientConfig,
}

impl Client {
    /// Create a new client pointing at the given base URL with default config.
    pub fn new(base_url: &str) -> Result<Self, ClientError> {
        Self::with_config(base_url, ClientConfig::default())
    }

    /// Create a client with a custom [`ClientConfig`].
    pub fn with_config(base_url: &str, config: ClientConfig) -> Result<Self, ClientError> {
        let base_url = Url::parse(base_url)?;
        let mut builder = reqwest::Client::builder();
        if let Some(timeout) = config.timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(connect_timeout) = config.connect_timeout {
            builder = builder.connect_timeout(connect_timeout);
        }
        let inner = builder.build().map_err(ClientError::Request)?;
        Ok(Client {
            base_url,
            inner,
            config,
        })
    }

    /// Create a client with a custom `reqwest::Client`.
    ///
    /// Note: timeout settings from the provided `reqwest::Client` take
    /// precedence; the `ClientConfig` retry policy is still used.
    pub fn with_reqwest(base_url: &str, client: reqwest::Client) -> Result<Self, ClientError> {
        let base_url = Url::parse(base_url)?;
        Ok(Client {
            base_url,
            inner: client,
            config: ClientConfig::default(),
        })
    }

    /// Create a client with both a custom `reqwest::Client` and config.
    ///
    /// Timeout fields in `config` are ignored (the provided `reqwest::Client`
    /// owns its own timeout settings), but the retry policy is used.
    pub fn with_reqwest_and_config(
        base_url: &str,
        client: reqwest::Client,
        config: ClientConfig,
    ) -> Result<Self, ClientError> {
        let base_url = Url::parse(base_url)?;
        Ok(Client {
            base_url,
            inner: client,
            config,
        })
    }

    /// Returns a reference to the current [`ClientConfig`].
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Call an endpoint with the given arguments.
    ///
    /// The endpoint type `E` determines the HTTP method, URL path, request
    /// body, and response type. All of these are verified at compile time.
    ///
    /// If a retry policy is configured, retryable failures (matching status
    /// codes or timeouts) will be retried with exponential backoff and jitter.
    pub async fn call<E: CallEndpoint>(&self, args: E::Args) -> Result<E::Response, ClientError> {
        let policy = &self.config.retry_policy;

        if policy.max_retries == 0 {
            return self.call_once::<E>(&args).await;
        }

        self.call_with_retry::<E>(&args, policy).await
    }

    /// Execute a single request attempt (no retry).
    async fn call_once<E: CallEndpoint>(&self, args: &E::Args) -> Result<E::Response, ClientError> {
        let path = E::build_path(args);
        let url = self.base_url.join(&path)?;
        let method = E::method();

        let mut request = self.inner.request(method, url);

        if let Some(body_result) = E::request_body(args) {
            let body = body_result?;
            request = request
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body);
        }

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) if e.is_timeout() => return Err(ClientError::Timeout),
            Err(e) => return Err(ClientError::Request(e)),
        };

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::Status { status, body });
        }

        let bytes = response.bytes().await?;
        E::parse_response(&bytes)
    }

    /// Execute a request with retries according to the given policy.
    async fn call_with_retry<E: CallEndpoint>(
        &self,
        args: &E::Args,
        policy: &RetryPolicy,
    ) -> Result<E::Response, ClientError> {
        let mut last_error: ClientError;

        // Initial attempt (attempt 0).
        match self.call_once::<E>(args).await {
            Ok(response) => return Ok(response),
            Err(e) => {
                if !Self::is_retryable(&e, policy) {
                    return Err(e);
                }
                last_error = e;
            }
        }

        // Retry attempts.
        for attempt in 0..policy.max_retries {
            let backoff = policy.backoff_for_attempt(attempt);
            tokio::time::sleep(backoff).await;

            match self.call_once::<E>(args).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if !Self::is_retryable(&e, policy) {
                        return Err(e);
                    }
                    last_error = e;
                }
            }
        }

        Err(ClientError::RetryExhausted {
            last_error: Box::new(last_error),
            attempts: policy.max_retries + 1,
        })
    }

    /// Check whether an error is retryable under the given policy.
    fn is_retryable(error: &ClientError, policy: &RetryPolicy) -> bool {
        match error {
            ClientError::Status { status, .. } => policy.should_retry_status(*status),
            ClientError::Timeout | ClientError::Request(_) => {
                if error.is_timeout() {
                    policy.retry_on_timeout
                } else {
                    // Transport errors (connection reset, etc.) are retryable
                    // when timeout retries are enabled.
                    policy.retry_on_timeout
                }
            }
            _ => false,
        }
    }
}
