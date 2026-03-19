# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **`typeway_path!`**: ergonomic path type definitions (`typeway_path!(type P = "users" / u32)`)
- **`typeway_api!`**: inline API type definitions with method/path/body syntax
- **`endpoint!`**: builder macro for composing type-level wrappers
- **`#[handler]`**: attribute macro that validates handler functions at definition site (async check, extractor types, return type)
- **`#[api_description]`**: trait-based API definition with auto-generated endpoint types and `Serves` impl
- **`bind!()`**: handler binding macro
- **`bind_auth!()`**: handler binding with `Protected<Auth>` extraction
- **`bind_strict!()`**: handler binding with `Strict` return type enforcement
- **`bind_validated!()`**: handler binding with `Validated` body extraction
- **`bind_content_type!()`**: handler binding with `ContentType` enforcement
