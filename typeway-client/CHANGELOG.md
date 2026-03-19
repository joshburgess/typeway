# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **Type-safe HTTP client**: `Client<A>` derived from the same API type as the server
- **`CallEndpoint` trait**: `client.call::<Endpoint>(args)` with compile-time verification of path captures, request bodies, and response types
- **All HTTP methods**: GET, POST, PUT, DELETE, PATCH support
- **`ClientConfig`**: configurable per-request timeout (default 30s), connect timeout (default 10s), and retry policy
- **`RetryPolicy`**: exponential backoff with jitter (0–25%), configurable max retries (default 3), initial backoff (100ms), max backoff (10s), backoff multiplier (2.0), and retryable status codes (429, 502, 503, 504)
- **`RetryPolicy::none()`**: disable all retries
- **`ClientError`**: structured error enum with `Status`, `Url`, `Request`, `Deserialize`, `Serialize`, `Timeout`, and `RetryExhausted` variants
