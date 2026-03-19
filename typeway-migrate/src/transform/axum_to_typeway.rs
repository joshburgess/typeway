//! Transform an [`ApiModel`] into Typeway source code tokens.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::model::*;

/// Transform an `ApiModel` into a complete Typeway source file.
pub fn emit_typeway(model: &ApiModel) -> TokenStream {
    let use_stmts = emit_use_statements();
    let passthrough = emit_passthrough_items(&model.passthrough_items);
    let path_decls = emit_path_declarations(model);
    let api_type = emit_api_type(model);
    let handlers = emit_handlers(model);
    let server = emit_server(model);

    quote! {
        #use_stmts

        #passthrough

        #path_decls

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

fn emit_use_statements() -> TokenStream {
    quote! {
        use typeway::prelude::*;
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
    let endpoints: Vec<TokenStream> = model
        .endpoints
        .iter()
        .map(|ep| {
            let path_type = &ep.path.typeway_type_name;
            let res_type = &ep.response_type;

            match ep.method {
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
    let binds: Vec<TokenStream> = model
        .endpoints
        .iter()
        .map(|ep| {
            let name = &ep.handler.name;
            quote! { bind!(#name) }
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

    quote! {
        async fn serve(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Server::<API>::new((
                #(#binds,)*
            ))
            #(#layers)*
            #state
            #nest
            .serve(addr)
            .await
        }
    }
}
