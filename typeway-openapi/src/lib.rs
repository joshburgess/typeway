//! `typeway-openapi` — OpenAPI spec generation and codegen.
//!
//! Two directions:
//!
//! - **Rust → OpenAPI**: Walk an API type and produce an OpenAPI spec
//!   (via [`ApiToSpec`]).
//! - **OpenAPI → Rust**: Parse an OpenAPI spec (2.x or 3.x) and generate
//!   typeway Rust code (via [`codegen_v2`] and [`codegen_v3`]).

pub mod codegen_common;
#[allow(dead_code)]
pub mod codegen_v2;
#[allow(dead_code)]
pub mod codegen_v3;
pub mod derive;
pub mod spec;
pub mod swagger_output;

pub use codegen_v2::swagger_to_typeway;
pub use codegen_v3::openapi3_to_typeway;
#[cfg(feature = "schemars")]
pub use derive::from_schemars;
pub use derive::{
    apply_handler_docs, auto_tag_operations, collect_security_schemes, ApiToSpec,
    CollectOperations, EndpointDoc, EndpointToOperation, ErrorResponses, ExampleValue,
    QueryParameters, ToSchema,
};
pub use spec::{Components, OpenApiSpec, SecurityRequirement, SecurityScheme};
pub use swagger_output::{to_swagger2, to_swagger2_json};
