//! Parse Axum source files into the shared [`ApiModel`] IR.

use std::collections::HashMap;

use anyhow::{Context, Result};
use syn::{
    Expr, ExprCall, ExprMethodCall, ExprPath, Item, ItemFn, Lit,
};

use crate::model::*;
use crate::parse::common;

/// A route extracted from a `Router::new().route(...)` chain.
#[derive(Debug)]
struct RawRoute {
    path_pattern: String,
    method: HttpMethod,
    handler_name: String,
}

/// Parse an Axum source file into an `ApiModel`.
pub fn parse_axum_file(source: &str) -> Result<ApiModel> {
    let file: syn::File =
        syn::parse_str(source).context("failed to parse Rust source file")?;

    let mut handler_fns: HashMap<String, &ItemFn> = HashMap::new();
    let mut raw_routes: Vec<RawRoute> = Vec::new();
    let mut layers: Vec<Expr> = Vec::new();
    let mut state_type: Option<syn::Type> = None;
    let mut passthrough_items: Vec<Item> = Vec::new();
    let mut use_items: Vec<syn::ItemUse> = Vec::new();

    // First pass: collect handler functions and identify the router function.
    for item in &file.items {
        match item {
            Item::Fn(func) => {
                if func.sig.asyncness.is_some() {
                    handler_fns.insert(func.sig.ident.to_string(), func);
                } else {
                    // Check if this is a router-building function.
                    let routes_and_layers =
                        extract_routes_from_fn(func);
                    if !routes_and_layers.routes.is_empty() {
                        raw_routes.extend(routes_and_layers.routes);
                        layers.extend(routes_and_layers.layers);
                        if routes_and_layers.state_type.is_some() {
                            state_type = routes_and_layers.state_type;
                        }
                    } else {
                        passthrough_items.push(item.clone());
                    }
                }
            }
            Item::Use(u) => {
                use_items.push(u.clone());
            }
            other => {
                passthrough_items.push(other.clone());
            }
        }
    }

    // If no routes found in sync functions, check async main or similar.
    if raw_routes.is_empty() {
        for func in handler_fns.values() {
            let routes_and_layers = extract_routes_from_fn(func);
            if !routes_and_layers.routes.is_empty() {
                raw_routes.extend(routes_and_layers.routes);
                layers.extend(routes_and_layers.layers);
                if routes_and_layers.state_type.is_some() {
                    state_type = routes_and_layers.state_type;
                }
            }
        }
    }

    // Build path models and deduplicate by pattern.
    let mut path_models: HashMap<String, PathModel> = HashMap::new();
    for route in &raw_routes {
        path_models
            .entry(route.path_pattern.clone())
            .or_insert_with(|| PathModel::from_axum_path(&route.path_pattern));
    }

    // Build endpoint models by matching routes to handlers.
    let mut endpoints = Vec::new();
    for route in &raw_routes {
        let path_model = path_models[&route.path_pattern].clone();

        let handler = match handler_fns.get(&route.handler_name) {
            Some(func) => analyze_handler(func, &mut path_models),
            None => {
                // Handler not found as an async fn — might be a closure or imported.
                // Create a placeholder.
                HandlerModel {
                    name: syn::Ident::new(
                        &route.handler_name,
                        proc_macro2::Span::call_site(),
                    ),
                    is_async: true,
                    extractors: Vec::new(),
                    return_type: syn::parse_quote! { impl IntoResponse },
                    body: Vec::new(),
                    attrs: Vec::new(),
                }
            }
        };

        // Determine request body type from handler's Json<T> extractor.
        let request_body = handler
            .extractors
            .iter()
            .find(|e| e.kind == ExtractorKind::Json)
            .and_then(|e| e.inner_type.clone());

        let response_type = handler.return_type.clone();

        endpoints.push(EndpointModel {
            method: route.method,
            path: path_model,
            handler,
            request_body,
            response_type,
        });
    }

    // Fill in capture types from handler Path<T> extractors.
    for endpoint in &mut endpoints {
        fill_capture_types(endpoint, &path_models);
    }

    Ok(ApiModel {
        endpoints,
        state_type,
        layers,
        passthrough_items,
        use_items,
    })
}

/// Results from scanning a function for route definitions.
struct RoutesAndLayers {
    routes: Vec<RawRoute>,
    layers: Vec<Expr>,
    state_type: Option<syn::Type>,
}

