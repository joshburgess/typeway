//! The type-safe [`Client`] for calling API endpoints.

use serde::Serialize;
use url::Url;

use crate::call::CallEndpoint;
use crate::config::ClientConfig;
use crate::error::ClientError;
use crate::request_builder::{CallOverrides, RequestBuilder, ResponseMeta};
use crate::retry::RetryPolicy;
use crate::typed_response::TypedResponse;

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
    pub(crate) base_url: Url,
    pub(crate) inner: reqwest::Client,
    pub(crate) config: ClientConfig,
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
        if !config.default_headers.is_empty() {
            builder = builder.default_headers(config.default_headers.clone());
        }
        if config.cookie_store {
            builder = builder.cookie_store(true);
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

    /// Call an endpoint with query parameters appended to the URL.
    ///
    /// Works exactly like [`call`](Client::call) but serializes `query` via
    /// [`serde_urlencoded`] and appends the result as a query string.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #[derive(Serialize)]
    /// struct Pagination { page: u32, limit: u32 }
    ///
    /// let users = client
    ///     .call_with_query::<ListUsersEndpoint>((), &Pagination { page: 2, limit: 20 })
    ///     .await?;
    /// ```
    pub async fn call_with_query<E: CallEndpoint, Q: Serialize>(
        &self,
        args: E::Args,
        query: &Q,
    ) -> Result<E::Response, ClientError> {
        let query_string = serde_urlencoded::to_string(query)
            .map_err(|e| ClientError::Serialize(e.to_string()))?;
        let overrides = CallOverrides {
            extra_headers: None,
            query_string: Some(query_string),
            query_params: None,
            timeout: None,
        };
        let policy = &self.config.retry_policy;

        if policy.max_retries == 0 {
            return self
                .call_inner::<E>(&args, Some(&overrides))
                .await
                .map(|(_meta, body)| body);
        }

        self.call_with_retry_query::<E>(&args, &overrides, policy)
            .await
    }

    /// Call an endpoint and return the full response metadata alongside the body.
    ///
    /// This is the same as [`call`](Client::call) but wraps the result in a
    /// [`TypedResponse`] that exposes the HTTP status code and headers.
    ///
    /// If a retry policy is configured, retries are applied as with `call`.
    pub async fn call_full<E: CallEndpoint>(
        &self,
        args: E::Args,
    ) -> Result<TypedResponse<E::Response>, ClientError> {
        let policy = &self.config.retry_policy;

        if policy.max_retries == 0 {
            let (meta, body) = self.call_inner::<E>(&args, None).await?;
            return Ok(TypedResponse {
                body,
                status: meta.status,
                headers: meta.headers,
            });
        }

        // For retried calls, we use call_with_retry which returns just the body.
        // The metadata from the final successful attempt is captured via call_inner.
        self.call_with_retry_full::<E>(&args, policy).await
    }

    /// Start building a request to an endpoint with per-call overrides.
    ///
    /// Returns a [`RequestBuilder`] that allows adding extra headers, query
    /// parameters, or a per-request timeout before sending. Retries are
    /// **not** applied on the builder path.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let user = client
    ///     .request::<GetUserEndpoint>((42u32,))
    ///     .header(http::header::ACCEPT, HeaderValue::from_static("application/json"))
    ///     .timeout(Duration::from_secs(5))
    ///     .send()
    ///     .await?;
    /// ```
    pub fn request<E: CallEndpoint>(&self, args: E::Args) -> RequestBuilder<'_, E> {
        RequestBuilder::new(self, args)
    }

    /// Execute a single request attempt (no retry).
    async fn call_once<E: CallEndpoint>(&self, args: &E::Args) -> Result<E::Response, ClientError> {
        self.call_inner::<E>(args, None)
            .await
            .map(|(_meta, body)| body)
    }

    /// Core request execution shared by `call_once`, `call_full`, and `RequestBuilder`.
    ///
    /// Returns the response metadata and deserialized body. When `overrides` is
    /// `Some`, extra headers, query parameters, and a per-request timeout are
    /// applied to this single attempt.
    pub(crate) async fn call_inner<E: CallEndpoint>(
        &self,
        args: &E::Args,
        overrides: Option<&CallOverrides>,
    ) -> Result<(ResponseMeta, E::Response), ClientError> {
        let path = E::build_path(args);
        let mut url = self.base_url.join(&path)?;
        let method = E::method();

        // Apply serialized query string (from `call_with_query`).
        if let Some(ovr) = overrides {
            if let Some(qs) = &ovr.query_string {
                if !qs.is_empty() {
                    url.set_query(Some(qs));
                }
            }
        }

        // Apply per-call query parameters.
        if let Some(ovr) = overrides {
            if let Some(params) = &ovr.query_params {
                let mut pairs = url.query_pairs_mut();
                for (key, value) in params {
                    pairs.append_pair(key, value);
                }
            }
        }

        let tracing_enabled = self.config.enable_tracing;
        let start = if tracing_enabled {
            Some(std::time::Instant::now())
        } else {
            None
        };

        // Keep copies for tracing log messages after the request is sent.
        let (trace_method, trace_url) = if tracing_enabled {
            (Some(method.clone()), Some(url.clone()))
        } else {
            (None, None)
        };

        if tracing_enabled {
            tracing::debug!(
                http.method = %method,
                http.url = %url,
                "sending request"
            );
        }

        let mut request = self.inner.request(method, url);

        if let Some(body_result) = E::request_body(args) {
            let body = body_result?;
            request = request
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body);
        }

        // Apply per-call extra headers.
        if let Some(ovr) = overrides {
            if let Some(headers) = &ovr.extra_headers {
                for (name, value) in headers {
                    request = request.header(name, value);
                }
            }
            if let Some(timeout) = ovr.timeout {
                request = request.timeout(timeout);
            }
        }

        // Apply request interceptors.
        for interceptor in &self.config.request_interceptors {
            request = interceptor(request);
        }

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) if e.is_timeout() => {
                if let (Some(start), Some(m), Some(u)) = (start, &trace_method, &trace_url) {
                    tracing::debug!(
                        http.method = %m,
                        http.url = %u,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "request timed out"
                    );
                }
                return Err(ClientError::Timeout);
            }
            Err(e) => {
                if let (Some(start), Some(m), Some(u)) = (start, &trace_method, &trace_url) {
                    tracing::debug!(
                        http.method = %m,
                        http.url = %u,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "request failed"
                    );
                }
                return Err(ClientError::Request(e));
            }
        };

        // Apply response interceptors.
        for interceptor in &self.config.response_interceptors {
            interceptor(&response);
        }

        let status = response.status();
        let headers = response.headers().clone();

        if let (Some(start), Some(m), Some(u)) = (start, &trace_method, &trace_url) {
            tracing::debug!(
                http.method = %m,
                http.url = %u,
                http.status = status.as_u16(),
                duration_ms = start.elapsed().as_millis() as u64,
                "received response"
            );
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::Status { status, body });
        }

        let bytes = response.bytes().await?;
        let body = E::parse_response(&bytes)?;

        Ok((ResponseMeta { status, headers }, body))
    }

    /// Execute a request with retries according to the given policy.
    async fn call_with_retry<E: CallEndpoint>(
        &self,
        args: &E::Args,
        policy: &RetryPolicy,
    ) -> Result<E::Response, ClientError> {
        self.call_with_retry_full::<E>(args, policy)
            .await
            .map(|typed| typed.body)
    }

    /// Execute a request with retries, returning full response metadata.
    async fn call_with_retry_full<E: CallEndpoint>(
        &self,
        args: &E::Args,
        policy: &RetryPolicy,
    ) -> Result<TypedResponse<E::Response>, ClientError> {
        let mut last_error: ClientError;

        // Initial attempt (attempt 0).
        match self.call_inner::<E>(args, None).await {
            Ok((meta, body)) => {
                return Ok(TypedResponse {
                    body,
                    status: meta.status,
                    headers: meta.headers,
                });
            }
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

            match self.call_inner::<E>(args, None).await {
                Ok((meta, body)) => {
                    return Ok(TypedResponse {
                        body,
                        status: meta.status,
                        headers: meta.headers,
                    });
                }
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

    /// Execute a query-parameterized request with retries.
    async fn call_with_retry_query<E: CallEndpoint>(
        &self,
        args: &E::Args,
        overrides: &CallOverrides,
        policy: &RetryPolicy,
    ) -> Result<E::Response, ClientError> {
        let mut last_error: ClientError;

        // Initial attempt.
        match self.call_inner::<E>(args, Some(overrides)).await {
            Ok((_meta, body)) => return Ok(body),
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

            match self.call_inner::<E>(args, Some(overrides)).await {
                Ok((_meta, body)) => return Ok(body),
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
