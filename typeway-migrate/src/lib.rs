pub mod emit;
pub mod model;
pub mod parse;
pub mod transform;

use anyhow::Result;

/// Convert Axum source code to Typeway source code.
pub fn axum_to_typeway(source: &str) -> Result<String> {
    let model = parse::axum::parse_axum_file(source)?;
    let tokens = transform::axum_to_typeway::emit_typeway(&model);
    Ok(emit::codegen::format_tokens(&tokens))
}

/// Convert Typeway source code to Axum source code.
pub fn typeway_to_axum(source: &str) -> Result<String> {
    let model = parse::typeway::parse_typeway_file(source)?;
    let tokens = transform::typeway_to_axum::emit_axum(&model);
    Ok(emit::codegen::format_tokens(&tokens))
}