/// Scan a function body for `Router::new().route(...)` chains.
fn extract_routes_from_fn(func: &ItemFn) -> RoutesAndLayers {
    let mut result = RoutesAndLayers {
        routes: Vec::new(),
        layers: Vec::new(),
        state_type: None,
    };

    // Walk statements looking for expressions that are Router chains.
    for stmt in &func.block.stmts {
        let expr = match stmt {
            syn::Stmt::Expr(e, _) => e,
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    &init.expr
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        extract_from_expr(expr, &mut result);
    }

    result
}

/// Recursively extract routes and layers from a method call chain.
fn extract_from_expr(expr: &Expr, result: &mut RoutesAndLayers) {
    match expr {
        Expr::MethodCall(mc) => {
            let method_name = mc.method.to_string();

            match method_name.as_str() {
                "route" => {
                    if let Some((path, routes)) = parse_route_call(mc) {
                        for (method, handler_name) in routes {
                            result.routes.push(RawRoute {
                                path_pattern: path.clone(),
                                method,
                                handler_name,
                            });
                        }
                    }
                    // Continue into the receiver for chained calls.
                    extract_from_expr(&mc.receiver, result);
                }
                "layer" => {
                    if let Some(arg) = mc.args.first() {
                        result.layers.push(arg.clone());
                    }
                    extract_from_expr(&mc.receiver, result);
                }
                "with_state" => {
                    if let Some(arg) = mc.args.first() {
                        // Try to extract the state type from the expression.
                        result.state_type = infer_type_from_expr(arg);
                    }
                    extract_from_expr(&mc.receiver, result);
                }
                "nest" => {
                    // TODO: handle nested routers
                    extract_from_expr(&mc.receiver, result);
                }
                _ => {
                    extract_from_expr(&mc.receiver, result);
                }
            }
        }
        Expr::Call(ExprCall { func, .. }) => {
            // Router::new() — nothing to extract, but recurse into func
            if let Expr::Path(_) = func.as_ref() {
                // This is the Router::new() call — terminal
            }
        }
        _ => {}
    }
}

/// Parse a `.route("/path", get(handler).post(handler2))` call.
///
/// Returns the path pattern and a list of (method, handler_name) pairs.
fn parse_route_call(mc: &ExprMethodCall) -> Option<(String, Vec<(HttpMethod, String)>)> {
    // First arg: path string literal.
    let path = match mc.args.first()? {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(s) => s.value(),
            _ => return None,
        },
        _ => return None,
    };

    // Second arg: method_router expression (e.g., `get(handler)` or `get(h1).post(h2)`).
    let method_router = mc.args.get(1)?;
    let mut routes = Vec::new();
    extract_method_routes(method_router, &mut routes);

    if routes.is_empty() {
        None
    } else {
        Some((path, routes))
    }
}

/// Extract (method, handler_name) pairs from a method router expression.
///
/// Handles:
/// - `get(handler)` → [(Get, "handler")]
/// - `get(h1).post(h2)` → [(Get, "h1"), (Post, "h2")]
fn extract_method_routes(expr: &Expr, out: &mut Vec<(HttpMethod, String)>) {
    match expr {
        // get(handler) or post(handler)
        Expr::Call(call) => {
            if let Expr::Path(ExprPath { path, .. }) = call.func.as_ref() {
                if let Some(seg) = path.segments.last() {
                    let method_name = seg.ident.to_string();
                    if let Some(method) = HttpMethod::from_axum_method_name(&method_name) {
                        if let Some(handler_name) = extract_handler_name_from_args(&call.args) {
                            out.push((method, handler_name));
                        }
                    }
                }
            }
        }
        // get(h1).post(h2) — chained method router
        Expr::MethodCall(mc) => {
            let method_name = mc.method.to_string();
            if let Some(method) = HttpMethod::from_axum_method_name(&method_name) {
                if let Some(handler_name) = extract_handler_name_from_args(&mc.args) {
                    out.push((method, handler_name));
                }
            }
            // Recurse into receiver for the chain.
            extract_method_routes(&mc.receiver, out);
        }
        _ => {}
    }
}

/// Extract the handler function name from call arguments.
fn extract_handler_name_from_args(
    args: &syn::punctuated::Punctuated<Expr, syn::token::Comma>,
) -> Option<String> {
    let first = args.first()?;
    match first {
        Expr::Path(ExprPath { path, .. }) => {
            Some(path.segments.last()?.ident.to_string())
        }
        _ => None,
    }
}

/// Analyze a handler function's signature and body.
fn analyze_handler(
    func: &ItemFn,
    _path_models: &mut HashMap<String, PathModel>,
) -> HandlerModel {
    let extractors: Vec<ExtractorModel> = func
        .sig
        .inputs
        .iter()
        .filter_map(common::analyze_extractor)
        .collect();

    let return_type = common::extract_return_type(&func.sig.output);
    let body = func.block.stmts.clone();
    let attrs = func.attrs.clone();

    HandlerModel {
        name: func.sig.ident.clone(),
        is_async: func.sig.asyncness.is_some(),
        extractors,
        return_type,
        body,
        attrs,
    }
}

/// Fill in capture types on a path model from the handler's Path<T> extractor.
fn fill_capture_types(
    endpoint: &mut EndpointModel,
    path_models: &HashMap<String, PathModel>,
) {
    let path_extractor = endpoint
        .handler
        .extractors
        .iter()
        .find(|e| e.kind == ExtractorKind::Path);

    if let Some(ext) = path_extractor {
        if let Some(inner) = &ext.inner_type {
            let capture_types = common::extract_path_capture_types(inner);
            let captures: Vec<_> = endpoint
                .path
                .segments
                .iter_mut()
                .filter(|s| matches!(s, PathSegment::Capture { .. }))
                .collect();

            for (seg, ty) in captures.into_iter().zip(capture_types) {
                if let PathSegment::Capture { ty: ref mut slot, .. } = seg {
                    *slot = Some(Box::new(ty));
                }
            }
        }
    }

    // Update the canonical path model with filled types.
    if let Some(canonical) = path_models.get(&endpoint.path.raw_pattern) {
        // Use the canonical type name.
        endpoint.path.typeway_type_name = canonical.typeway_type_name.clone();
    }
}

/// Try to infer a type from an expression (very rough heuristic).
fn infer_type_from_expr(expr: &Expr) -> Option<syn::Type> {
    match expr {
        Expr::Struct(s) => {
            let path = &s.path;
            Some(syn::parse_quote! { #path })
        }
        Expr::Call(call) => {
            if let Expr::Path(p) = call.func.as_ref() {
                let path = &p.path;
                Some(syn::parse_quote! { #path })
            } else {
                None
            }
        }
        Expr::Path(p) => {
            let path = &p.path;
            Some(syn::parse_quote! { #path })
        }
        _ => None,
    }
}
