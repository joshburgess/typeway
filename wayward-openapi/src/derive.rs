//! Traits for deriving OpenAPI specs from Wayward API types.
//!
//! [`EndpointToOperation`] converts a single endpoint to an OpenAPI operation.
//! [`ApiToSpec`] converts an entire API type (tuple of endpoints) to a full spec.

use indexmap::IndexMap;

use wayward_core::*;

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
/// use wayward_openapi::{ToSchema, from_schemars};
///
/// #[derive(JsonSchema)]
/// struct User { id: u32, name: String }
///
/// impl ToSchema for User {
///     fn schema() -> wayward_openapi::spec::Schema { from_schemars::<Self>() }
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
// EndpointToOperation
// ---------------------------------------------------------------------------

/// Convert a single endpoint type to an OpenAPI operation + path pattern.
pub trait EndpointToOperation {
    fn path_pattern() -> String;
    fn method() -> http::Method;
    fn to_operation() -> Operation;
}

// Bodyless endpoints (NoBody request)
impl<M, P, Res> EndpointToOperation for Endpoint<M, P, NoBody, Res>
where
    M: HttpMethod,
    P: PathSpec + ExtractPath + PathParameters,
    Res: ToSchema,
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

        // Add path parameter names from the pattern.
        assign_param_names_from_pattern(&mut op.parameters, &P::pattern());

        let mut responses = IndexMap::new();
        let mut content = IndexMap::new();
        content.insert(
            "application/json".to_string(),
            MediaType {
                schema: Some(Res::schema()),
            },
        );
        responses.insert(
            "200".to_string(),
            Response {
                description: "Successful response".to_string(),
                content,
            },
        );
        op.responses = responses;
        op
    }
}

// Body endpoints — we need separate impls per method to avoid overlap with NoBody.
macro_rules! impl_endpoint_to_operation_with_body {
    ($Method:ty) => {
        impl<P, Req, Res> EndpointToOperation for Endpoint<$Method, P, Req, Res>
        where
            P: PathSpec + ExtractPath + PathParameters,
            Req: ToSchema,
            Res: ToSchema,
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

                // Request body
                let mut req_content = IndexMap::new();
                req_content.insert(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Req::schema()),
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
