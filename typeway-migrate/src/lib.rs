pub mod cargo_editor;
pub mod emit;
pub mod interactive;
pub mod model;
pub mod parse;
pub mod transform;

use anyhow::Result;

/// Convert Axum source code to Typeway source code.
pub fn axum_to_typeway(source: &str) -> Result<String> {
    let model = parse::axum::parse_axum_file(source)?;
    let tokens = transform::axum_to_typeway::emit_typeway(&model);
    let warning_lines = transform::axum_to_typeway::emit_warning_lines(&model);
    let formatted = emit::codegen::format_tokens(&tokens);

    if warning_lines.is_empty() {
        Ok(formatted)
    } else {
        let mut output = warning_lines.join("\n");
        output.push_str("\n\n");
        output.push_str(&formatted);
        Ok(output)
    }
}

/// Convert Axum source code to Typeway source code with interactive and partial options.
///
/// When `interactive` is true, the tool prompts the user for decisions on
/// ambiguous cases instead of silently emitting TODO warnings.
///
/// When `partial` is provided, only endpoints matching the listed path patterns
/// are converted; others are excluded from the output.
pub fn axum_to_typeway_with_options(
    source: &str,
    interactive: bool,
    partial: Option<&[String]>,
) -> Result<String> {
    let mut model = parse::axum::parse_axum_file(source)?;

    if let Some(patterns) = partial {
        interactive::filter_partial(&mut model, patterns);
    }

    if interactive {
        interactive::resolve_ambiguities(&mut model)?;
    }

    let tokens = transform::axum_to_typeway::emit_typeway(&model);
    let warning_lines = transform::axum_to_typeway::emit_warning_lines(&model);
    let formatted = emit::codegen::format_tokens(&tokens);

    if warning_lines.is_empty() {
        Ok(formatted)
    } else {
        let mut output = warning_lines.join("\n");
        output.push_str("\n\n");
        output.push_str(&formatted);
        Ok(output)
    }
}

/// Convert Typeway source code to Axum source code.
pub fn typeway_to_axum(source: &str) -> Result<String> {
    let model = parse::typeway::parse_typeway_file(source)?;
    let tokens = transform::typeway_to_axum::emit_axum(&model);
    Ok(emit::codegen::format_tokens(&tokens))
}

/// Convert Typeway source code to Axum source code with interactive option.
///
/// When `interactive` is true, the tool prompts the user for decisions on
/// ambiguous cases.
pub fn typeway_to_axum_with_options(source: &str, interactive: bool) -> Result<String> {
    let mut model = parse::typeway::parse_typeway_file(source)?;

    if interactive {
        interactive::resolve_ambiguities(&mut model)?;
    }

    let tokens = transform::typeway_to_axum::emit_axum(&model);
    Ok(emit::codegen::format_tokens(&tokens))
}
