//! Transform an [`ApiModel`] into Axum source code tokens.

use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::model::*;
use crate::parse::common;

/// Transform an `ApiModel` into a complete Axum source file.
pub fn emit_axum(model: &ApiModel) -> TokenStream {
    let use_stmts = emit_use_statements(model);
    let passthrough = emit_passthrough_items(&model.passthrough_items);
    let handlers = emit_handlers(model);
    let router = emit_router(model);

    quote! {
        #use_stmts

        #passthrough

        #handlers

        #router
    }
}

/// Emit Axum use statements, replacing typeway imports.
fn emit_use_statements(model: &ApiModel) -> TokenStream {
    // Determine which axum extractors are needed.
    let mut needs_path = false;
    let mut needs_state = false;
    let mut needs_json = false;
    let mut needs_query = false;
    let mut needs_status_code = false;

    for ep in &model.endpoints {
        for ext in &ep.handler.extractors {
            match ext.kind {
                ExtractorKind::Path => needs_path = true,
                ExtractorKind::State => needs_state = true,
                ExtractorKind::Json => needs_json = true,
                ExtractorKind::Query => needs_query = true,
                _ => {}
            }
        }
        // Check response type for StatusCode.
        let res_str = quote! { #(& ep.response_type) }.to_string();
        if res_str.contains("StatusCode") {
            needs_status_code = true;
        }
    }

    // Also check response types more carefully.
    for ep in &model.endpoints {
        let rt = &ep.response_type;
        let rt_str = format!("{}", quote! { #rt });
        if rt_str.contains("StatusCode") {
            needs_status_code = true;
        }
    }

    let mut extract_items = Vec::new();
    if needs_path {
        extract_items.push(quote! { Path });
    }
    if needs_state {
        extract_items.push(quote! { State });
    }
    if needs_json {
        extract_items.push(quote! { Json });
    }
    if needs_query {
        extract_items.push(quote! { Query });
    }

    let extract_use = if extract_items.is_empty() {
        TokenStream::new()
    } else {
        quote! { use axum::extract::{#(#extract_items),*}; }
    };

    let status_use = if needs_status_code {
        quote! { use axum::http::StatusCode; }
    } else {
        TokenStream::new()
    };

    // Collect unique method names needed for routing.
    let mut method_set = std::collections::BTreeSet::new();
    for ep in &model.endpoints {
        method_set.insert(ep.method.axum_fn_name());
    }
    let methods: Vec<TokenStream> = method_set
        .into_iter()
        .map(|name| {
            let method_name = format_ident!("{}", name);
            quote! { #method_name }
        })
        .collect();

    let routing_use = if methods.is_empty() {
        TokenStream::new()
    } else {
        quote! { use axum::routing::{#(#methods),*}; }
    };

    // Pass through non-typeway use statements (e.g., serde).
    let other_uses: Vec<TokenStream> = model
        .use_items
        .iter()
        .filter(|u| {
            let u_str = quote! { #u }.to_string();
            !u_str.contains("typeway")
        })
        .map(|u| quote! { #u })
        .collect();

    quote! {
        #extract_use
        #status_use
        #routing_use
        use axum::Router;
        #(#other_uses)*
    }
}

fn emit_passthrough_items(items: &[syn::Item]) -> TokenStream {
    let mut tokens = TokenStream::new();
    for item in items {
        tokens.extend(quote! { #item });
    }
    tokens
}

/// Emit handler functions with Axum-style extractors.
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

/// Emit a single handler function, transforming extractors to Axum style.
fn emit_single_handler(endpoint: &EndpointModel) -> TokenStream {
    let handler = &endpoint.handler;
    let name = &handler.name;
    let return_type = &handler.return_type;
    let attrs = &handler.attrs;

    let mut params = Vec::new();
    let mut body_stmts = Vec::new();

    // Collect names of destructured variables so we can filter their `let x = x.0;` lines.
    let mut destructured_vars: std::collections::HashSet<String> = std::collections::HashSet::new();

    for ext in &handler.extractors {
        match ext.kind {
            ExtractorKind::Path => {
                // Typeway style: `path: Path<PathType>` with `let (id,) = path.0;` in body.
                // Axum style: `Path(id): Path<u32>` or `Path((a, b)): Path<(u32, u32)>`.
                let capture_types = get_capture_types_for_path(endpoint);
                let capture_names = get_capture_names_for_path(handler);

                if capture_types.len() == 1 && capture_names.len() == 1 {
                    let var = &capture_names[0];
                    let ty = &capture_types[0];
                    params.push(quote! { Path(#var): Path<#ty> });
                    destructured_vars.insert(var.to_string());
                } else if capture_types.len() > 1 && capture_names.len() == capture_types.len() {
                    let vars = &capture_names;
                    let tys = &capture_types;
                    params.push(quote! { Path((#(#vars),*)): Path<(#(#tys),*)> });
                    for v in vars {
                        destructured_vars.insert(v.to_string());
                    }
                } else {
                    // Fallback: pass through.
                    let pat = &ext.pattern;
                    let ty = &ext.full_type;
                    params.push(quote! { #pat: #ty });
                }
            }
            ExtractorKind::State => {
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("state"));
                let full_type = &ext.full_type;
                params.push(quote! { State(#var): #full_type });
                destructured_vars.insert(var.to_string());
            }
            ExtractorKind::Json => {
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("body"));
                let full_type = &ext.full_type;
                params.push(quote! { Json(#var): #full_type });
                destructured_vars.insert(var.to_string());
            }
            ExtractorKind::Query => {
                // Query<T> passes through to Axum style with destructuring.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("query"));
                let full_type = &ext.full_type;
                params.push(quote! { Query(#var): #full_type });
                destructured_vars.insert(var.to_string());
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
                    destructured_vars.insert(var.to_string());
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
                // WebSocket upgrade extractor passes through.
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("ws"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });
            }
            ExtractorKind::Unknown if endpoint.requires_auth => {
                // Auth extractor: emit as a plain argument (Axum custom extractor).
                let var = ext
                    .var_name
                    .clone()
                    .unwrap_or_else(|| format_ident!("auth"));
                let full_type = &ext.full_type;
                params.push(quote! { #var: #full_type });
            }
            _ => {
                let pat = &ext.pattern;
                let ty = &ext.full_type;
                params.push(quote! { #pat: #ty });
            }
        }
    }

    // Filter body statements: remove `let x = x.0;` and `let (x,) = y.0;` lines
    // that correspond to destructured extractors.
    for stmt in &handler.body {
        if should_filter_destructuring_stmt(stmt, &destructured_vars) {
            continue;
        }
        body_stmts.push(quote! { #stmt });
    }

    quote! {
        #(#attrs)*
        async fn #name(#(#params),*) -> #return_type {
            #(#body_stmts)*
        }
    }
}

/// Check if a statement is a `let x = x.0;` or `let (x,) = y.0;` destructuring
/// that should be removed because the extractor pattern now handles it.
fn should_filter_destructuring_stmt(
    stmt: &syn::Stmt,
    destructured_vars: &std::collections::HashSet<String>,
) -> bool {
    let local = match stmt {
        syn::Stmt::Local(local) => local,
        _ => return false,
    };

    let init = match &local.init {
        Some(init) => init,
        None => return false,
    };

    // Check if the RHS is `something.0` (field access on .0).
    let is_dot_zero = match init.expr.as_ref() {
        syn::Expr::Field(field) => {
            matches!(&field.member, syn::Member::Unnamed(idx) if idx.index == 0)
        }
        _ => false,
    };

    if !is_dot_zero {
        return false;
    }

    // Check if the LHS pattern involves a destructured variable.
    match &local.pat {
        syn::Pat::Ident(pat_id) => {
            destructured_vars.contains(&pat_id.ident.to_string())
        }
        syn::Pat::Tuple(tuple) => {
            // `let (x,) = y.0;`
            tuple.elems.iter().any(|elem| {
                if let syn::Pat::Ident(pi) = elem {
                    destructured_vars.contains(&pi.ident.to_string())
                } else {
                    false
                }
            })
        }
        _ => false,
    }
}

/// Get capture types from the endpoint's path model.
fn get_capture_types_for_path(endpoint: &EndpointModel) -> Vec<syn::Type> {
    endpoint
        .path
        .segments
        .iter()
        .filter_map(|seg| match seg {
            PathSegment::Capture { ty, .. } => ty.as_ref().map(|t| *t.clone()),
            _ => None,
        })
        .collect()
}

/// Get capture variable names from the handler's Path extractor destructuring.
///
/// For Typeway handlers, the pattern is `path: Path<TypewayPath>` with
/// `let (id,) = path.0;` in the body. We look for those let bindings.
fn get_capture_names_for_path(handler: &HandlerModel) -> Vec<syn::Ident> {
    // First, look for the path extractor parameter name.
    let path_param_name = handler
        .extractors
        .iter()
        .find(|e| e.kind == ExtractorKind::Path)
        .and_then(|e| e.var_name.clone());

    let param_name = match path_param_name {
        Some(name) => name.to_string(),
        None => return Vec::new(),
    };

    // Look for `let (x,) = param_name.0;` or `let (x, y) = param_name.0;` in the body.
    for stmt in &handler.body {
        if let syn::Stmt::Local(local) = stmt {
            if let Some(init) = &local.init {
                // Check RHS is `param_name.0`.
                if let syn::Expr::Field(field) = init.expr.as_ref() {
                    let is_param_field = match field.base.as_ref() {
                        syn::Expr::Path(ep) => {
                            ep.path.segments.last().map(|s| s.ident.to_string())
                                == Some(param_name.clone())
                        }
                        _ => false,
                    };
                    let is_dot_zero = matches!(
                        &field.member,
                        syn::Member::Unnamed(idx) if idx.index == 0
                    );

                    if is_param_field && is_dot_zero {
                        // Extract names from LHS pattern.
                        match &local.pat {
                            syn::Pat::Tuple(tuple) => {
                                let names: Vec<_> = tuple
                                    .elems
                                    .iter()
                                    .filter_map(|elem| {
                                        if let syn::Pat::Ident(pi) = elem {
                                            Some(pi.ident.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                if !names.is_empty() {
                                    return names;
                                }
                            }
                            syn::Pat::Ident(pi) => {
                                return vec![pi.ident.clone()];
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Fallback: generate names from path segments.
    endpoint_default_capture_names(&handler.extractors)
}

/// Generate default capture names from path segments when we can't find them in the body.
fn endpoint_default_capture_names(extractors: &[ExtractorModel]) -> Vec<syn::Ident> {
    // Try to get from the extractor pattern itself.
    for ext in extractors {
        if ext.kind == ExtractorKind::Path {
            let names = common::extract_path_var_names(&ext.pattern);
            if !names.is_empty() {
                return names;
            }
        }
    }
    Vec::new()
}

/// Emit the Router construction.
fn emit_router(model: &ApiModel) -> TokenStream {
    // Group endpoints by path pattern.
    let mut groups: BTreeMap<String, Vec<&EndpointModel>> = BTreeMap::new();
    for ep in &model.endpoints {
        groups
            .entry(ep.path.raw_pattern.clone())
            .or_default()
            .push(ep);
    }

    let mut route_calls = Vec::new();

    for (pattern, endpoints) in &groups {
        // Convert typeway path pattern to Axum format.
        // Typeway: /users/{u32} → Axum: /users/{id}
        let axum_pattern = to_axum_path_pattern(pattern, endpoints);

        // Build method router: `get(handler1).post(handler2)`
        let mut method_chain: Option<TokenStream> = None;
        for ep in endpoints {
            let method_fn = format_ident!("{}", ep.method.axum_fn_name());
            let handler_name = &ep.handler.name;
            let call = quote! { #method_fn(#handler_name) };

            method_chain = Some(match method_chain {
                Some(existing) => quote! { #existing.#call },
                None => call,
            });
        }

        if let Some(chain) = method_chain {
            route_calls.push(quote! {
                .route(#axum_pattern, #chain)
            });
        }
    }

    let layers: Vec<TokenStream> = model
        .layers
        .iter()
        .map(|layer| quote! { .layer(#layer) })
        .collect();

    // Determine the function name and return type based on state.
    let fn_sig = if let Some(st) = &model.state_type {
        quote! { fn app() -> Router<#st> }
    } else {
        quote! { fn app() -> Router }
    };

    quote! {
        #fn_sig {
            Router::new()
                #(#route_calls)*
                #(#layers)*
        }
    }
}

/// Convert a path pattern to Axum format, using capture names derived from handlers.
fn to_axum_path_pattern(_pattern: &str, endpoints: &[&EndpointModel]) -> String {
    // Use the first endpoint that has a Path extractor to derive capture names.
    let ep = endpoints.first().unwrap();

    // Try to get capture names from handler body destructuring.
    let capture_names = get_capture_names_for_path(&ep.handler);

    let mut result = String::new();
    let mut capture_idx = 0;

    for seg in &ep.path.segments {
        result.push('/');
        match seg {
            PathSegment::Literal(s) => result.push_str(s),
            PathSegment::Capture { name, .. } => {
                result.push('{');
                if capture_idx < capture_names.len() {
                    result.push_str(&capture_names[capture_idx].to_string());
                } else {
                    // Fallback to the segment name from the path model.
                    result.push_str(name);
                }
                result.push('}');
                capture_idx += 1;
            }
        }
    }

    if result.is_empty() {
        "/".to_string()
    } else {
        result
    }
}
