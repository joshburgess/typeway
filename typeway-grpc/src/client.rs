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
            config: $crate::interceptors::GrpcClientConfig,
        }

        impl $name {
            /// Create a new gRPC client pointing at the given base URL.
            ///
            /// The client is configured with HTTP/2 prior knowledge and a
            /// default 30-second timeout.
            $vis fn new(base_url: &str) -> ::core::result::Result<Self, $crate::client::GrpcClientError> {
                Self::with_config(base_url, $crate::interceptors::GrpcClientConfig::default())
            }

            /// Create a new gRPC client with the given configuration.
            ///
            /// The client is configured with HTTP/2 prior knowledge. The
            /// `config` controls default metadata, timeout, and interceptors
            /// applied to every request.
            $vis fn with_config(
                base_url: &str,
                config: $crate::interceptors::GrpcClientConfig,
            ) -> ::core::result::Result<Self, $crate::client::GrpcClientError> {
                let base_url = ::url::Url::parse(base_url)
                    .map_err($crate::client::GrpcClientError::Url)?;
                let mut builder = ::reqwest::Client::builder()
                    .http2_prior_knowledge();
                if let ::core::option::Option::Some(timeout) = config.timeout {
                    builder = builder.timeout(timeout);
                }
                let inner = builder.build()
                    .map_err($crate::client::GrpcClientError::Http)?;
                let service_path = ::std::format!("{}.{}", $package, $service);
                ::core::result::Result::Ok($name { base_url, inner, service_path, config })
            }

            /// Create a gRPC client wrapping an existing `reqwest::Client`.
            ///
            /// The caller is responsible for configuring the client for HTTP/2.
            /// Uses default configuration (no metadata, no interceptors, 30s timeout).
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
                    config: $crate::interceptors::GrpcClientConfig::default(),
                })
            }

            /// Return the fully-qualified service path (e.g., `"users.v1.UserService"`).
            $vis fn service_path(&self) -> &str {
                &self.service_path
            }

            /// Return a reference to the client configuration.
            $vis fn config(&self) -> &$crate::interceptors::GrpcClientConfig {
                &self.config
            }

            /// Call a server-streaming gRPC method by name, collecting all
            /// response frames as a `Vec<serde_json::Value>`.
            ///
            /// This is a generic method for streaming endpoints. The
            /// `method_name` should be the snake_case method name (e.g.,
            /// `"list_users"`); it is converted to PascalCase automatically.
            $vis async fn call_streaming(
                &self,
                method_name: &str,
                body: ::serde_json::Value,
            ) -> ::core::result::Result<
                ::std::vec::Vec<::serde_json::Value>,
                $crate::client::GrpcClientError,
            > {
                let pascal_name: ::std::string::String = method_name
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

                let json_bytes = ::serde_json::to_vec(&body)
                    .map_err(|e| $crate::client::GrpcClientError::Serialize(e.to_string()))?;
                let framed_body = $crate::framing::encode_grpc_frame(&json_bytes);

                let mut request = self.inner
                    .post(url)
                    .header("content-type", "application/grpc+json")
                    .header("te", "trailers")
                    .body(framed_body);

                for (key, value) in &self.config.default_metadata {
                    request = request.header(key.as_str(), value.as_str());
                }
                for interceptor in &self.config.interceptors {
                    request = interceptor(request);
                }

                let response = request
                    .send()
                    .await
                    .map_err($crate::client::GrpcClientError::Http)?;

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

                let bytes = response.bytes().await
                    .map_err($crate::client::GrpcClientError::Http)?;

                let (data_frames, _trailers) = $crate::framing::decode_grpc_frames(&bytes);

                let items: ::std::vec::Vec<::serde_json::Value> = data_frames
                    .iter()
                    .filter_map(|frame| ::serde_json::from_slice(frame).ok())
                    .collect();

                ::core::result::Result::Ok(items)
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

                    let mut request = self.inner
                        .post(url)
                        .header("content-type", "application/grpc+json")
                        .header("te", "trailers")
                        .body(body);

                    // Apply default metadata from config.
                    for (key, value) in &self.config.default_metadata {
                        request = request.header(key.as_str(), value.as_str());
                    }

                    // Apply request interceptors.
                    for interceptor in &self.config.interceptors {
                        request = interceptor(request);
                    }

                    let response = request
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

/// Generate a gRPC client automatically derived from an API type.
///
/// Unlike [`grpc_client!`] which requires manual endpoint listing, this macro
/// derives the client from the API type alone -- the same type that drives the
/// server, REST client, OpenAPI spec, and `.proto` generation.
///
/// The generated client provides:
///
/// - `new(base_url)` and `with_config(base_url, config)` constructors
/// - `call_method(method_name, body)` for unary RPCs by name
/// - `call_method_streaming(method_name, body)` for server-streaming RPCs
/// - `service_descriptor()` to inspect the generated gRPC method mappings
/// - `proto()` to get the `.proto` definition
///
/// Method names are PascalCase RPC names derived from the API endpoints
/// (e.g., `"GetUser"`, `"ListUsers"`, `"CreateUser"`).
///
/// # Compile-time safety
///
/// The macro emits a compile-time assertion that the API type implements
/// [`GrpcReady`], ensuring all request/response types have `ToProtoType`
/// implementations. This catches missing implementations at build time
/// rather than producing incomplete `.proto` output at runtime.
///
/// # Example
///
/// ```ignore
/// use typeway_grpc::auto_grpc_client;
///
/// auto_grpc_client! {
///     pub struct UserServiceClient;
///     api = MyAPI;
///     service = "UserService";
///     package = "users.v1";
/// }
///
/// // Usage:
/// let client = UserServiceClient::new("http://localhost:3000")?;
///
/// // Discover available methods:
/// let desc = client.service_descriptor();
/// for method in &desc.methods {
///     println!("{}", method.name); // "GetUser", "ListUser", etc.
/// }
///
/// // Call by name:
/// let user = client.call_method("GetUser", serde_json::json!({"param1": "42"})).await?;
/// ```
#[macro_export]
macro_rules! auto_grpc_client {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
        api = $api:ty;
        service = $service:expr;
        package = $package:expr;
    ) => {
        // Compile-time assertion: the API type must be GrpcReady.
        const _: () = {
            fn _assert_grpc_ready<T: $crate::GrpcReady>() {}
            fn _check() { _assert_grpc_ready::<$api>(); }
        };

        $(#[$meta])*
        $vis struct $name {
            base_url: ::url::Url,
            inner: ::reqwest::Client,
            service_path: ::std::string::String,
            config: $crate::interceptors::GrpcClientConfig,
        }

        impl $name {
            /// Create a new auto-derived gRPC client.
            ///
            /// Uses HTTP/2 prior knowledge and the default 30-second timeout.
            $vis fn new(base_url: &str) -> ::core::result::Result<Self, $crate::client::GrpcClientError> {
                Self::with_config(base_url, $crate::interceptors::GrpcClientConfig::default())
            }

            /// Create a new auto-derived gRPC client with custom configuration.
            $vis fn with_config(
                base_url: &str,
                config: $crate::interceptors::GrpcClientConfig,
            ) -> ::core::result::Result<Self, $crate::client::GrpcClientError> {
                let base_url = ::url::Url::parse(base_url)
                    .map_err($crate::client::GrpcClientError::Url)?;
                let mut builder = ::reqwest::Client::builder()
                    .http2_prior_knowledge();
                if let ::core::option::Option::Some(timeout) = config.timeout {
                    builder = builder.timeout(timeout);
                }
                let inner = builder.build()
                    .map_err($crate::client::GrpcClientError::Http)?;
                let service_path = ::std::format!("{}.{}", $package, $service);
                ::core::result::Result::Ok($name { base_url, inner, service_path, config })
            }

            /// Create a client wrapping an existing `reqwest::Client`.
            ///
            /// The caller is responsible for configuring HTTP/2 support.
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
                    config: $crate::interceptors::GrpcClientConfig::default(),
                })
            }

            /// Return the fully-qualified service path (e.g., `"users.v1.UserService"`).
            $vis fn service_path(&self) -> &str {
                &self.service_path
            }

            /// Get the service descriptor for this API.
            ///
            /// Lists all available RPC methods, their HTTP method mappings,
            /// and path patterns.
            $vis fn service_descriptor(&self) -> $crate::GrpcServiceDescriptor {
                <$api as $crate::ApiToServiceDescriptor>::service_descriptor($service, $package)
            }

            /// Get the `.proto` definition for this API.
            $vis fn proto(&self) -> ::std::string::String {
                <$api as $crate::ApiToProto>::to_proto($service, $package)
            }

            /// Call a unary gRPC method by name with a JSON body.
            ///
            /// The `method_name` should be PascalCase (e.g., `"GetUser"`,
            /// `"ListUsers"`).
            $vis async fn call_method(
                &self,
                method_name: &str,
                body: ::serde_json::Value,
            ) -> ::core::result::Result<::serde_json::Value, $crate::client::GrpcClientError> {
                let path = ::std::format!("/{}/{}", self.service_path, method_name);
                let url = self.base_url.join(&path)
                    .map_err($crate::client::GrpcClientError::Url)?;

                let json_bytes = ::serde_json::to_vec(&body)
                    .map_err(|e| $crate::client::GrpcClientError::Serialize(e.to_string()))?;
                let framed = $crate::framing::encode_grpc_frame(&json_bytes);

                let mut request = self.inner
                    .post(url)
                    .header("content-type", "application/grpc+json")
                    .header("te", "trailers")
                    .body(framed);

                for (key, value) in &self.config.default_metadata {
                    request = request.header(key.as_str(), value.as_str());
                }
                for interceptor in &self.config.interceptors {
                    request = interceptor(request);
                }

                let response = request
                    .send()
                    .await
                    .map_err($crate::client::GrpcClientError::Http)?;

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

                let body_bytes = response.bytes().await
                    .map_err($crate::client::GrpcClientError::Http)?;
                let unframed = $crate::framing::decode_grpc_frame(&body_bytes)
                    .unwrap_or(&body_bytes);

                ::serde_json::from_slice(unframed)
                    .map_err(|e| $crate::client::GrpcClientError::Deserialize(e.to_string()))
            }

            /// Call a server-streaming gRPC method by name.
            ///
            /// Returns all response frames as a `Vec<serde_json::Value>`.
            /// The `method_name` should be PascalCase (e.g., `"ListUsers"`).
            $vis async fn call_method_streaming(
                &self,
                method_name: &str,
                body: ::serde_json::Value,
            ) -> ::core::result::Result<
                ::std::vec::Vec<::serde_json::Value>,
                $crate::client::GrpcClientError,
            > {
                let path = ::std::format!("/{}/{}", self.service_path, method_name);
                let url = self.base_url.join(&path)
                    .map_err($crate::client::GrpcClientError::Url)?;

                let json_bytes = ::serde_json::to_vec(&body)
                    .map_err(|e| $crate::client::GrpcClientError::Serialize(e.to_string()))?;
                let framed = $crate::framing::encode_grpc_frame(&json_bytes);

                let mut request = self.inner
                    .post(url)
                    .header("content-type", "application/grpc+json")
                    .header("te", "trailers")
                    .body(framed);

                for (key, value) in &self.config.default_metadata {
                    request = request.header(key.as_str(), value.as_str());
                }
                for interceptor in &self.config.interceptors {
                    request = interceptor(request);
                }

                let response = request
                    .send()
                    .await
                    .map_err($crate::client::GrpcClientError::Http)?;

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

                let body_bytes = response.bytes().await
                    .map_err($crate::client::GrpcClientError::Http)?;
                let (data_frames, _trailers) = $crate::framing::decode_grpc_frames(&body_bytes);

                let items: ::std::vec::Vec<::serde_json::Value> = data_frames
                    .iter()
                    .filter_map(|frame| ::serde_json::from_slice(frame).ok())
                    .collect();

                ::core::result::Result::Ok(items)
            }
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
