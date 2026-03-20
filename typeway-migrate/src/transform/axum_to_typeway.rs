//! Transform an [`ApiModel`] into Typeway source code tokens.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::model::*;

/// Emit a `client_api!` invocation as a formatted string.
///
/// The output is returned as a comment block suitable for appending to the
/// generated source. Protected endpoints have the `Protected<Auth, ...>`
/// wrapper stripped because auth on the client side is typically handled by
/// request interceptors / headers rather than type-level wrappers.
pub fn emit_client_api_string(model: &ApiModel) -> String {
    if model.endpoints.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "// --- Type-safe client (requires `client` feature) ---".to_string(),
        "// Uncomment the following to generate a named client struct:".to_string(),
        "//".to_string(),
        "// client_api! {".to_string(),
        "//     pub struct ApiClient;".to_string(),
        "//".to_string(),
    ];

    for ep in &model.endpoints {
        let method_name = &ep.handler.name;
        let endpoint_type = build_client_endpoint_type(ep);
        lines.push(format!("//     {} => {};", method_name, endpoint_type));
    }

    lines.push("// }".to_string());
    lines.join("\n")
}

/// Build the endpoint type string for a single endpoint in client_api! context.
///
/// For auth-protected endpoints, the `Protected<Auth, E>` wrapper is stripped
/// and only the inner endpoint type is used.
fn build_client_endpoint_type(ep: &EndpointModel) -> String {
    let path_type = &ep.path.typeway_type_name;
    let res_type = quote! { #path_type };
    let response_type = &ep.response_type;
    let res_str = quote! { #response_type }.to_string();

    match ep.method {
        HttpMethod::Get | HttpMethod::Delete | HttpMethod::Head | HttpMethod::Options => {
            let endpoint_name = ep.method.typeway_endpoint_name();
            format!("{}<{}, {}>", endpoint_name, res_type, res_str)
        }
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {
            let endpoint_name = ep.method.typeway_endpoint_name();
            if let Some(ref req_type) = ep.request_body {
                let req_str = quote! { #req_type }.to_string();
                format!("{}<{}, {}, {}>", endpoint_name, res_type, req_str, res_str)
            } else {
                format!("{}<{}, (), {}>", endpoint_name, res_type, res_str)
            }
        }
    }
}

/// Transform an `ApiModel` into a complete Typeway source file.
pub fn emit_typeway(model: &ApiModel) -> TokenStream {
    let use_stmts = emit_use_statements(model);
    let passthrough = emit_passthrough_items(&model.passthrough_items);
    let path_decls = emit_path_declarations(model);
    let validator_structs = emit_validator_structs(model);
    let api_type = emit_api_type(model);
    let handlers = emit_handlers(model);
    let server = emit_server(model);

    quote! {
        #use_stmts

        #passthrough

        #path_decls

        #validator_structs

        #api_type

        #handlers

        #server
    }
}

/// Generate warning comment lines to prepend to the output source.
///
/// These are returned as plain strings because proc-macro token streams
/// cannot represent comments.
pub fn emit_warning_lines(model: &ApiModel) -> Vec<String> {
    let mut lines = Vec::new();
    for warning in &model.warnings {
        lines.push(format!("// TODO: {}", warning));
    }
    lines
}

fn emit_use_statements(model: &ApiModel) -> TokenStream {
    let has_effects = !model.detected_effects.is_empty();
    let has_validation = model
        .endpoints
        .iter()
        .any(|ep| ep.has_validation && ep.validator_name.is_some());

    let base = quote! { use typeway::prelude::*; };

    let effects_import = if has_effects {
        let effect_names: Vec<TokenStream> = model
            .detected_effects
            .iter()
            .map(|e| {
                let ident = format_ident!("{}", e.effect_name);
                quote! { #ident }
            })
            .collect();

        quote! {
            use typeway_core::effects::{#(#effect_names,)* Requires};
            use typeway_server::EffectfulServer;
        }
    } else {
        TokenStream::new()
    };

    let validation_import = if has_validation {
        let bind_validated_needed = model
            .endpoints
            .iter()
            .any(|ep| ep.bind_macro == BindMacro::BindValidated);

        if bind_validated_needed {
            quote! {
                use typeway_server::typed::{Validate, Validated};
                use typeway_server::bind_validated;
            }
        } else {
            quote! {
                use typeway_server::typed::{Validate, Validated};
            }
        }
    } else {
        TokenStream::new()
    };

    quote! {
        #base
        #effects_import
        #validation_import
    }
}

fn emit_passthrough_items(items: &[syn::Item]) -> TokenStream {
    let mut tokens = TokenStream::new();
    for item in items {
        tokens.extend(quote! { #item });
    }
    tokens
}

/// Emit `typeway_path!` declarations for all unique paths.
fn emit_path_declarations(model: &ApiModel) -> TokenStream {
    let mut seen = std::collections::HashSet::new();
    let mut tokens = TokenStream::new();

    for endpoint in &model.endpoints {
        let key = endpoint.path.raw_pattern.clone();
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        let type_name = &endpoint.path.typeway_type_name;
        let segments = emit_path_segments(&endpoint.path.segments);

        tokens.extend(quote! {
            typeway_path!(type #type_name = #segments);
        });
    }

    tokens
}

/// Emit validator struct skeletons for endpoints with detected validation.
fn emit_validator_structs(model: &ApiModel) -> TokenStream {
    let mut tokens = TokenStream::new();
    let mut seen = std::collections::HashSet::new();

    for endpoint in &model.endpoints {
        if let Some(ref validator_name) = endpoint.validator_name {
            if seen.contains(validator_name) {
                continue;
            }
            seen.insert(validator_name.clone());

            let validator_ident = format_ident!("{}", validator_name);

            // Determine the request body type name for the impl.
            let body_type = match &endpoint.request_body {
                Some(ty) => ty.clone(),
                None => continue,
            };

            // Collect detected validation pattern hints from the handler body.
            let body_str: String = endpoint
                .handler
                .body
                .iter()
                .map(|stmt| quote!(#stmt).to_string())
                .collect::<Vec<_>>()
                .join(" ");

            let mut hints = Vec::new();
            if body_str.contains(".is_empty()") {
                hints.push("  //   - .is_empty() checks");
            }
            if body_str.contains(".len()") {
                hints.push("  //   - .len() checks");
            }
            if body_str.contains("Err(") && body_str.contains("valid") {
                hints.push("  //   - Err(...) with validation messages");
            }

            let body_type_str = quote!(#body_type).to_string();
            let doc_line = format!(
                " TODO: Implement validation logic for {}.",
                body_type_str
            );
            let gen_line =
                " This validator was auto-generated because the handler contained";
            let move_line = " manual validation patterns. Move your validation logic here.";

            let hints_comment = if hints.is_empty() {
                TokenStream::new()
            } else {
                let hint_lines: Vec<TokenStream> = std::iter::once(
                    "  // Detected patterns in the original handler:".to_string(),
                )
                .chain(hints.iter().map(|h| h.to_string()))
                .map(|line| {
                    let line_str = line;
                    quote! {
                        #[doc = #line_str]
                    }
                })
                .collect();
                // We use a different approach: emit as regular comments via a trick.
                // Since quote! can't emit raw comments, we build them as doc attributes
                // that will be rendered by prettyplease.
                // Actually, let's just use a simpler approach with the code body.
                let _ = hint_lines;
                TokenStream::new()
            };

            let _ = hints_comment;

            // Build the hint string for the Ok(()) body comment.
            let pattern_hint = if hints.is_empty() {
                String::new()
            } else {
                let mut s = String::from(
                    "\n        // Detected patterns in the original handler:\n",
                );
                for h in &hints {
                    s.push_str(&format!("        {}\n", h.trim()));
                }
                s
            };
            let _ = pattern_hint;

            tokens.extend(quote! {
                #[doc = #doc_line]
                #[doc = #gen_line]
                #[doc = #move_line]
                struct #validator_ident;

                impl Validate<#body_type> for #validator_ident {
                    fn validate(body: &#body_type) -> Result<(), String> {
                        // TODO: Add validation rules here.
                        Ok(())
                    }
                }
            });
        }
    }

    tokens
}

/// Emit the path segments for a `typeway_path!` invocation.
///
/// "users" / u32 / "posts"
fn emit_path_segments(segments: &[PathSegment]) -> TokenStream {
    let mut parts: Vec<TokenStream> = Vec::new();

    for seg in segments {
        match seg {
            PathSegment::Literal(s) => {
                parts.push(quote! { #s });
            }
            PathSegment::Capture { ty, .. } => {
                if let Some(ty) = ty {
                    parts.push(quote! { #ty });
                } else {
                    // Fallback: unknown capture type.
                    parts.push(quote! { String });
                }
            }
        }
    }

    if parts.is_empty() {
        TokenStream::new()
    } else {
        let first = &parts[0];
        let rest = &parts[1..];
        let mut out = quote! { #first };
        for part in rest {
            out = quote! { #out / #part };
        }
        out
    }
}

/// Emit the `type API = (...)` declaration.
fn emit_api_type(model: &ApiModel) -> TokenStream {
    let has_cors = model
        .detected_effects
        .iter()
        .any(|e| e.effect_name == "CorsRequired");

    let endpoints: Vec<TokenStream> = model
        .endpoints
        .iter()
        .map(|ep| {
            let path_type = &ep.path.typeway_type_name;
            let res_type = &ep.response_type;

            let inner = match ep.method {
                HttpMethod::Get | HttpMethod::Delete | HttpMethod::Head | HttpMethod::Options => {
                    let endpoint_name =
                        format_ident!("{}", ep.method.typeway_endpoint_name());
                    quote! { #endpoint_name<#path_type, #res_type> }
                }
                HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {
                    let endpoint_name =
                        format_ident!("{}", ep.method.typeway_endpoint_name());
                    if let Some(ref req_type) = ep.request_body {
                        quote! { #endpoint_name<#path_type, #req_type, #res_type> }
                    } else {
                        // POST without a body — unusual but valid.
                        quote! { #endpoint_name<#path_type, (), #res_type> }
                    }
                }
            };

            // Wrap in Validated<Validator, ...> if validation is detected.
            // Validated goes inside Protected (Protected outside, Validated inside).
            let validated = if let Some(ref validator_name) = ep.validator_name {
                let validator_ident = format_ident!("{}", validator_name);
                quote! { Validated<#validator_ident, #inner> }
            } else {
                inner
            };

            // Wrap in Protected<AuthType, ...> if auth is required.
            let wrapped = if ep.requires_auth {
                if let Some(ref auth_ty) = ep.auth_type {
                    let auth_ident = format_ident!("{}", auth_ty);
                    quote! { Protected<#auth_ident, #validated> }
                } else {
                    validated
                }
            } else {
                validated
            };

            // Wrap public GET endpoints in Requires<CorsRequired, E> when CORS is detected.
            // Heuristic: endpoints NOT wrapped in Protected and using GET method are
            // the public-facing endpoints browsers access.
            if has_cors && !ep.requires_auth && ep.method == HttpMethod::Get {
                quote! { Requires<CorsRequired, #wrapped> }
            } else {
                wrapped
            }
        })
        .collect();

    quote! {
        type API = (#(#endpoints,)*);
    }
}

/// Emit handler functions with Typeway-style extractors.
fn emit_handlers(model: &ApiModel) -> TokenStream {
    let mut tokens = TokenStream::new();
    let mut seen = std::collections::HashSet::new();

    for endpoint in &model.endpoints {
        let name = &endpoint.handler.name;
        if seen.contains(&name.to_string()) {
            continue;
        }
        seen.insert(name.to_string());

        let handler_tokens = emit_single_handler(endpoint);
        tokens.extend(handler_tokens);
    }

    tokens
}

/// Emit a single handler function, transforming extractors.
fn emit_single_handler(endpoint: &EndpointModel) -> TokenStream {
    let handler = &endpoint.handler;
    let name = &handler.name;
    let return_type = &handler.return_type;
    let attrs = &handler.attrs;

    // Build the parameter list, transforming Path extractors.
    let mut params = Vec::new();
    let mut body_prefix: Vec<TokenStream> = Vec::new();

    for ext in &handler.extractors {
        match ext.kind {
            ExtractorKind::Path => {
                // Replace Path<u32> with Path<TypewayPathType>
                let path_type = &endpoint.path.typeway_type_name;
                let param_name = format_ident!("path");
                params.push(quote! { #param_name: Path<#path_type> });

                // Add destructuring at the start of the body.
                let var_names = crate::parse::common::extract_path_var_names(&ext.pattern);
                if !var_names.is_empty() {
                    if var_names.len() == 1 {
                        let var = &var_names[0];
                        body_prefix.push(quote! {
                            let (#var,) = path.0;
                        });
                    } else {
                        body_prefix.push(quote! {
                            let (#(#var_names,)*) = path.0;
                        });
                    }
                }
            }
            ExtractorKind::State => {
                // State(name): State<T> → name: State<T>, then add `let name = name.0;`
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("state"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });

                // Check if it was destructured (State(x) pattern).
                if matches!(&ext.pattern, syn::Pat::TupleStruct(_)) {
                    body_prefix.push(quote! {
                        let #var = #var.0;
                    });
                }
            }
            ExtractorKind::Json => {
                // Json(body): Json<T> → body: Json<T>, then add `let body = body.0;`
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("body"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });

                if matches!(&ext.pattern, syn::Pat::TupleStruct(_)) {
                    body_prefix.push(quote! {
                        let #var = #var.0;
                    });
                }
            }
            ExtractorKind::Query => {
                // Query<T> works the same in both frameworks — pass through.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("query"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });

                if matches!(&ext.pattern, syn::Pat::TupleStruct(_)) {
                    body_prefix.push(quote! {
                        let #var = #var.0;
                    });
                }
            }
            ExtractorKind::Header | ExtractorKind::HeaderMap => {
                // Header/HeaderMap extractors pass through unchanged.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("headers"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });

                if matches!(&ext.pattern, syn::Pat::TupleStruct(_)) {
                    body_prefix.push(quote! {
                        let #var = #var.0;
                    });
                }
            }
            ExtractorKind::Cookie | ExtractorKind::CookieJar => {
                // Cookie/CookieJar extractors pass through unchanged.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("cookie"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });

                if matches!(&ext.pattern, syn::Pat::TupleStruct(_)) {
                    body_prefix.push(quote! {
                        let #var = #var.0;
                    });
                }
            }
            ExtractorKind::Multipart | ExtractorKind::Form => {
                // Multipart/Form extractors pass through unchanged.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("form"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });
            }
            ExtractorKind::WebSocketUpgrade => {
                // WebSocket upgrade extractor passes through (untyped).
                // TODO: Consider using session-typed WebSocket with `TypedWebSocket<Protocol>` for protocol safety.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("ws"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });
            }
            ExtractorKind::Unknown if endpoint.requires_auth => {
                // Auth extractor: keep as first argument for bind_auth!.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("auth"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });
            }
            _ => {
                // Pass through other extractors unchanged.
                let pat = &ext.pattern;
                let ty = &ext.full_type;
                params.push(quote! { #pat: #ty });
            }
        }
    }

    let body_stmts = &handler.body;

    quote! {
        #(#attrs)*
        async fn #name(#(#params),*) -> #return_type {
            #(#body_prefix)*
            #(#body_stmts)*
        }
    }
}

/// Emit the Server construction and serve call.
fn emit_server(model: &ApiModel) -> TokenStream {
    let has_effects = !model.detected_effects.is_empty();

    let binds: Vec<TokenStream> = model
        .endpoints
        .iter()
        .map(|ep| {
            let name = &ep.handler.name;
            match ep.bind_macro {
                BindMacro::BindAuth => quote! { bind_auth!(#name) },
                BindMacro::BindValidated => quote! { bind_validated!(#name) },
                BindMacro::Bind => quote! { bind!(#name) },
            }
        })
        .collect();

    let layers: Vec<TokenStream> = model
        .layers
        .iter()
        .map(|layer| {
            quote! { .layer(#layer) }
        })
        .collect();

    let state = model.state_type.as_ref().map(|_| {
        // We can't easily recover the state construction expression,
        // so emit a placeholder.
        quote! {
            .with_state(state) // TODO: provide state value
        }
    });

    let nest = model.prefix.as_ref().map(|p| {
        quote! {
            .nest(#p)
        }
    });

    // OpenAPI setup comment — emitted as a doc attr on the serve function.
    let openapi_call = quote! { .with_openapi("API", "1.0.0") };

    if has_effects {
        // Generate .provide::<EffectName>() calls for each detected effect.
        let provides: Vec<TokenStream> = model
            .detected_effects
            .iter()
            .map(|e| {
                let effect_ident = format_ident!("{}", e.effect_name);
                quote! { .provide::<#effect_ident>() }
            })
            .collect();

        quote! {
            // Requires: typeway = { version = "0.1", features = ["openapi"] }
            // OpenAPI spec at /openapi.json, Swagger UI at /docs
            async fn serve(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                EffectfulServer::<API>::new((
                    #(#binds,)*
                ))
                #(#provides)*
                #(#layers)*
                #state
                #nest
                #openapi_call
                .ready()
                .serve(addr)
                .await
            }
        }
    } else {
        quote! {
            // Requires: typeway = { version = "0.1", features = ["openapi"] }
            // OpenAPI spec at /openapi.json, Swagger UI at /docs
            async fn serve(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                Server::<API>::new((
                    #(#binds,)*
                ))
                #(#layers)*
                #state
                #nest
                #openapi_call
                .serve(addr)
                .await
            }
        }
    }
}
