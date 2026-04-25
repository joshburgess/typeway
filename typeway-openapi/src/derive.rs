//! Traits for deriving OpenAPI specs from Wayward API types.
//!
//! [`EndpointToOperation`] converts a single endpoint to an OpenAPI operation.
//! [`ApiToSpec`] converts an entire API type (tuple of endpoints) to a full spec.

use indexmap::IndexMap;

use typeway_core::*;
use typeway_core::effects::{Effect, Requires};

use crate::spec::*;

// ---------------------------------------------------------------------------
// Schema derivation for Rust types
// ---------------------------------------------------------------------------

/// Derive a JSON Schema representation for a Rust type.
///
/// This is a simplified alternative to `schemars` — it covers common types
/// without adding a heavy dependency. For production use with complex types,
/// integrate `schemars` behind a feature flag.
pub trait ToSchema {
    fn schema() -> Schema;
    fn type_name() -> &'static str;

    /// Return an example JSON value for this type.
    ///
    /// Override this to include response examples in the generated OpenAPI spec.
    /// The default returns `None` (no example).
    fn example() -> Option<serde_json::Value> {
        None
    }
}

/// Provide example response values for OpenAPI spec generation.
///
/// Implement this trait on types that should include example values in the
/// generated OpenAPI spec. The framework calls [`ExampleValue::example()`],
/// serializes the result, and includes it in the MediaType's `example` field.
///
/// # Example
///
/// ```ignore
/// use typeway_openapi::ExampleValue;
///
/// #[derive(Serialize)]
/// struct User { id: u32, name: String }
///
/// impl ExampleValue for User {
///     fn example() -> Self {
///         User { id: 1, name: "Alice".to_string() }
///     }
/// }
/// ```
pub trait ExampleValue: serde::Serialize {
    /// Return an example instance of this type.
    fn example() -> Self;

    /// Serialize the example to a JSON value.
    fn example_json() -> Option<serde_json::Value>
    where
        Self: Sized,
    {
        serde_json::to_value(Self::example()).ok()
    }
}

impl ToSchema for String {
    fn schema() -> Schema {
        Schema::string()
    }
    fn type_name() -> &'static str {
        "string"
    }
}

impl ToSchema for &str {
    fn schema() -> Schema {
        Schema::string()
    }
    fn type_name() -> &'static str {
        "string"
    }
}

impl ToSchema for u8 {
    fn schema() -> Schema {
        Schema::integer()
    }
    fn type_name() -> &'static str {
        "u8"
    }
}

impl ToSchema for u16 {
    fn schema() -> Schema {
        Schema::integer()
    }
    fn type_name() -> &'static str {
        "u16"
    }
}

impl ToSchema for u32 {
    fn schema() -> Schema {
        Schema::integer()
    }
    fn type_name() -> &'static str {
        "u32"
    }
}

impl ToSchema for u64 {
    fn schema() -> Schema {
        Schema::integer64()
    }
    fn type_name() -> &'static str {
        "u64"
    }
}

impl ToSchema for i32 {
    fn schema() -> Schema {
        Schema::integer()
    }
    fn type_name() -> &'static str {
        "i32"
    }
}

impl ToSchema for i64 {
    fn schema() -> Schema {
        Schema::integer64()
    }
    fn type_name() -> &'static str {
        "i64"
    }
}

impl ToSchema for f32 {
    fn schema() -> Schema {
        Schema::number()
    }
    fn type_name() -> &'static str {
        "f32"
    }
}

impl ToSchema for f64 {
    fn schema() -> Schema {
        Schema::number()
    }
    fn type_name() -> &'static str {
        "f64"
    }
}

impl ToSchema for bool {
    fn schema() -> Schema {
        Schema::boolean()
    }
    fn type_name() -> &'static str {
        "bool"
    }
}

impl<T: ToSchema> ToSchema for Vec<T> {
    fn schema() -> Schema {
        Schema::array(T::schema())
    }
    fn type_name() -> &'static str {
        "array"
    }
}

impl ToSchema for () {
    fn schema() -> Schema {
        Schema::object()
    }
    fn type_name() -> &'static str {
        "()"
    }
}

impl ToSchema for serde_json::Value {
    fn schema() -> Schema {
        Schema::object()
    }
    fn type_name() -> &'static str {
        "Value"
    }
}

impl ToSchema for http::StatusCode {
    fn schema() -> Schema {
        Schema::integer()
    }
    fn type_name() -> &'static str {
        "StatusCode"
    }
}

