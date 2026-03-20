//! API type to `.proto` file generation.
//!
//! [`EndpointToRpc`] converts a single endpoint to an RPC method descriptor.
//! [`CollectRpcs`] walks API tuples, and [`ApiToProto`] produces the complete
//! `.proto` file as a string.
//!
//! # Capture-to-field mapping
//!
//! Path captures are extracted from the pattern string returned by
//! [`ExtractPath::pattern()`]. Since the pattern uses unnamed `{}`
//! placeholders, fields are named `param1`, `param2`, etc. and default
//! to `string` type. Override with custom [`ToProtoType`] impls for
//! more precise types.

use indexmap::IndexSet;

use typeway_core::effects::{Effect, Requires};
use typeway_core::endpoint::{Endpoint, NoBody};
use typeway_core::method::*;
use typeway_core::path::{ExtractPath, PathSpec};

use crate::mapping::{build_message, ProtoField, ToProtoType};

// ---------------------------------------------------------------------------
// RPC method descriptor
// ---------------------------------------------------------------------------

/// An RPC method extracted from an endpoint type.
#[derive(Debug, Clone)]
pub struct RpcMethod {
    /// PascalCase RPC name (e.g., `"GetUser"`, `"ListUsers"`).
    pub name: String,
    /// HTTP method string (e.g., `"GET"`, `"POST"`).
    pub http_method: String,
    /// Path pattern (e.g., `"/users/{}"`, `"/users/{}/posts"`).
    pub path_pattern: String,
    /// Request message, or `None` if the RPC takes `google.protobuf.Empty`.
    pub request_message: Option<ProtoMessage>,
    /// Response message definition.
    pub response_message: ProtoMessage,
    /// Whether this RPC uses server-side streaming for the response.
    pub server_streaming: bool,
}

/// A protobuf message definition paired with its name.
#[derive(Debug, Clone)]
pub struct ProtoMessage {
    /// Message type name (e.g., `"GetUserRequest"`).
    pub name: String,
    /// Full message definition text (e.g., `"message GetUserRequest { ... }"`).
    pub definition: String,
}

// ---------------------------------------------------------------------------
// EndpointToRpc trait
// ---------------------------------------------------------------------------

/// Convert a single endpoint type to an [`RpcMethod`] descriptor.
pub trait EndpointToRpc {
    /// Produce the RPC method descriptor for this endpoint.
    fn to_rpc() -> RpcMethod;
}

// Bodyless endpoints (GET, DELETE, HEAD, OPTIONS)
impl<M, P, Res, Q, Err> EndpointToRpc for Endpoint<M, P, NoBody, Res, Q, Err>
where
    M: HttpMethod,
    P: PathSpec + ExtractPath,
    Res: ToProtoType,
{
    fn to_rpc() -> RpcMethod {
        let method_str = format!("{}", M::METHOD);
        let path = P::pattern();
        let rpc_name = path_to_rpc_name(&method_str, &path);

        // Build request message from path captures.
        let captures = captures_from_pattern(&path);
        let request_message = if captures.is_empty() {
            None
        } else {
            let req_name = format!("{}Request", rpc_name);
            Some(ProtoMessage {
                name: req_name.clone(),
                definition: build_message(&req_name, &captures),
            })
        };

        // Response message.
        let response_message = build_response_message::<Res>(&rpc_name);

        RpcMethod {
            name: rpc_name,
            http_method: method_str,
            path_pattern: path,
            request_message,
            response_message,
            server_streaming: false,
        }
    }
}

