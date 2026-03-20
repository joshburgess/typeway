//! `typeway-openapi` — OpenAPI 3.1 spec derivation from Wayward API types.
//!
//! Walks an API type at startup and produces an OpenAPI specification.
//! No annotations required beyond what's already in the types.

pub mod derive;
pub mod spec;

#[cfg(feature = "schemars")]
pub use derive::from_schemars;
pub use derive::{
    ApiToSpec, CollectOperations, EndpointDoc, EndpointToOperation, ErrorResponses, ExampleValue,
    QueryParameters, ToSchema, apply_handler_docs, auto_tag_operations, collect_security_schemes,
};
pub use spec::{
    Components, OpenApiSpec, SecurityRequirement, SecurityScheme,
};