impl<T: ToSchema> ToSchema for Option<T> {
    fn schema() -> Schema {
        T::schema()
    }
    fn type_name() -> &'static str {
        T::type_name()
    }
}

impl<T: ToSchema> ToSchema for Box<T> {
    fn schema() -> Schema {
        T::schema()
    }
    fn type_name() -> &'static str {
        T::type_name()
    }
}

impl<T: ToSchema> ToSchema for std::sync::Arc<T> {
    fn schema() -> Schema {
        T::schema()
    }
    fn type_name() -> &'static str {
        T::type_name()
    }
}

impl<K: ToSchema, V: ToSchema> ToSchema for std::collections::HashMap<K, V> {
    fn schema() -> Schema {
        Schema::object()
    }
    fn type_name() -> &'static str {
        "object"
    }
}

impl<K: ToSchema, V: ToSchema> ToSchema for indexmap::IndexMap<K, V> {
    fn schema() -> Schema {
        Schema::object()
    }
    fn type_name() -> &'static str {
        "object"
    }
}

// ---------------------------------------------------------------------------
// schemars bridge (feature = "schemars")
// ---------------------------------------------------------------------------

/// Convert a `schemars::schema::Schema` to our simplified `Schema`.
///
/// Available with `feature = "schemars"`. Use this to implement `ToSchema`
/// for types that derive `schemars::JsonSchema`:
///
/// ```ignore
/// use schemars::JsonSchema;
/// use typeway_openapi::{ToSchema, from_schemars};
///
/// #[derive(JsonSchema)]
/// struct User { id: u32, name: String }
///
/// impl ToSchema for User {
///     fn schema() -> typeway_openapi::spec::Schema { from_schemars::<Self>() }
///     fn type_name() -> &'static str { "User" }
/// }
/// ```
#[cfg(feature = "schemars")]
pub fn from_schemars<T: schemars::JsonSchema>() -> Schema {
    let root = schemars::schema_for!(T);
    convert_schemars_schema(&serde_json::to_value(root.schema).unwrap_or_default())
}

#[cfg(feature = "schemars")]
fn convert_schemars_schema(value: &serde_json::Value) -> Schema {
    let schema_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let format = value
        .get("format")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let items = value
        .get("items")
        .map(|v| Box::new(convert_schemars_schema(v)));
    let properties = value
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), convert_schemars_schema(v)))
                .collect()
        });
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Schema {
        schema_type,
        format,
        items,
        properties,
        description,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Error response schema
// ---------------------------------------------------------------------------

/// Generate OpenAPI error response entries for an endpoint's error type.
///
/// Implemented for `()` (no error schema). Implement this trait on your
/// error type to have error schemas appear in the OpenAPI spec.
///
/// ```ignore
/// impl ErrorResponses for JsonError {
///     fn error_responses() -> IndexMap<String, Response> {
///         // Generate 4xx error schema
///     }
/// }
///
/// // Then use in endpoint type:
/// type API = (
///     GetEndpoint<UsersPath, Json<Vec<User>>, (), JsonError>,
/// );
/// ```
pub trait ErrorResponses {
    fn error_responses() -> IndexMap<String, Response>;
}

impl ErrorResponses for () {
    fn error_responses() -> IndexMap<String, Response> {
        IndexMap::new()
    }
}

// ---------------------------------------------------------------------------
// Path parameter extraction
// ---------------------------------------------------------------------------

/// Extract OpenAPI path parameters from a path spec.
pub trait PathParameters {
    fn parameters() -> Vec<Parameter>;
}

impl PathParameters for HNil {
    fn parameters() -> Vec<Parameter> {
        Vec::new()
    }
}

impl<S: LitSegment, T: PathParameters> PathParameters for HCons<Lit<S>, T> {
    fn parameters() -> Vec<Parameter> {
        T::parameters()
    }
}

impl<U: ToSchema, T: PathParameters> PathParameters for HCons<Capture<U>, T> {
    fn parameters() -> Vec<Parameter> {
        let mut params = vec![Parameter {
            name: format!("param{}", T::parameters().len()),
            location: ParameterLocation::Path,
            required: true,
            schema: Some(U::schema()),
        }];
        params.extend(T::parameters());
        params
    }
}

// ---------------------------------------------------------------------------
// EndpointDoc — optional metadata for OpenAPI operations
// ---------------------------------------------------------------------------

/// Optional documentation metadata for an endpoint.
///
/// Implement this trait on your endpoint type alias to add summary,
/// description, tags, and operation ID to the generated OpenAPI spec.
///
/// # Example
///
/// ```
/// use typeway_openapi::EndpointDoc;
///
/// struct GetUsersEndpoint;
///
/// impl EndpointDoc for GetUsersEndpoint {
///     fn summary() -> Option<&'static str> { Some("List all users") }
///     fn description() -> Option<&'static str> { Some("Returns a paginated list of users") }
///     fn tags() -> Vec<&'static str> { vec!["users"] }
///     fn operation_id() -> Option<&'static str> { Some("listUsers") }
/// }
/// ```
pub trait EndpointDoc {
    fn summary() -> Option<&'static str> {
        None
    }
    fn description() -> Option<&'static str> {
        None
    }
    fn tags() -> Vec<&'static str> {
        Vec::new()
    }
    fn operation_id() -> Option<&'static str> {
        None
    }
}

