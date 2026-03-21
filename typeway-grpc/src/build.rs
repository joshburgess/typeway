//! Build script helpers for compiling `.proto` files.
//!
//! These functions are intended to be called from a user's `build.rs` to
//! generate Rust types from `.proto` files at build time.
//!
//! # Example
//!
//! ```ignore
//! // In your build.rs:
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     typeway_grpc::build::compile_protos(&["proto/service.proto"], &["proto/"])?;
//!     Ok(())
//! }
//! ```
//!
//! This generates prost types that can be included in your code:
//!
//! ```ignore
//! // In your lib.rs or main.rs:
//! pub mod proto {
//!     include!(concat!(env!("OUT_DIR"), "/my.package.v1.rs"));
//! }
//! ```

/// Compile `.proto` files into Rust types using prost.
///
/// This is a thin wrapper around `prost_build::compile_protos` that
/// sets up the output directory and includes.
///
/// # Arguments
///
/// - `protos` — Paths to `.proto` files to compile
/// - `includes` — Directories to search for imports
///
/// # Example
///
/// ```ignore
/// // build.rs
/// fn main() {
///     typeway_grpc::build::compile_protos(
///         &["proto/users.proto", "proto/orders.proto"],
///         &["proto/"],
///     ).unwrap();
/// }
/// ```
pub fn compile_protos(
    protos: &[impl AsRef<std::path::Path>],
    includes: &[impl AsRef<std::path::Path>],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    // Generate serde derives alongside prost derives, so types work
    // with both JSON (typeway REST) and binary protobuf (gRPC).
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    config.compile_protos(protos, includes)?;
    Ok(())
}

/// Compile `.proto` files with custom prost configuration.
///
/// Use this when you need more control over code generation options
/// (e.g., custom type attributes, field attributes, extern paths).
///
/// # Example
///
/// ```ignore
/// // build.rs
/// fn main() {
///     let mut config = prost_build::Config::new();
///     config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
///     config.field_attribute("User.name", "#[serde(default)]");
///
///     typeway_grpc::build::compile_protos_with_config(
///         config,
///         &["proto/users.proto"],
///         &["proto/"],
///     ).unwrap();
/// }
/// ```
pub fn compile_protos_with_config(
    config: prost_build::Config,
    protos: &[impl AsRef<std::path::Path>],
    includes: &[impl AsRef<std::path::Path>],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = config;
    config.compile_protos(protos, includes)?;
    Ok(())
}

/// Generate a `.proto` file from a typeway API type at build time,
/// then compile it with prost.
///
/// This combines typeway's proto generation (`ApiToProto::to_proto`)
/// with prost compilation, producing Rust types that have both
/// `prost::Message` and `serde` derives.
///
/// # Arguments
///
/// - `proto_content` — The `.proto` file content (from `ApiToProto::to_proto`)
/// - `package` — The protobuf package name (e.g., `"users.v1"`)
///
/// # Example
///
/// ```ignore
/// // build.rs
/// fn main() {
///     // Generate proto from the API type.
///     let proto = MyAPI::to_proto("UserService", "users.v1");
///
///     typeway_grpc::build::compile_proto_string(
///         &proto,
///         "users.v1",
///     ).unwrap();
/// }
/// ```
pub fn compile_proto_string(
    proto_content: &str,
    package: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")
        .map_err(|_| "OUT_DIR not set — must be called from build.rs")?;

    // Write the proto content to a temporary file.
    let proto_dir = std::path::Path::new(&out_dir).join("typeway_proto");
    std::fs::create_dir_all(&proto_dir)?;

    let filename = format!("{}.proto", package.replace('.', "_"));
    let proto_path = proto_dir.join(&filename);
    std::fs::write(&proto_path, proto_content)?;

    compile_protos(&[proto_path], &[proto_dir])?;

    Ok(())
}
