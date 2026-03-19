//! Parse Typeway source files into the shared [`ApiModel`] IR.

use std::collections::HashMap;

use anyhow::{Context, Result};
use syn::{
    Expr, ExprCall, ExprPath, Item, ItemFn, ItemType, Type, TypePath, TypeTuple,
};

use crate::model::*;
use crate::parse::common;

/// Parse a Typeway source file into an `ApiModel`.
pub fn parse_typeway_file(source: &str) -> Result<ApiModel> {
    let file: syn::File =
        syn::parse_str(source).context("failed to parse Rust source file")?;

    let mut handler_fns: HashMap<String, &ItemFn> = HashMap::new();
    let mut path_types: HashMap<String, PathModel> = HashMap::new();
    let mut raw_endpoints: Vec<RawEndpoint> = Vec::new();
    let mut handler_order: Vec<String> = Vec::new();
    let mut layers: Vec<Expr> = Vec::new();
    let mut state_type: Option<Type> = None;
    let mut passthrough_items: Vec<Item> = Vec::new();
    let mut use_items: Vec<syn::ItemUse> = Vec::new();

    // First pass: collect everything.
    for item in &file.items {
        match item {
            Item::Fn(func) => {
                let name = func.sig.ident.to_string();

                // Check if this is the serve function containing Server::new.
                let server_info = extract_server_info(func);
                if let Some(info) = server_info {
                    handler_order = info.handler_names;
                    layers = info.layers;
                    if info.state_type.is_some() {
                        state_type = info.state_type;
                    }
                } else if func.sig.asyncness.is_some() {
                    handler_fns.insert(name, func);
                } else {
                    passthrough_items.push(item.clone());
                }
            }
            Item::Macro(mac) => {
                // Parse typeway_path! macro invocations.
                if let Some(path_model) = parse_typeway_path_macro(mac) {
                    path_types.insert(
                        path_model.typeway_type_name.to_string(),
                        path_model,
                    );
                } else {
                    passthrough_items.push(item.clone());
                }
            }
            Item::Type(item_type) => {
                // Parse `type API = (...)`.
                if item_type.ident == "API" {
                    raw_endpoints = parse_api_type_alias(item_type, &path_types);
                } else {
                    passthrough_items.push(item.clone());
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

    // Match handlers to endpoints by position (from Server::new bind order).
    let mut endpoints = Vec::new();
    for (i, raw_ep) in raw_endpoints.iter().enumerate() {
        let handler_name = handler_order
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("handler_{}", i));

        let handler = match handler_fns.get(&handler_name) {
            Some(func) => analyze_handler(func),
            None => HandlerModel {
                name: syn::Ident::new(&handler_name, proc_macro2::Span::call_site()),
                is_async: true,
                extractors: Vec::new(),
                return_type: syn::parse_quote! { impl IntoResponse },
                body: Vec::new(),
                attrs: Vec::new(),
            },
        };

        // Determine request body type from the endpoint definition or handler.
        let request_body = raw_ep.request_body.clone().or_else(|| {
            handler
                .extractors
                .iter()
                .find(|e| e.kind == ExtractorKind::Json)
                .and_then(|e| e.inner_type.clone())
        });

        let response_type = raw_ep.response_type.clone();

        let mut path = raw_ep.path.clone();

        // Fill capture types from path model and handler extractors.
        fill_capture_types_from_handler(&mut path, &handler);

        endpoints.push(EndpointModel {
            method: raw_ep.method,
            path,
            handler,
            request_body,
            response_type,
        });
    }

    Ok(ApiModel {
        endpoints,
        state_type,
        layers,
        passthrough_items,
        use_items,
    })
}

/// A raw endpoint parsed from the `type API = (...)` declaration.
#[derive(Debug)]
struct RawEndpoint {
    method: HttpMethod,
    path: PathModel,
    request_body: Option<Type>,
    response_type: Type,
}

/// Information extracted from a `Server::<API>::new(...)` call.
struct ServerInfo {
    handler_names: Vec<String>,
    layers: Vec<Expr>,
    state_type: Option<Type>,
}

/// Parse a `typeway_path!` macro invocation.
///
/// Expected syntax:
/// ```text
/// typeway_path!(type UsersPath = "users");
/// typeway_path!(type UserByIdPath = "users" / u32);
/// ```
fn parse_typeway_path_macro(mac: &syn::ItemMacro) -> Option<PathModel> {
    let path = mac.mac.path.segments.last()?;
    if path.ident != "typeway_path" {
        return None;
    }

    let tokens = mac.mac.tokens.clone();
    let token_str = tokens.to_string();

    // Parse: "type TypeName = segments..."
    // Strip "type " prefix.
    let rest = token_str.strip_prefix("type")?;
    let rest = rest.trim_start();

    // Split on "=".
    let (type_name, segments_str) = rest.split_once('=')?;
    let type_name = type_name.trim();
    let segments_str = segments_str.trim();

    // Parse segments separated by "/".
    let segments = parse_path_segments(segments_str);

    // Reconstruct the raw pattern from segments.
    let raw_pattern = segments_to_raw_pattern(&segments);

    let type_ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

    Some(PathModel {
        raw_pattern,
        segments,
        typeway_type_name: type_ident,
    })
}

/// Parse path segments from a string like `"users" / u32 / "posts" / u32`.
fn parse_path_segments(s: &str) -> Vec<PathSegment> {
    s.split('/')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| {
            if part.starts_with('"') && part.ends_with('"') {
                // Literal segment: strip quotes.
                let literal = part[1..part.len() - 1].to_string();
                PathSegment::Literal(literal)
            } else {
                // Capture segment: the part is a type name.
                let ty: Type = syn::parse_str(part).unwrap_or_else(|_| syn::parse_quote! { String });
                PathSegment::Capture {
                    name: part.to_lowercase(),
                    ty: Some(Box::new(ty)),
                }
            }
        })
        .collect()
}

/// Convert segments to a raw URL pattern string.
fn segments_to_raw_pattern(segments: &[PathSegment]) -> String {
    let parts: Vec<String> = segments
        .iter()
        .map(|seg| match seg {
            PathSegment::Literal(s) => s.clone(),
            PathSegment::Capture { name, .. } => format!("{{{}}}", name),
        })
        .collect();

    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// Parse `type API = (Endpoint1, Endpoint2, ...)` to extract endpoint types.
fn parse_api_type_alias(
    item: &ItemType,
    path_types: &HashMap<String, PathModel>,
) -> Vec<RawEndpoint> {
    let mut endpoints = Vec::new();

    // The type should be a tuple.
    if let Type::Tuple(TypeTuple { elems, .. }) = item.ty.as_ref() {
        for elem in elems {
            if let Some(ep) = parse_endpoint_type(elem, path_types) {
                endpoints.push(ep);
            }
        }
    }

    endpoints
}

/// Parse a single endpoint type like `GetEndpoint<UsersPath, Json<Vec<User>>>`.
fn parse_endpoint_type(
    ty: &Type,
    path_types: &HashMap<String, PathModel>,
) -> Option<RawEndpoint> {
    let type_path = match ty {
        Type::Path(TypePath { path, .. }) => path,
        _ => return None,
    };

    let last_seg = type_path.segments.last()?;
    let endpoint_name = last_seg.ident.to_string();

    // Determine method from endpoint type name.
    let method = method_from_endpoint_name(&endpoint_name)?;

    // Extract generic arguments.
    let args = match &last_seg.arguments {
        syn::PathArguments::AngleBracketed(ab) => &ab.args,
        _ => return None,
    };

    let type_args: Vec<&Type> = args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .collect();

    if type_args.is_empty() {
        return None;
    }

    // First arg is the path type name.
    let path_type_name = type_to_ident_string(type_args[0])?;
    let path = path_types
        .get(&path_type_name)
        .cloned()
        .unwrap_or_else(|| {
            // Fallback: create a placeholder path.
            PathModel {
                raw_pattern: format!("/{}", path_type_name.to_lowercase()),
                segments: vec![PathSegment::Literal(path_type_name.to_lowercase())],
                typeway_type_name: syn::Ident::new(
                    &path_type_name,
                    proc_macro2::Span::call_site(),
                ),
            }
        });

    // For body-having methods (Post, Put, Patch): args are (Path, ReqBody, ResBody).
    // For other methods: args are (Path, ResBody).
    let (request_body, response_type) = if method.has_body() && type_args.len() >= 3 {
        (Some(type_args[1].clone()), type_args[2].clone())
    } else if type_args.len() >= 2 {
        (None, type_args[1].clone())
    } else {
        (None, syn::parse_quote! { () })
    };

    Some(RawEndpoint {
        method,
        path,
        request_body,
        response_type,
    })
}

/// Extract a simple identifier string from a type.
fn type_to_ident_string(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            Some(path.segments.last()?.ident.to_string())
        }
        _ => None,
    }
}

/// Map endpoint type name prefix to an HTTP method.
fn method_from_endpoint_name(name: &str) -> Option<HttpMethod> {
    if name.starts_with("Get") {
        Some(HttpMethod::Get)
    } else if name.starts_with("Post") {
        Some(HttpMethod::Post)
    } else if name.starts_with("Put") {
        Some(HttpMethod::Put)
    } else if name.starts_with("Delete") {
        Some(HttpMethod::Delete)
    } else if name.starts_with("Patch") {
        Some(HttpMethod::Patch)
    } else if name.starts_with("Head") {
        Some(HttpMethod::Head)
    } else if name.starts_with("Options") {
        Some(HttpMethod::Options)
    } else {
        None
    }
}

/// Extract Server::new info from a function body.
fn extract_server_info(func: &ItemFn) -> Option<ServerInfo> {
    let mut info = ServerInfo {
        handler_names: Vec::new(),
        layers: Vec::new(),
        state_type: None,
    };

    let mut found = false;

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

        if extract_server_chain(expr, &mut info) {
            found = true;
        }
    }

    if found {
        Some(info)
    } else {
        None
    }
}