/// Blanket impl: all endpoints have no documentation by default.
impl<M: HttpMethod, P: PathSpec, Req, Res, Q, Err> EndpointDoc
    for Endpoint<M, P, Req, Res, Q, Err>
{
}

// ---------------------------------------------------------------------------
// QueryParameters — extract query param schema for OpenAPI
// ---------------------------------------------------------------------------

/// Extract OpenAPI query parameters from a query type.
///
/// Implement this for your query parameter struct to have its fields
/// appear in the OpenAPI spec. A blanket impl is provided for `()`
/// (no query params).
pub trait QueryParameters {
    fn query_parameters() -> Vec<Parameter>;
}

impl QueryParameters for () {
    fn query_parameters() -> Vec<Parameter> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// EndpointToOperation
// ---------------------------------------------------------------------------

/// Convert a single endpoint type to an OpenAPI operation + path pattern.
pub trait EndpointToOperation {
    fn path_pattern() -> String;
    fn method() -> http::Method;
    fn to_operation() -> Operation;
}

// Bodyless endpoints (NoBody request)
impl<M, P, Res, Q, Err> EndpointToOperation for Endpoint<M, P, NoBody, Res, Q, Err>
where
    M: HttpMethod,
    P: PathSpec + ExtractPath + PathParameters,
    Res: ToSchema,
    Q: QueryParameters,
    Err: ErrorResponses,
{
    fn path_pattern() -> String {
        P::pattern()
    }

    fn method() -> http::Method {
        M::METHOD
    }

    fn to_operation() -> Operation {
        let mut op = Operation::new();
        op.parameters = P::parameters();
        assign_param_names_from_pattern(&mut op.parameters, &P::pattern());
        op.parameters.extend(Q::query_parameters());

        // Apply documentation metadata if available.
        op.summary = <Self as EndpointDoc>::summary().map(|s| s.to_string());
        op.description = <Self as EndpointDoc>::description().map(|s| s.to_string());
        op.operation_id = <Self as EndpointDoc>::operation_id().map(|s| s.to_string());
        op.tags = <Self as EndpointDoc>::tags()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        let mut responses = IndexMap::new();
        let mut content = IndexMap::new();
        content.insert(
            "application/json".to_string(),
            MediaType {
                schema: Some(Res::schema()),
                example: Res::example(),
            },
        );
        responses.insert(
            "200".to_string(),
            Response {
                description: "Successful response".to_string(),
                content,
            },
        );
        // Merge error responses from the Err type.
        responses.extend(Err::error_responses());
        op.responses = responses;
        op
    }
}

// Body endpoints — we need separate impls per method to avoid overlap with NoBody.
macro_rules! impl_endpoint_to_operation_with_body {
    ($Method:ty) => {
        impl<P, Req, Res, Q, Err> EndpointToOperation for Endpoint<$Method, P, Req, Res, Q, Err>
        where
            P: PathSpec + ExtractPath + PathParameters,
            Req: ToSchema,
            Res: ToSchema,
            Q: QueryParameters,
            Err: ErrorResponses,
        {
            fn path_pattern() -> String {
                P::pattern()
            }

            fn method() -> http::Method {
                <$Method as HttpMethod>::METHOD
            }

            fn to_operation() -> Operation {
                let mut op = Operation::new();
                op.parameters = P::parameters();
                assign_param_names_from_pattern(&mut op.parameters, &P::pattern());
                op.parameters.extend(Q::query_parameters());

                // Apply documentation metadata.
                op.summary = <Endpoint<$Method, P, Req, Res, Q, Err> as EndpointDoc>::summary()
                    .map(|s| s.to_string());
                op.description =
                    <Endpoint<$Method, P, Req, Res, Q, Err> as EndpointDoc>::description()
                        .map(|s| s.to_string());
                op.operation_id =
                    <Endpoint<$Method, P, Req, Res, Q, Err> as EndpointDoc>::operation_id()
                        .map(|s| s.to_string());
                op.tags = <Endpoint<$Method, P, Req, Res, Q, Err> as EndpointDoc>::tags()
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect();

                // Request body
                let mut req_content = IndexMap::new();
                req_content.insert(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Req::schema()),
                        example: Req::example(),
                    },
                );
                op.request_body = Some(RequestBody {
                    required: true,
                    content: req_content,
                });

                // Response
                let mut res_content = IndexMap::new();
                res_content.insert(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Res::schema()),
                        example: Res::example(),
                    },
                );
                let mut responses = IndexMap::new();
                responses.insert(
                    "200".to_string(),
                    Response {
                        description: "Successful response".to_string(),
                        content: res_content,
                    },
                );
                responses.extend(Err::error_responses());
                op.responses = responses;
                op
            }
        }
    };
}