// Body endpoints (POST, PUT, PATCH) — separate impls per method to avoid
// overlapping with the NoBody blanket impl above.
macro_rules! impl_endpoint_to_rpc_with_body {
    ($Method:ty) => {
        impl<P, Req, Res, Q, Err> EndpointToRpc for Endpoint<$Method, P, Req, Res, Q, Err>
        where
            P: PathSpec + ExtractPath,
            Req: ToProtoType,
            Res: ToProtoType,
        {
            fn to_rpc() -> RpcMethod {
                let method_str = format!("{}", <$Method as HttpMethod>::METHOD);
                let path = P::pattern();
                let rpc_name = path_to_rpc_name(&method_str, &path);

                // Request = capture fields + body reference.
                let mut fields = captures_from_pattern(&path);
                let body_tag = fields.len() as u32 + 1;

                let body_type_name = Req::proto_type_name();
                if body_type_name != "google.protobuf.Empty" {
                    fields.push(ProtoField {
                        name: "body".to_string(),
                        proto_type: body_type_name.to_string(),
                        tag: body_tag,
                        repeated: Req::is_repeated(),
                        optional: false,
                    });
                }

                let req_name = format!("{}Request", rpc_name);
                let request_message = Some(ProtoMessage {
                    name: req_name.clone(),
                    definition: build_message(&req_name, &fields),
                });

                let response_message = build_response_message::<Res>(&rpc_name);

                RpcMethod {
                    name: rpc_name,
                    http_method: method_str,
                    path_pattern: path,
                    request_message,
                    response_message,
                    server_streaming: false,
                }
            }
        }
    };
}

impl_endpoint_to_rpc_with_body!(Post);
impl_endpoint_to_rpc_with_body!(Put);
impl_endpoint_to_rpc_with_body!(Patch);

// ---------------------------------------------------------------------------
// Wrapper type delegation
// ---------------------------------------------------------------------------

/// `Requires<E, Inner>` delegates to the inner endpoint.
impl<Eff: Effect, Inner: EndpointToRpc> EndpointToRpc for Requires<Eff, Inner> {
    fn to_rpc() -> RpcMethod {
        Inner::to_rpc()
    }
}

/// `Deprecated<Inner>` delegates to the inner endpoint.
impl<Inner: EndpointToRpc> EndpointToRpc for typeway_core::versioning::Deprecated<Inner> {
    fn to_rpc() -> RpcMethod {
        Inner::to_rpc()
    }
}

/// `ServerStream<E>` delegates to the inner endpoint but marks it as
/// server-streaming.
impl<E: EndpointToRpc> EndpointToRpc for crate::streaming::ServerStream<E> {
    fn to_rpc() -> RpcMethod {
        let mut rpc = E::to_rpc();
        rpc.server_streaming = true;
        rpc
    }
}

// ---------------------------------------------------------------------------
// CollectRpcs trait
// ---------------------------------------------------------------------------

// Forward declaration; the trait and blanket impl are below. Wrapper impls
// that need `CollectRpcs` (not just `EndpointToRpc`) are placed after the
// trait definition.

/// Collect RPC method descriptors from an endpoint or tuple of endpoints.
pub trait CollectRpcs {
    /// Collect all RPC methods into a `Vec`.
    fn collect_rpcs() -> Vec<RpcMethod>;
}

impl<E: EndpointToRpc> CollectRpcs for E {
    fn collect_rpcs() -> Vec<RpcMethod> {
        vec![E::to_rpc()]
    }
}

macro_rules! impl_collect_rpcs_for_tuple {
    ($($T:ident),+) => {
        impl<$($T: CollectRpcs,)+> CollectRpcs for ($($T,)+) {
            fn collect_rpcs() -> Vec<RpcMethod> {
                let mut rpcs = Vec::new();
                $(rpcs.extend($T::collect_rpcs());)+
                rpcs
            }
        }
    };
}

impl_collect_rpcs_for_tuple!(A);
impl_collect_rpcs_for_tuple!(A, B);
impl_collect_rpcs_for_tuple!(A, B, C);
impl_collect_rpcs_for_tuple!(A, B, C, D);
impl_collect_rpcs_for_tuple!(A, B, C, D, E);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U);
impl_collect_rpcs_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V);

// ---------------------------------------------------------------------------
// CollectRpcs for wrapper types that contain full API tuples
// ---------------------------------------------------------------------------

/// `VersionedApi<B, C, R>` delegates RPC collection to the resolved API type.
impl<B, C, R> CollectRpcs for typeway_core::versioning::VersionedApi<B, C, R>
where
    R: typeway_core::ApiSpec + CollectRpcs,
{
    fn collect_rpcs() -> Vec<RpcMethod> {
        R::collect_rpcs()
    }
}

// ---------------------------------------------------------------------------
// ApiToProto trait
// ---------------------------------------------------------------------------