/// Recursively walk a method chain to find Server::new(...) and its layers.
fn extract_server_chain(expr: &Expr, info: &mut ServerInfo) -> bool {
    match expr {
        Expr::MethodCall(mc) => {
            let method_name = mc.method.to_string();
            match method_name.as_str() {
                "layer" => {
                    if let Some(arg) = mc.args.first() {
                        info.layers.push(arg.clone());
                    }
                    extract_server_chain(&mc.receiver, info)
                }
                "with_state" => {
                    if let Some(arg) = mc.args.first() {
                        info.state_type = infer_type_from_expr(arg);
                    }
                    extract_server_chain(&mc.receiver, info)
                }
                "serve" | "await" => extract_server_chain(&mc.receiver, info),
                _ => extract_server_chain(&mc.receiver, info),
            }
        }
        Expr::Call(call) => {
            // Check if this is Server::<API>::new((...))
            if is_server_new_call(call) {
                if let Some(arg) = call.args.first() {
                    info.handler_names = extract_bind_handler_names(arg);
                }
                return true;
            }
            false
        }
        Expr::Await(aw) => extract_server_chain(&aw.base, info),
        _ => false,
    }
}

/// Check if an expression is `Server::<API>::new(...)`.
fn is_server_new_call(call: &ExprCall) -> bool {
    match call.func.as_ref() {
        Expr::Path(ExprPath { path, .. }) => {
            let segments: Vec<_> = path.segments.iter().collect();
            // Match Server::<API>::new or Server::new
            if segments.len() >= 2 {
                let last = segments.last().unwrap();
                let second_last = segments[segments.len() - 2];
                return last.ident == "new" && second_last.ident == "Server";
            }
            false
        }
        _ => false,
    }
}

