# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **OpenAPI 3.1 spec generation**: `ApiToSpec` trait walks API types at startup to produce a full spec
- **Enum schema support (`oneOf`)**: `#[derive(TypewaySchema)]` on Rust enums produces OpenAPI `oneOf` schemas. All four serde tagging modes are handled: externally tagged, internally tagged via `tag = "..."`, adjacently tagged via `tag + content`, and `untagged`. Honors `rename_all` (including kebab-case on PascalCase variants) and per-variant `#[serde(rename = "...")]`
- **Swagger 2 downgrade for sum types**: `to_swagger2_json` rewrites `oneOf` to the `x-oneOf` vendor extension and converts the OpenAPI 3 discriminator object to its Swagger 2 string form, so generated Swagger 2 specs validate
- **`EndpointToOperation`**: individual route-to-OpenAPI operation conversion
- **Embedded Swagger UI**: served at `/docs` with no CDN dependencies
- **Spec endpoint**: `GET /openapi.json` returns the generated spec
- **`EndpointDoc` trait**: attach summary, description, tags, and operation ID to endpoints
- **`ErrorResponses` trait**: typed error schemas included in the spec
- **`QueryParameters` trait**: typed query params in the spec
- **`ToSchema`**: impls for common types (String, u32, Vec, etc.)
- **`schemars` bridge** (feature `schemars`): derive JSON schemas from `schemars::JsonSchema` impls
