//! Type-safe gRPC client generation via macro.
//!
//! The [`grpc_client!`] macro generates a named struct with a method for each
//! gRPC endpoint. The client makes HTTP/2 requests with
//! `content-type: application/grpc+json` to gRPC paths, using the same
//! endpoint types as the REST client.
//!
//! # Example
//!
//! ```ignore
//! use typeway_grpc::grpc_client;
//!
//! grpc_client! {
//!     pub struct UserServiceClient;
//!     service = "UserService";
//!     package = "users.v1";
//!
//!     list_users => GetEndpoint<UsersPath, Json<Vec<User>>>;
//!     get_user => GetEndpoint<UserByIdPath, Json<User>>;
//!     create_user => PostEndpoint<UsersPath, Json<CreateUser>, Json<User>>;
//! }
//!
//! // Usage:
//! let client = UserServiceClient::new("http://localhost:3000").unwrap();
//! let users = client.list_users(()).await.unwrap();
//! let user = client.get_user((42u32,)).await.unwrap();
//! ```

/// Generate a type-safe gRPC client struct from endpoint types.
///
/// The generated struct wraps a `reqwest::Client` configured for HTTP/2 and
/// provides a method for each endpoint that:
///
/// 1. Serializes the request using the endpoint's `CallEndpoint::request_body`
/// 2. Sends it to the appropriate gRPC path (derived from method name)
/// 3. Checks the `grpc-status` trailer for errors
/// 4. Deserializes the response using `CallEndpoint::parse_response`
///
/// # Syntax
///
/// ```ignore
/// grpc_client! {
///     $(#[$meta:meta])*
///     $vis struct $Name;
///     service = "ServiceName";
///     package = "package.v1";
///
///     $(
///         $(#[$method_meta:meta])*
///         $method_name => $EndpointType;
///     )*
/// }
/// ```
///
/// # Generated API
///
/// - `$Name::new(base_url: &str) -> Result<Self, ClientError>` — creates
///   a client with HTTP/2 prior knowledge enabled.
/// - `$Name::$method_name(args) -> Result<Response, ClientError>` — calls the
///   endpoint via gRPC.
#[macro_export]
macro_rules! grpc_client {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
        service = $service:expr;
        package = $package:expr;

        $(
            $(#[$method_meta:meta])*
            $method:ident => $endpoint:ty;
        )*
    ) => {
        $(#[$meta])*
        $vis struct $name {
            base_url: ::url::Url,
            inner: ::reqwest::Client,
            service_path: ::std::string::String,
        }

        impl $name {
            /// Create a new gRPC client pointing at the given base URL.
            ///
            /// The client is configured with HTTP/2 prior knowledge, which is
            /// required for gRPC communication.
            $vis fn new(base_url: &str) -> ::core::result::Result<Self, $crate::client::GrpcClientError> {
                let base_url = ::url::Url::parse(base_url)
                    .map_err($crate::client::GrpcClientError::Url)?;
                let inner = ::reqwest::Client::builder()
                    .http2_prior_knowledge()
                    .build()
                    .map_err($crate::client::GrpcClientError::Http)?;
                let service_path = ::std::format!("{}.{}", $package, $service);
                ::core::result::Result::Ok($name { base_url, inner, service_path })
            }

            /// Create a gRPC client wrapping an existing `reqwest::Client`.
            ///
            /// The caller is responsible for configuring the client for HTTP/2.
            $vis fn with_client(
                base_url: &str,
                client: ::reqwest::Client,
            ) -> ::core::result::Result<Self, $crate::client::GrpcClientError> {
                let base_url = ::url::Url::parse(base_url)
                    .map_err($crate::client::GrpcClientError::Url)?;
                let service_path = ::std::format!("{}.{}", $package, $service);
                ::core::result::Result::Ok($name {
                    base_url,
                    inner: client,
                    service_path,
                })
            }

            /// Return the fully-qualified service path (e.g., `"users.v1.UserService"`).
            $vis fn service_path(&self) -> &str {
                &self.service_path
            }

            $(
                $(#[$method_meta])*
                $vis async fn $method(
                    &self,
                    args: <$endpoint as ::typeway_client::CallEndpoint>::Args,
                ) -> ::core::result::Result<
                    <$endpoint as ::typeway_client::CallEndpoint>::Response,
                    $crate::client::GrpcClientError,
                > {
                    // Convert snake_case method name to PascalCase for gRPC.
                    let rpc_name = stringify!($method);
                    let pascal_name: ::std::string::String = rpc_name
                        .split('_')
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                ::core::option::Option::Some(f) => {
                                    f.to_uppercase().to_string() + &c.collect::<::std::string::String>()
                                }
                                ::core::option::Option::None => ::std::string::String::new(),
                            }
                        })
                        .collect();

                    let path = ::std::format!("/{}/{}", self.service_path, pascal_name);
                    let url = self.base_url.join(&path)
                        .map_err($crate::client::GrpcClientError::Url)?;

                    // Serialize request body and wrap in gRPC frame.
                    let raw_body = match <$endpoint as ::typeway_client::CallEndpoint>::request_body(&args) {
                        ::core::option::Option::Some(result) => {
                            result.map_err(|e| $crate::client::GrpcClientError::Serialize(e.to_string()))?
                        }
                        ::core::option::Option::None => ::std::vec::Vec::new(),
                    };
                    let body = $crate::framing::encode_grpc_frame(&raw_body);

                    let response = self.inner
                        .post(url)
                        .header("content-type", "application/grpc+json")
                        .header("te", "trailers")
                        .body(body)
                        .send()
                        .await
                        .map_err($crate::client::GrpcClientError::Http)?;

                    // Check grpc-status header for errors.
                    let grpc_status = response.headers()
                        .get("grpc-status")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or(0);

                    if grpc_status != 0 {
                        let grpc_message = response.headers()
                            .get("grpc-message")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string();
                        return ::core::result::Result::Err(
                            $crate::client::GrpcClientError::Status {
                                code: grpc_status,
                                message: grpc_message,
                            }
                        );
                    }

                    // Decode gRPC frame from response.
                    let bytes = response.bytes().await
                        .map_err($crate::client::GrpcClientError::Http)?;
                    let unframed = $crate::framing::decode_grpc_frame(&bytes)
                        .unwrap_or(&bytes);
                    <$endpoint as ::typeway_client::CallEndpoint>::parse_response(unframed)
                        .map_err(|e| $crate::client::GrpcClientError::Deserialize(e.to_string()))
                }
            )*
        }
    };
}

/// Errors that can occur when making gRPC client requests.
#[derive(Debug, thiserror::Error)]
pub enum GrpcClientError {
    /// The gRPC server returned a non-zero status code.
    #[error("gRPC error (code {code}): {message}")]
    Status {
        /// The gRPC status code (e.g., 5 for NOT_FOUND).
        code: i32,
        /// The human-readable error message from the server.
        message: String,
    },

    /// Failed to parse the base URL.
    #[error("invalid URL: {0}")]
    Url(#[source] url::ParseError),

    /// HTTP transport error from reqwest.
    #[error("HTTP error: {0}")]
    Http(#[source] reqwest::Error),

    /// Failed to serialize the request body.
    #[error("serialization error: {0}")]
    Serialize(String),

    /// Failed to deserialize the response body.
    #[error("deserialization error: {0}")]
    Deserialize(String),
}