/// Extract handler names from `(bind!(h1), bind!(h2), ...)`.
fn extract_bind_handler_names(expr: &Expr) -> Vec<String> {
    let mut names = Vec::new();

    match expr {
        Expr::Tuple(tuple) => {
            for elem in &tuple.elems {
                if let Some(name) = extract_single_bind_name(elem) {
                    names.push(name);
                }
            }
        }
        _ => {
            // Single handler, not a tuple.
            if let Some(name) = extract_single_bind_name(expr) {
                names.push(name);
            }
        }
    }

    names
}

/// Extract the handler name from a `bind!(handler_name)` macro call.
fn extract_single_bind_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Macro(mac) => {
            let path = mac.mac.path.segments.last()?;
            if path.ident == "bind" {
                let tokens_str = mac.mac.tokens.to_string();
                Some(tokens_str.trim().to_string())
            } else {
                None
            }
        }
        Expr::Path(ExprPath { path, .. }) => {
            // Bare handler name without bind!
            Some(path.segments.last()?.ident.to_string())
        }
        _ => None,
    }
}

/// Analyze a handler function's signature and body.
fn analyze_handler(func: &ItemFn) -> HandlerModel {
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

/// Fill capture types on a path model from the handler's Path<T> extractor.
fn fill_capture_types_from_handler(path: &mut PathModel, handler: &HandlerModel) {
    let path_extractor = handler
        .extractors
        .iter()
        .find(|e| e.kind == ExtractorKind::Path);

    if let Some(ext) = path_extractor {
        if let Some(inner) = &ext.inner_type {
            // The inner type might be the path type name itself (e.g., UsersByIdPath).
            // In that case, we already have the types from the typeway_path! declaration.
            // But if capture types are still None, try to fill them from a tuple type.
            let capture_types = common::extract_path_capture_types(inner);

            // Only fill if capture_types look like actual types (not path type names).
            let all_filled = path
                .segments
                .iter()
                .all(|s| match s {
                    PathSegment::Capture { ty, .. } => ty.is_some(),
                    _ => true,
                });

            if !all_filled {
                let captures: Vec<_> = path
                    .segments
                    .iter_mut()
                    .filter(|s| matches!(s, PathSegment::Capture { .. }))
                    .collect();

                for (seg, ty) in captures.into_iter().zip(capture_types) {
                    if let PathSegment::Capture { ty: ref mut slot, .. } = seg {
                        if slot.is_none() {
                            *slot = Some(Box::new(ty));
                        }
                    }
                }
            }
        }
    }
}

/// Try to infer a type from an expression.
fn infer_type_from_expr(expr: &Expr) -> Option<Type> {
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