impl_endpoint_to_operation_with_body!(Post);
impl_endpoint_to_operation_with_body!(Put);
impl_endpoint_to_operation_with_body!(Patch);

/// Assign parameter names from `{name}` placeholders in the pattern.
fn assign_param_names_from_pattern(params: &mut [Parameter], pattern: &str) {
    let names: Vec<&str> = pattern
        .split('/')
        .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
        .map(|seg| &seg[1..seg.len() - 1])
        .collect();

    for (i, param) in params.iter_mut().enumerate() {
        if let Some(name) = names.get(i) {
            if !name.is_empty() {
                param.name = name.to_string();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// EndpointToOperation / CollectOperations for wrapper types (typeway-core)
// ---------------------------------------------------------------------------

/// `Requires<E, T>` delegates to the inner endpoint, adding no OpenAPI annotations.
///
/// Effect requirements are internal implementation details and do not affect
/// the generated OpenAPI spec.
impl<Eff: Effect, Inner: EndpointToOperation> EndpointToOperation for Requires<Eff, Inner> {
    fn path_pattern() -> String {
        Inner::path_pattern()
    }
    fn method() -> http::Method {
        Inner::method()
    }
    fn to_operation() -> Operation {
        Inner::to_operation()
    }
}

/// `Deprecated<E>` delegates to the inner endpoint but marks the operation
/// as deprecated in the OpenAPI spec.
impl<Inner: EndpointToOperation> EndpointToOperation
    for typeway_core::versioning::Deprecated<Inner>
{
    fn path_pattern() -> String {
        Inner::path_pattern()
    }
    fn method() -> http::Method {
        Inner::method()
    }
    fn to_operation() -> Operation {
        let mut op = Inner::to_operation();
        op.deprecated = true;
        op
    }
}

// ---------------------------------------------------------------------------
// Auto-tagging by path prefix
// ---------------------------------------------------------------------------

/// Extract a tag name from a path pattern.
///
/// Uses the first non-empty, non-parameter segment as the tag. For example:
/// - `/users` -> `"users"`
/// - `/api/articles` -> `"api"` (first segment)
/// - `/users/{id}/posts` -> `"users"`
/// - `/{param}` -> `None`
fn extract_tag_from_path(path: &str) -> Option<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .find(|s| !s.starts_with('{'))
        .map(|s| s.to_string())
}

/// Post-process a spec to auto-assign tags based on path prefix.
///
/// Operations that already have tags (set via [`EndpointDoc`]) are not modified.
/// Operations without tags get a tag derived from the first literal path segment.
pub fn auto_tag_operations(spec: &mut OpenApiSpec) {
    for (path, item) in &mut spec.paths {
        let tag = extract_tag_from_path(path);
        for op in item.all_operations_mut() {
            if op.tags.is_empty() {
                if let Some(ref tag) = tag {
                    op.tags.push(tag.clone());
                }
            }
        }
    }
}

/// Post-process a spec to collect security schemes from operations.
///
/// If any operation references `bearerAuth`, adds a bearer JWT security
/// scheme to the spec's components section.
pub fn collect_security_schemes(spec: &mut OpenApiSpec) {
    let has_bearer = spec.paths.values().any(|item| {
        let ops: Vec<&Operation> = [
            item.get.as_ref(),
            item.post.as_ref(),
            item.put.as_ref(),
            item.delete.as_ref(),
            item.patch.as_ref(),
            item.head.as_ref(),
            item.options.as_ref(),
        ]
        .into_iter()
        .flatten()
        .collect();

        ops.iter().any(|op| {
            op.security.iter().any(|req| req.0.contains_key("bearerAuth"))
        })
    });

    if has_bearer {
        let components = spec.components.get_or_insert_with(|| Components {
            security_schemes: IndexMap::new(),
        });
        components
            .security_schemes
            .entry("bearerAuth".to_string())
            .or_insert_with(SecurityScheme::bearer_jwt);
    }
}

// ---------------------------------------------------------------------------
// ApiToSpec
// ---------------------------------------------------------------------------

/// Convert an entire API type to an OpenAPI spec.
pub trait ApiToSpec {
    fn to_spec(title: &str, version: &str) -> OpenApiSpec;
}

/// Helper: collect operations from a single endpoint or tuple of endpoints.
pub trait CollectOperations {
    fn collect_into(spec: &mut OpenApiSpec);
}

impl<E: EndpointToOperation> CollectOperations for E {
    fn collect_into(spec: &mut OpenApiSpec) {
        let pattern = E::path_pattern();
        let method = E::method();
        let op = E::to_operation();

        let path_item = spec.paths.entry(pattern).or_default();
        path_item.set_operation(&method, op);
    }
}

macro_rules! impl_collect_for_tuple {
    ($($T:ident),+) => {
        impl<$($T: CollectOperations,)+> CollectOperations for ($($T,)+) {
            fn collect_into(spec: &mut OpenApiSpec) {
                $($T::collect_into(spec);)+
            }
        }

        impl<$($T: CollectOperations,)+> ApiToSpec for ($($T,)+) {
            fn to_spec(title: &str, version: &str) -> OpenApiSpec {
                let mut spec = OpenApiSpec::new(title, version);
                $($T::collect_into(&mut spec);)+
                auto_tag_operations(&mut spec);
                collect_security_schemes(&mut spec);
                spec
            }
        }
    };
}

impl_collect_for_tuple!(A);
impl_collect_for_tuple!(A, B);
impl_collect_for_tuple!(A, B, C);
impl_collect_for_tuple!(A, B, C, D);
impl_collect_for_tuple!(A, B, C, D, E);
impl_collect_for_tuple!(A, B, C, D, E, F);
impl_collect_for_tuple!(A, B, C, D, E, F, G);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_collect_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

// ---------------------------------------------------------------------------
// Handler doc application
// ---------------------------------------------------------------------------

/// Patch an OpenAPI spec with handler documentation metadata.
///
/// For each [`HandlerDoc`](typeway_core::HandlerDoc) in the slice, finds the
/// operation whose `operation_id` matches (or whose method+path combination
/// can be matched) and sets its `summary`, `description`, `operation_id`, and
/// `tags` fields. Existing values are overwritten.
///
/// Operations not matched by any doc entry are left unchanged.
pub fn apply_handler_docs(spec: &mut OpenApiSpec, docs: &[typeway_core::HandlerDoc]) {
    for doc in docs {
        // First pass: try to match by existing operation_id.
        let mut matched = false;
        for (_path, item) in spec.paths.iter_mut() {
            for op in item.all_operations_mut() {
                if op.operation_id.as_deref() == Some(doc.operation_id) {
                    apply_doc_to_operation(op, doc);
                    matched = true;
                    break;
                }
            }
            if matched {
                break;
            }
        }

        // Second pass: if no operation_id match, assign to the first operation
        // that has no summary and no operation_id. This handles the common case
        // where operations don't yet have operation_ids set.
        if !matched {
            for (_path, item) in spec.paths.iter_mut() {
                for op in item.all_operations_mut() {
                    if op.operation_id.is_none() && op.summary.is_none() {
                        apply_doc_to_operation(op, doc);
                        matched = true;
                        break;
                    }
                }
                if matched {
                    break;
                }
            }
        }
    }
}

fn apply_doc_to_operation(op: &mut Operation, doc: &typeway_core::HandlerDoc) {
    if !doc.summary.is_empty() {
        op.summary = Some(doc.summary.to_string());
    }
    if !doc.description.is_empty() {
        op.description = Some(doc.description.to_string());
    }
    op.operation_id = Some(doc.operation_id.to_string());
    if !doc.tags.is_empty() {
        op.tags = doc.tags.iter().map(|s| s.to_string()).collect();
    }
}