/// Generate a complete `.proto` file from an API type.
///
/// # Example
///
/// ```ignore
/// let proto = MyAPI::to_proto("UserService", "users.v1");
/// std::fs::write("service.proto", proto).unwrap();
/// ```
pub trait ApiToProto: CollectRpcs {
    /// Generate a `.proto` file string for this API.
    ///
    /// - `service_name`: the gRPC service name (PascalCase)
    /// - `package`: the proto package name (e.g., `"myapp.v1"`)
    fn to_proto(service_name: &str, package: &str) -> String {
        let rpcs = Self::collect_rpcs();
        generate_proto_file(service_name, package, &rpcs)
    }
}

/// Blanket impl: anything that can collect RPCs can generate a `.proto` file.
impl<T: CollectRpcs> ApiToProto for T {}

// ---------------------------------------------------------------------------
// Proto file generation
// ---------------------------------------------------------------------------

/// Generate the full `.proto` file text from a service name, package, and RPC methods.
fn generate_proto_file(service_name: &str, package: &str, rpcs: &[RpcMethod]) -> String {
    let mut lines = vec![
        "syntax = \"proto3\";".to_string(),
        String::new(),
        format!("package {};", package),
        String::new(),
    ];

    // Collect all message definitions (deduplicated by content, preserving order).
    let mut messages: IndexSet<String> = IndexSet::new();
    for rpc in rpcs {
        if let Some(ref req) = rpc.request_message {
            messages.insert(req.definition.clone());
        }
        messages.insert(rpc.response_message.definition.clone());
    }

    // Service definition.
    lines.push(format!("service {} {{", service_name));
    for rpc in rpcs {
        let req_type = rpc
            .request_message
            .as_ref()
            .map(|m| m.name.as_str())
            .unwrap_or("google.protobuf.Empty");
        let res_type = &rpc.response_message.name;
        let stream_prefix = if rpc.server_streaming { "stream " } else { "" };
        lines.push(format!("  // {} {}", rpc.http_method, rpc.path_pattern));
        lines.push(format!(
            "  rpc {}({}) returns ({}{});",
            rpc.name, req_type, stream_prefix, res_type
        ));
    }
    lines.push("}".to_string());
    lines.push(String::new());

    // Message definitions.
    for msg in &messages {
        lines.push(msg.clone());
        lines.push(String::new());
    }

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Extract capture fields from a path pattern string.
///
/// The pattern uses `{}` for unnamed captures (from `ExtractPath::pattern()`).
/// Fields are named `param1`, `param2`, etc. and default to `string` type.
fn captures_from_pattern(pattern: &str) -> Vec<ProtoField> {
    pattern
        .split('/')
        .filter(|s| *s == "{}" || (s.starts_with('{') && s.ends_with('}')))
        .enumerate()
        .map(|(i, seg)| {
            let name = if seg == "{}" {
                format!("param{}", i + 1)
            } else {
                // Named capture like {id} — strip braces.
                seg[1..seg.len() - 1].to_string()
            };
            ProtoField {
                name,
                proto_type: "string".to_string(),
                tag: (i + 1) as u32,
                repeated: false,
                optional: false,
            }
        })
        .collect()
}

/// Build a response message for a given response type.
fn build_response_message<Res: ToProtoType>(rpc_name: &str) -> ProtoMessage {
    let type_name = Res::proto_type_name();

    if Res::is_message() {
        // The response is a user-defined message type — use it directly.
        ProtoMessage {
            name: type_name.to_string(),
            definition: Res::message_definition().unwrap_or_else(|| {
                // Fallback: empty message definition if none provided.
                build_message(type_name, &[])
            }),
        }
    } else if type_name == "google.protobuf.Empty" {
        // Unit type maps to Empty.
        ProtoMessage {
            name: "google.protobuf.Empty".to_string(),
            definition: String::new(),
        }
    } else {
        // Primitive response — wrap in a response message.
        let res_msg_name = format!("{}Response", rpc_name);
        ProtoMessage {
            name: res_msg_name.clone(),
            definition: build_message(
                &res_msg_name,
                &[ProtoField {
                    name: "value".to_string(),
                    proto_type: type_name.to_string(),
                    tag: 1,
                    repeated: Res::is_repeated(),
                    optional: false,
                }],
            ),
        }
    }
}

/// Derive an RPC method name from the HTTP method and path pattern.
///
/// Examples:
/// - `GET /users` -> `ListUser`
/// - `GET /users/{}` -> `GetUser`
/// - `POST /users` -> `CreateUser`
/// - `PUT /users/{}` -> `UpdateUser`
/// - `DELETE /users/{}` -> `DeleteUser`
/// - `GET /users/{}/posts` -> `ListPost`
fn path_to_rpc_name(method: &str, path: &str) -> String {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .collect();

    let resource = segments
        .last()
        .map(|s| to_pascal_case_singular(s))
        .unwrap_or_else(|| "Resource".to_string());

    // For GET, check if the last segment is a capture — that signals a
    // single-resource fetch ("Get") vs a collection fetch ("List").
    let last_segment = path.split('/').rfind(|s| !s.is_empty());
    let last_is_capture = last_segment
        .is_some_and(|s| s.starts_with('{'));

    let prefix = match method {
        "GET" => {
            if last_is_capture {
                "Get"
            } else {
                "List"
            }
        }
        "POST" => "Create",
        "PUT" => "Update",
        "PATCH" => "Update",
        "DELETE" => "Delete",
        "HEAD" => "Head",
        "OPTIONS" => "Options",
        _ => "Call",
    };

    format!("{}{}", prefix, resource)
}

/// Convert a snake_case string to PascalCase and naively singularize.
///
/// Singularization strips a trailing 's' if present (handles simple English
/// plurals like "users" -> "User", "posts" -> "Post"). This is intentionally
/// simple — users can override RPC names for complex cases.
fn to_pascal_case_singular(s: &str) -> String {
    let pascal: String = s
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.collect::<String>()
                }
                None => String::new(),
            }
        })
        .collect();

    // Naive singularization: strip trailing 's' if the word is longer than 1 char.
    if pascal.ends_with('s') && pascal.len() > 1 {
        pascal[..pascal.len() - 1].to_string()
    } else {
        pascal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_name_get_collection() {
        assert_eq!(path_to_rpc_name("GET", "/users"), "ListUser");
    }

    #[test]
    fn rpc_name_get_single() {
        assert_eq!(path_to_rpc_name("GET", "/users/{}"), "GetUser");
    }

    #[test]
    fn rpc_name_post() {
        assert_eq!(path_to_rpc_name("POST", "/users"), "CreateUser");
    }

    #[test]
    fn rpc_name_put() {
        assert_eq!(path_to_rpc_name("PUT", "/users/{}"), "UpdateUser");
    }

    #[test]
    fn rpc_name_delete() {
        assert_eq!(path_to_rpc_name("DELETE", "/users/{}"), "DeleteUser");
    }

    #[test]
    fn rpc_name_nested_path() {
        assert_eq!(path_to_rpc_name("GET", "/users/{}/posts"), "ListPost");
    }

    #[test]
    fn rpc_name_nested_path_with_capture() {
        assert_eq!(path_to_rpc_name("GET", "/users/{}/posts/{}"), "GetPost");
    }

    #[test]
    fn captures_unnamed() {
        let fields = captures_from_pattern("/users/{}/posts/{}");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "param1");
        assert_eq!(fields[0].tag, 1);
        assert_eq!(fields[1].name, "param2");
        assert_eq!(fields[1].tag, 2);
    }

    #[test]
    fn captures_none() {
        let fields = captures_from_pattern("/users");
        assert!(fields.is_empty());
    }

    #[test]
    fn captures_named() {
        let fields = captures_from_pattern("/users/{id}/posts/{post_id}");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "id");
        assert_eq!(fields[1].name, "post_id");
    }

    #[test]
    fn pascal_case_singular() {
        assert_eq!(to_pascal_case_singular("users"), "User");
        assert_eq!(to_pascal_case_singular("posts"), "Post");
        assert_eq!(to_pascal_case_singular("status"), "Statu");
        assert_eq!(to_pascal_case_singular("data"), "Data");
        assert_eq!(to_pascal_case_singular("user_profiles"), "UserProfile");
    }
}
