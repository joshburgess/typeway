//! `typeway-macros` — proc macros for the Typeway web framework.
//!
//! Provides `typeway_path!` for ergonomic path type construction,
//! `typeway_api!` for defining complete API types with inline routes,
//! and `#[handler]` for validating handler functions at the definition site.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Type};

// ---------------------------------------------------------------------------
// Path segment parsing (shared between macros)
// ---------------------------------------------------------------------------

enum PathSegment {
    Literal(String),
    Capture(Type),
}

fn parse_one_segment(input: ParseStream) -> syn::Result<PathSegment> {
    if input.peek(LitStr) {
        let lit: LitStr = input.parse()?;
        let value = lit.value();
        if value.is_empty() {
            return Err(syn::Error::new(lit.span(), "path literal cannot be empty"));
        }
        if value.contains('/') {
            return Err(syn::Error::new(
                lit.span(),
                "path literal cannot contain '/'; use separate segments",
            ));
        }
        Ok(PathSegment::Literal(value))
    } else {
        let ty: Type = input.parse()?;
        Ok(PathSegment::Capture(ty))
    }
}

fn parse_path_segments(input: ParseStream) -> syn::Result<Vec<PathSegment>> {
    let mut segments = Vec::new();
    if input.is_empty() || input.peek(Token![;]) {
        return Ok(segments);
    }
    segments.push(parse_one_segment(input)?);
    while input.peek(Token![/]) {
        input.parse::<Token![/]>()?;
        segments.push(parse_one_segment(input)?);
    }
    Ok(segments)
}

/// Build the HCons type chain from path segments.
/// `mod_path` is the module path prefix for marker types (e.g., `__m::` ).
fn build_hlist_type(segments: &[PathSegment], mod_path: &TokenStream2) -> TokenStream2 {
    if segments.is_empty() {
        return quote! { ::typeway_core::HNil };
    }

    let head = match &segments[0] {
        PathSegment::Literal(s) => {
            let marker = lit_marker_ident(s);
            quote! { ::typeway_core::Lit<#mod_path #marker> }
        }
        PathSegment::Capture(ty) => {
            quote! { ::typeway_core::Capture<#ty> }
        }
    };

    let tail = build_hlist_type(&segments[1..], mod_path);
    quote! { ::typeway_core::HCons<#head, #tail> }
}

fn lit_marker_ident(s: &str) -> Ident {
    format_ident!("__lit_{}", s)
}

/// Collect unique marker type definitions for all literal segments.
fn collect_marker_defs(
    segments: &[PathSegment],
    seen: &mut std::collections::HashSet<String>,
) -> Vec<TokenStream2> {
    let mut defs = Vec::new();
    for seg in segments {
        if let PathSegment::Literal(s) = seg {
            if seen.insert(s.clone()) {
                let marker = lit_marker_ident(s);
                let value = s.as_str();
                defs.push(quote! {
                    #[allow(non_camel_case_types)]
                    pub struct #marker;
                    impl ::typeway_core::LitSegment for #marker {
                        const VALUE: &'static str = #value;
                    }
                });
            }
        }
    }
    defs
}

// ---------------------------------------------------------------------------
// typeway_path! macro
// ---------------------------------------------------------------------------

/// Defines a path type with auto-generated literal segment markers.
///
/// Markers are scoped in a private module to avoid name collisions.
///
/// # Syntax
///
/// ```ignore
/// typeway_path!(type UserPath = "users" / u32);
/// ```
///
/// Expands to:
///
/// ```ignore
/// mod __wp_UserPath {
///     pub struct __lit_users;
///     impl typeway_core::LitSegment for __lit_users { ... }
/// }
/// type UserPath = HCons<Lit<__wp_UserPath::__lit_users>, HCons<Capture<u32>, HNil>>;
/// ```
#[proc_macro]
pub fn typeway_path(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as WaywardPathInput);
    let name = &input.name;
    let vis = &input.vis;
    let mod_name = format_ident!("__wp_{}", name);

    let mut seen = std::collections::HashSet::new();
    let marker_defs = collect_marker_defs(&input.segments, &mut seen);

    let mod_path: TokenStream2 = quote! { #mod_name:: };
    let hlist_type = build_hlist_type(&input.segments, &mod_path);

    quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #mod_name {
            #(#marker_defs)*
        }
        #vis type #name = #hlist_type;
    }
    .into()
}

struct WaywardPathInput {
    vis: syn::Visibility,
    name: Ident,
    segments: Vec<PathSegment>,
}

impl Parse for WaywardPathInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vis: syn::Visibility = input.parse()?;
        input.parse::<Token![type]>()?;
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let segments = parse_path_segments(input)?;
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }
        Ok(WaywardPathInput {
            vis,
            name,
            segments,
        })
    }
}

// ---------------------------------------------------------------------------
// typeway_api! macro
// ---------------------------------------------------------------------------

/// Defines a complete API type with inline route definitions.
///
/// # Syntax
///
/// ```ignore
/// typeway_api! {
///     type MyAPI = {
///         GET "users" => Json<Vec<User>>,
///         GET "users" / u32 => Json<User>,
///         POST "users" [Json<CreateUser>] => Json<User>,
///         DELETE "users" / u32 => StatusCode,
///     };
/// }
/// ```
///
/// - Methods: `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, `OPTIONS`
/// - Request body is specified in `[brackets]` (optional, only for POST/PUT/PATCH)
/// - Response type follows `=>`
#[proc_macro]
pub fn typeway_api(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as WaywardApiInput);
    let name = &input.name;
    let vis = &input.vis;
    let mod_name = format_ident!("__wa_{}", name);

    let mut seen = std::collections::HashSet::new();
    let mut all_marker_defs = Vec::new();
    for route in &input.routes {
        all_marker_defs.extend(collect_marker_defs(&route.path, &mut seen));
    }

    let mod_path: TokenStream2 = quote! { #mod_name:: };
    let mut endpoint_types = Vec::new();
    for route in &input.routes {
        let path_type = build_hlist_type(&route.path, &mod_path);
        let method = method_type_ident(&route.method);
        let res_type = &route.response;

        let endpoint = if let Some(ref req) = route.request {
            quote! { ::typeway_core::Endpoint<::typeway_core::#method, #path_type, #req, #res_type> }
        } else {
            quote! { ::typeway_core::Endpoint<::typeway_core::#method, #path_type, ::typeway_core::NoBody, #res_type> }
        };
        endpoint_types.push(endpoint);
    }

    quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #mod_name {
            #(#all_marker_defs)*
        }
        #vis type #name = (#(#endpoint_types,)*);
    }
    .into()
}

fn method_type_ident(method: &str) -> Ident {
    let s = match method.to_uppercase().as_str() {
        "GET" => "Get",
        "POST" => "Post",
        "PUT" => "Put",
        "DELETE" => "Delete",
        "PATCH" => "Patch",
        "HEAD" => "Head",
        "OPTIONS" => "Options",
        other => panic!("unknown HTTP method: {other}"),
    };
    Ident::new(s, Span::call_site())
}

struct ApiRoute {
    method: String,
    path: Vec<PathSegment>,
    request: Option<Type>,
    response: Type,
}

struct WaywardApiInput {
    vis: syn::Visibility,
    name: Ident,
    routes: Vec<ApiRoute>,
}

impl Parse for WaywardApiInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vis: syn::Visibility = input.parse()?;
        input.parse::<Token![type]>()?;
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        let content;
        syn::braced!(content in input);

        let mut routes = Vec::new();
        while !content.is_empty() {
            routes.push(parse_api_route(&content)?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }

        Ok(WaywardApiInput { vis, name, routes })
    }
}

fn parse_api_route(input: ParseStream) -> syn::Result<ApiRoute> {
    let method_ident: Ident = input.parse()?;
    let method = method_ident.to_string();

    let mut path = Vec::new();
    while !input.peek(Token![=>]) && !input.peek(syn::token::Bracket) {
        if !path.is_empty() {
            input.parse::<Token![/]>()?;
        }
        path.push(parse_one_segment(input)?);
    }

    let request = if input.peek(syn::token::Bracket) {
        let bracket_content;
        syn::bracketed!(bracket_content in input);
        Some(bracket_content.parse::<Type>()?)
    } else {
        None
    };

    input.parse::<Token![=>]>()?;
    let response: Type = input.parse()?;

    Ok(ApiRoute {
        method,
        path,
        request,
        response,
    })
}

// ---------------------------------------------------------------------------
// path! — lightweight type-position macro (for use in type aliases/binds)
// ---------------------------------------------------------------------------

/// Constructs a path type expression. Unlike `typeway_path!`, this does NOT
/// generate marker types — it references markers that were already defined
/// by a `typeway_path!` or `typeway_api!` invocation.
///
/// Not recommended for direct use — prefer `typeway_path!` which handles
/// both marker generation and type definition.
#[proc_macro]
pub fn path(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as PathRefInput);
    let empty_mod = quote! {};
    let hlist = build_hlist_type(&input.segments, &empty_mod);
    hlist.into()
}

struct PathRefInput {
    segments: Vec<PathSegment>,
}

impl Parse for PathRefInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let segments = parse_path_segments(input)?;
        Ok(PathRefInput { segments })
    }
}

// ---------------------------------------------------------------------------
// #[handler] attribute macro
// ---------------------------------------------------------------------------

/// Validates a handler function at its definition site.
///
/// Checks that:
/// - The function is `async`
/// - All arguments (except the last) implement `FromRequestParts`
/// - The last argument implements either `FromRequestParts` or `FromRequest`
/// - The return type implements `IntoResponse`
///
/// The function is emitted unchanged — it already works with [`bind`] and
/// the blanket `Handler<Args>` impls. This macro exists purely for early,
/// readable compile errors instead of cryptic trait-resolution failures at
/// the `Server::new` call site.
///
/// # Example
///
/// ```ignore
/// #[handler]
/// async fn get_user(path: Path<UserByIdPath>, state: State<AppState>) -> Json<User> {
///     // ...
/// }
/// ```
///
/// # Compile errors
///
/// ```ignore
/// #[handler]
/// fn not_async() -> String { "hello".to_string() }
/// // error: handler functions must be async
///
/// #[handler]
/// async fn bad_return() -> NotAResponse { NotAResponse }
/// // error: `NotAResponse` does not implement `IntoResponse`
/// ```
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _ = attr; // no attributes expected
    let func = match syn::parse::<syn::ItemFn>(item.clone()) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    // Must be async.
    if func.sig.asyncness.is_none() {
        return syn::Error::new_spanned(func.sig.fn_token, "handler functions must be async")
            .to_compile_error()
            .into();
    }

    let fn_name = &func.sig.ident;
    let check_mod = format_ident!("__wayward_check_{}", fn_name);

    // Collect typed arguments (skip self).
    let typed_args: Vec<&syn::PatType> = func
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => Some(pt),
            _ => None,
        })
        .collect();

    // Generate FromRequestParts assertions for all-but-last args,
    // and a FromRequestParts-or-FromRequest check for the last arg.
    let mut parts_checks = Vec::new();

    for (i, arg) in typed_args.iter().enumerate() {
        let ty = &arg.ty;
        if i < typed_args.len() - 1 {
            // Non-last args must be FromRequestParts.
            let assert_fn = format_ident!("__assert_parts_{}", i);
            let call_fn = format_ident!("__call_parts_{}", i);
            parts_checks.push(quote! {
                fn #assert_fn<T: ::typeway_server::FromRequestParts>() {}
                fn #call_fn() { #assert_fn::<#ty>(); }
            });
        }
        // Last arg: could be FromRequestParts or FromRequest.
        // We can't express an "or" bound, so the blanket Handler
        // impl catches type mismatches for the last argument.
    }

    // Return type must implement IntoResponse.
    let ret_ty = match &func.sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => quote! { #ty },
    };

    let expanded = quote! {
        #func

        #[doc(hidden)]
        #[allow(non_snake_case, unused, dead_code, unreachable_code)]
        mod #check_mod {
            use super::*;

            fn __check_response<T: ::typeway_server::IntoResponse>() {}

            fn __check_response_call() {
                __check_response::<#ret_ty>();
            }

            #(#parts_checks)*
        }
    };

    expanded.into()
}

// ---------------------------------------------------------------------------
// #[api_description] trait macro
// ---------------------------------------------------------------------------

/// Defines an API as a trait, generating endpoint types and a `Serves` bridge.
///
/// Each method in the trait is annotated with an HTTP method attribute (`#[get(...)]`,
/// `#[post(...)]`, etc.) that specifies the path. The macro generates:
///
/// 1. A type alias `<TraitName>Spec` — a tuple of endpoint types
/// 2. The original trait with async method signatures
/// 3. An `into_handlers` method that produces a handler tuple for `Server::new`
///
/// # Example
///
/// ```ignore
/// #[api_description]
/// trait UserAPI {
///     #[get("users" / u32)]
///     async fn get_user(path: Path<UserByIdPath>) -> Json<User>;
///
///     #[post("users")]
///     async fn create_user(body: Json<CreateUser>) -> Json<User>;
/// }
///
/// struct MyImpl { db: DbPool }
/// impl UserAPI for MyImpl {
///     async fn get_user(path: Path<UserByIdPath>) -> Json<User> {
///         let user = User { id: path.id, name: "Alice".into() };
///         Json(user)
///     }
///     async fn create_user(body: Json<CreateUser>) -> Json<User> {
///         let user = User { id: 1, name: body.0.name.clone() };
///         Json(user)
///     }
/// }
///
/// // Use: serve_user_api() bridges the trait impl to Server::new
/// Server::<UserAPISpec>::new(serve_user_api(MyImpl));
/// ```
#[proc_macro_attribute]
pub fn api_description(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _ = attr;
    let trait_def = match syn::parse::<syn::ItemTrait>(item) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error().into(),
    };

    match api_description_impl(trait_def) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn api_description_impl(trait_def: syn::ItemTrait) -> syn::Result<TokenStream2> {
    let trait_name = &trait_def.ident;
    let trait_vis = &trait_def.vis;
    let spec_name = format_ident!("{}Spec", trait_name);
    let handlers_fn = format_ident!("serve_{}", to_snake_case(&trait_name.to_string()));
    let markers_mod = format_ident!("__wa_desc_{}", trait_name);

    // Parse each method and its route attribute.
    let mut routes = Vec::new();
    let mut clean_methods = Vec::new();

    for item in &trait_def.items {
        let method = match item {
            syn::TraitItem::Fn(m) => m,
            other => {
                clean_methods.push(quote! { #other });
                continue;
            }
        };

        // Find and parse the route attribute (#[get(...)], #[post(...)], etc.).
        let (http_method, path_segments, remaining_attrs) = parse_route_attr(method)?;

        let sig = &method.sig;
        if sig.asyncness.is_none() {
            return Err(syn::Error::new_spanned(
                sig.fn_token,
                "api_description methods must be async",
            ));
        }

        // Emit clean method (without the route attribute).
        // - Inject `&self` as the first parameter if not already present.
        // - Desugar `async fn` to `fn -> impl Future<Output = T> + Send`
        //   so the trait is object-safe and futures are Send.
        let default_body = &method.default;
        let mut clean_sig = method.sig.clone();
        let has_self = clean_sig
            .inputs
            .iter()
            .any(|arg| matches!(arg, syn::FnArg::Receiver(_)));
        if !has_self {
            clean_sig.inputs.insert(0, syn::parse_quote! { &self });
        }
        // Desugar async fn to fn -> impl Future + Send.
        if clean_sig.asyncness.is_some() {
            clean_sig.asyncness = None;
            let ret_ty = match &clean_sig.output {
                syn::ReturnType::Default => quote! { () },
                syn::ReturnType::Type(_, ty) => quote! { #ty },
            };
            clean_sig.output = syn::parse_quote! {
                -> impl ::std::future::Future<Output = #ret_ty> + Send
            };
        }
        let semi = if default_body.is_none() {
            quote! { ; }
        } else {
            quote! {}
        };
        clean_methods.push(quote! {
            #(#remaining_attrs)*
            #clean_sig #default_body #semi
        });

        routes.push(ParsedRoute {
            method_name: sig.ident.clone(),
            http_method,
            path_segments,
            sig: sig.clone(),
        });
    }

    // Collect all literal marker types.
    let mut seen = std::collections::HashSet::new();
    let mut all_marker_defs = Vec::new();
    for route in &routes {
        all_marker_defs.extend(collect_marker_defs(&route.path_segments, &mut seen));
    }

    let mod_path: TokenStream2 = quote! { #markers_mod:: };

    // Build endpoint types and path type aliases for each route.
    let mut endpoint_types = Vec::new();
    let mut path_type_aliases = Vec::new();
    for route in &routes {
        let path_type = build_hlist_type(&route.path_segments, &mod_path);
        let method_type = method_type_ident(&route.http_method);

        // Generate a path type alias named after the method (e.g., get_user -> GetUserPath).
        let path_alias = format_ident!("{}Path", to_pascal_case(&route.method_name.to_string()));
        path_type_aliases.push(quote! {
            #trait_vis type #path_alias = #path_type;
        });

        // Extract request body type and response type from the signature.
        let (req_type, res_type) = extract_req_res_types(&route.sig)?;

        let endpoint = match req_type {
            Some(req) => {
                quote! { ::typeway_core::Endpoint<::typeway_core::#method_type, #path_type, #req, #res_type> }
            }
            None => {
                quote! { ::typeway_core::Endpoint<::typeway_core::#method_type, #path_type, ::typeway_core::NoBody, #res_type> }
            }
        };
        endpoint_types.push(endpoint);
    }

    // Generate the into_handlers function.
    // For each route, create a closure that calls the trait method.
    let impl_clones: Vec<TokenStream2> = routes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let clone_name = format_ident!("__impl_{}", i);
            quote! { let #clone_name = __impl.clone(); }
        })
        .collect();

    let handler_binds: Vec<TokenStream2> = routes
        .iter()
        .enumerate()
        .map(|(i, route)| {
            let method_name = &route.method_name;
            let ep_type = &endpoint_types[i];
            let clone_name = format_ident!("__impl_{}", i);

            // Collect typed arguments (skip &self receivers).
            let args: Vec<&syn::PatType> = route
                .sig
                .inputs
                .iter()
                .filter_map(|arg| match arg {
                    syn::FnArg::Typed(pt) => Some(pt),
                    _ => None,
                })
                .collect();

            let arg_pats: Vec<&syn::Pat> = args.iter().map(|a| a.pat.as_ref()).collect();
            let arg_types: Vec<&syn::Type> = args.iter().map(|a| a.ty.as_ref()).collect();

            quote! {
                ::typeway_server::bind::<#ep_type, _, _>(
                    move |#(#arg_pats: #arg_types),*| {
                        let __self = #clone_name.clone();
                        async move {
                            __self.#method_name(#(#arg_pats),*).await
                        }
                    }
                )
            }
        })
        .collect();

    // Supertraits of the original trait.
    let supertraits = &trait_def.supertraits;
    let colon_token = &trait_def.colon_token;

    let expanded = quote! {
        // Marker types for literal path segments.
        #[doc(hidden)]
        #[allow(non_snake_case, non_camel_case_types)]
        mod #markers_mod {
            #(#all_marker_defs)*
        }

        // Path type aliases for each route (e.g., GetUserPath, CreateUserPath).
        #(#path_type_aliases)*

        // The API spec type alias.
        #trait_vis type #spec_name = (#(#endpoint_types,)*);

        // The trait itself (with route attributes stripped).
        #trait_vis trait #trait_name #colon_token #supertraits {
            #(#clean_methods)*
        }

        // Bridge: convert a trait impl into bound handlers for Server::new.
        //
        // Usage: `Server::<UserAPISpec>::new(serve_user_api(my_impl))`
        #trait_vis fn #handlers_fn<T>(
            __impl: T,
        ) -> (#(::typeway_server::BoundHandler<#endpoint_types>,)*)
        where
            T: #trait_name + Clone + Send + Sync + 'static,
        {
            #(#impl_clones)*
            (#(#handler_binds,)*)
        }
    };

    Ok(expanded)
}

/// Convert snake_case to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Convert PascalCase to snake_case, handling acronyms correctly.
/// "UserAPI" -> "user_api", "HTMLParser" -> "html_parser"
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                let prev_upper = chars[i - 1].is_uppercase();
                let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
                // Insert underscore before: a new uppercase word, or the last letter
                // of an acronym followed by a lowercase letter.
                if !prev_upper || next_lower {
                    result.push('_');
                }
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

struct ParsedRoute {
    method_name: Ident,
    http_method: String,
    path_segments: Vec<PathSegment>,
    sig: syn::Signature,
}

/// Parse the `#[get(...)]`, `#[post(...)]`, etc. attribute from a trait method.
/// Returns (http_method, path_segments, remaining_attrs).
fn parse_route_attr(
    method: &syn::TraitItemFn,
) -> syn::Result<(String, Vec<PathSegment>, Vec<syn::Attribute>)> {
    let route_methods = ["get", "post", "put", "delete", "patch", "head", "options"];
    let mut http_method = None;
    let mut path_segments = None;
    let mut remaining_attrs = Vec::new();

    for attr in &method.attrs {
        let ident = attr.path().get_ident();
        if let Some(id) = ident {
            let name = id.to_string();
            if route_methods.contains(&name.as_str()) {
                if http_method.is_some() {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "only one route attribute per method",
                    ));
                }
                http_method = Some(name.to_uppercase());
                // Parse the attribute arguments as path segments.
                let segments: Vec<PathSegment> = attr.parse_args_with(parse_path_segments)?;
                path_segments = Some(segments);
                continue;
            }
        }
        remaining_attrs.push(attr.clone());
    }

    match (http_method, path_segments) {
        (Some(m), Some(p)) => Ok((m, p, remaining_attrs)),
        _ => Err(syn::Error::new_spanned(
            &method.sig.ident,
            "api_description methods must have a route attribute: #[get(...)], #[post(...)], etc.",
        )),
    }
}

/// Extract request body type and response type from a method signature.
///
/// The response type is the return type. The request body type is the last
/// argument if the HTTP method supports a body (POST, PUT, PATCH) and the
/// last argument looks like a body extractor (Json<T>, Bytes, String).
/// For simplicity, we always treat the return type as the response and
/// don't try to extract the body type from the signature — that's determined
/// by the endpoint type parameters and the Handler impls at the server level.
fn extract_req_res_types(
    sig: &syn::Signature,
) -> syn::Result<(Option<TokenStream2>, TokenStream2)> {
    let res_type = match &sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => quote! { #ty },
    };

    // For the request type, we don't infer it from args — it's NoBody by default.
    // Users should specify body types via the endpoint type if needed.
    // The macro primarily provides ergonomic trait-based API definitions.
    Ok((None, res_type))
}

// ---------------------------------------------------------------------------
// endpoint! — builder-style endpoint type macro
// ---------------------------------------------------------------------------

/// Defines an endpoint type with builder-style options.
///
/// Desugars nested wrappers (`Protected`, `Validated`, `Strict`, etc.)
/// into a single readable declaration.
///
/// # Syntax
///
/// ```ignore
/// endpoint! {
///     GET "users" / u32 => Json<User>,
///     auth: AuthUser,
///     errors: JsonError,
///     strict: true,
/// }
///
/// endpoint! {
///     POST "users",
///     body: CreateUser => Json<User>,
///     auth: AuthUser,
///     validate: CreateUserValidator,
///     content_type: json,
///     errors: JsonError,
///     version: V1,
/// }
/// ```
///
/// # Fields
///
/// - Method + path + `=>` response (required)
/// - `body:` request body type (for POST/PUT/PATCH)
/// - `auth:` wraps in `Protected<Auth, _>`
/// - `validate:` wraps in `Validated<V, _>`
/// - `content_type:` wraps in `ContentType<C, _>` (`json` or a type)
/// - `errors:` sets the `Err` type parameter
/// - `version:` wraps in `Versioned<V, _>`
/// - `strict:` wraps in `Strict<_>` (if `true`)
/// - `rate_limit:` wraps in `RateLimited<R, _>`
#[proc_macro]
pub fn endpoint(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as EndpointInput);
    match endpoint_impl(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct EndpointInput {
    method: String,
    path_type: Type,
    body_type: Option<Type>,
    response_type: Type,
    auth: Option<Type>,
    validate: Option<Type>,
    content_type: Option<Type>,
    errors: Option<Type>,
    version: Option<Type>,
    strict: bool,
    rate_limit: Option<Type>,
}

impl Parse for EndpointInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse method
        let method_ident: Ident = input.parse()?;
        let method = method_ident.to_string().to_uppercase();

        // Parse path type (a named type, not segments)
        let path_type: Type = input.parse()?;

        // Parse => Response or , body: ... => Response
        let mut body_type = None;
        let response_type;

        if input.peek(Token![=>]) {
            // GET PathType => Response
            input.parse::<Token![=>]>()?;
            response_type = input.parse::<Type>()?;
        } else if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            // Look for body: ... => Response
            let key: Ident = input.parse()?;
            if key != "body" {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `=> Response` or `body: Type => Response`",
                ));
            }
            input.parse::<Token![:]>()?;
            body_type = Some(input.parse::<Type>()?);
            input.parse::<Token![=>]>()?;
            response_type = input.parse::<Type>()?;
        } else {
            return Err(input.error("expected `=>` or `,`"));
        }

        // Consume trailing comma
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }

        // Parse optional key: value fields
        let mut auth = None;
        let mut validate = None;
        let mut content_type = None;
        let mut errors = None;
        let mut version = None;
        let mut strict = false;
        let mut rate_limit = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "auth" => auth = Some(input.parse::<Type>()?),
                "validate" => validate = Some(input.parse::<Type>()?),
                "content_type" => {
                    if input.peek(Ident) {
                        let ct: Ident = input.parse()?;
                        content_type = Some(match ct.to_string().as_str() {
                            "json" => syn::parse_quote! { ::typeway_server::typed::JsonContent },
                            "form" => syn::parse_quote! { ::typeway_server::typed::FormContent },
                            _ => {
                                return Err(syn::Error::new(
                                    ct.span(),
                                    "expected `json`, `form`, or a type",
                                ))
                            }
                        });
                    } else {
                        content_type = Some(input.parse::<Type>()?);
                    }
                }
                "errors" => errors = Some(input.parse::<Type>()?),
                "version" => version = Some(input.parse::<Type>()?),
                "strict" => {
                    let v: syn::LitBool = input.parse()?;
                    strict = v.value;
                }
                "rate_limit" => rate_limit = Some(input.parse::<Type>()?),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown field `{other}`"),
                    ))
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(EndpointInput {
            method,
            path_type,
            body_type,
            response_type,
            auth,
            validate,
            content_type,
            errors,
            version,
            strict,
            rate_limit,
        })
    }
}

fn endpoint_impl(input: EndpointInput) -> syn::Result<TokenStream2> {
    let path_type = &input.path_type;
    let method_type = method_type_ident(&input.method);
    let response_type = &input.response_type;

    let (req_type, q_type, err_type) = {
        let req = match &input.body_type {
            Some(t) => quote! { #t },
            None => quote! { ::typeway_core::NoBody },
        };
        let q = quote! { () };
        let err = match &input.errors {
            Some(t) => quote! { #t },
            None => quote! { () },
        };
        (req, q, err)
    };

    let mut result = quote! {
        ::typeway_core::Endpoint<
            ::typeway_core::#method_type,
            #path_type,
            #req_type,
            #response_type,
            #q_type,
            #err_type
        >
    };

    // Apply wrappers inside-out:
    // strict → content_type → validate → rate_limit → version → auth
    // (auth is outermost so it's checked first at runtime)

    if input.strict {
        result = quote! { ::typeway_server::typed_response::Strict<#result> };
    }

    if let Some(ref ct) = input.content_type {
        result = quote! { ::typeway_server::typed::ContentType<#ct, #result> };
    }

    if let Some(ref v) = input.validate {
        result = quote! { ::typeway_server::typed::Validated<#v, #result> };
    }

    if let Some(ref r) = input.rate_limit {
        result = quote! { ::typeway_server::typed::RateLimited<#r, #result> };
    }

    if let Some(ref v) = input.version {
        result = quote! { ::typeway_server::typed::Versioned<#v, #result> };
    }

    if let Some(ref auth) = input.auth {
        result = quote! { ::typeway_server::auth::Protected<#auth, #result> };
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// #[documented_handler] attribute macro
// ---------------------------------------------------------------------------

/// Extracts doc comments from a handler function and generates a companion
/// `const` of type [`HandlerDoc`](typeway_core::HandlerDoc) containing the
/// summary, description, operation ID, and tags.
///
/// The first line of the doc comment becomes the `summary`. All subsequent
/// non-empty lines become the `description`. The function name becomes the
/// `operation_id`. Tags can be specified via the attribute parameter.
///
/// # Generated output
///
/// For a function named `list_users`, the macro generates a constant named
/// `LIST_USERS_DOC` of type `typeway_core::HandlerDoc`.
///
/// # Example
///
/// ```ignore
/// /// List all users.
/// ///
/// /// Returns a paginated list of users with optional filtering.
/// #[documented_handler(tags = "users")]
/// async fn list_users(state: State<Db>) -> Json<Vec<User>> {
///     // ...
/// }
///
/// // Generated:
/// // pub const LIST_USERS_DOC: typeway_core::HandlerDoc = typeway_core::HandlerDoc {
/// //     summary: "List all users.",
/// //     description: "Returns a paginated list of users with optional filtering.",
/// //     operation_id: "list_users",
/// //     tags: &["users"],
/// // };
/// ```
///
/// # Tags
///
/// Multiple tags can be comma-separated:
///
/// ```ignore
/// #[documented_handler(tags = "users, admin")]
/// ```
#[proc_macro_attribute]
pub fn documented_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = match syn::parse::<syn::ItemFn>(item.clone()) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    let tags = parse_documented_handler_tags(attr.into());

    // Extract doc comment lines from #[doc = "..."] attributes.
    let doc_lines: Vec<String> = func
        .attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(lit) = &nv.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        return Some(s.value());
                    }
                }
            }
            None
        })
        .collect();

    // First non-empty trimmed line is summary, rest is description.
    let trimmed: Vec<String> = doc_lines.iter().map(|l| l.trim().to_string()).collect();

    let summary = trimmed
        .iter()
        .find(|l| !l.is_empty())
        .cloned()
        .unwrap_or_default();

    // Description: everything after the first non-empty line, with leading
    // blank lines stripped, then joined with newlines.
    let description = {
        let after_summary: Vec<&str> = trimmed
            .iter()
            .skip_while(|l| l.is_empty()) // skip leading blanks
            .skip(1) // skip the summary line
            .map(|s| s.as_str())
            .collect();
        // Trim leading and trailing empty lines from the description.
        let start = after_summary.iter().position(|l| !l.is_empty());
        let end = after_summary.iter().rposition(|l| !l.is_empty());
        match (start, end) {
            (Some(s), Some(e)) => after_summary[s..=e].join("\n"),
            _ => String::new(),
        }
    };

    let fn_name = &func.sig.ident;
    let const_name = format_ident!("{}_DOC", to_screaming_snake(&fn_name.to_string()));
    let operation_id = fn_name.to_string();

    let tags_tokens: Vec<TokenStream2> = tags.iter().map(|t| quote! { #t }).collect();
    let tags_array = if tags_tokens.is_empty() {
        quote! { &[] }
    } else {
        quote! { &[#(#tags_tokens),*] }
    };

    let expanded = quote! {
        #func

        /// Auto-generated handler documentation metadata.
        pub const #const_name: ::typeway_core::HandlerDoc = ::typeway_core::HandlerDoc {
            summary: #summary,
            description: #description,
            operation_id: #operation_id,
            tags: #tags_array,
        };
    };

    expanded.into()
}

/// Parse `tags = "foo, bar"` from the attribute arguments.
fn parse_documented_handler_tags(attr: TokenStream2) -> Vec<String> {
    // Try to parse as `tags = "..."`.
    struct TagsAttr {
        tags: Vec<String>,
    }

    impl Parse for TagsAttr {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            if input.is_empty() {
                return Ok(TagsAttr { tags: Vec::new() });
            }
            let key: Ident = input.parse()?;
            if key != "tags" {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `tags = \"...\"`",
                ));
            }
            input.parse::<Token![=]>()?;
            let value: LitStr = input.parse()?;
            let tags = value
                .value()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(TagsAttr { tags })
        }
    }

    syn::parse2::<TagsAttr>(attr)
        .map(|t| t.tags)
        .unwrap_or_default()
}

/// Convert snake_case to SCREAMING_SNAKE_CASE.
fn to_screaming_snake(s: &str) -> String {
    s.to_uppercase()
}

// ---------------------------------------------------------------------------
// #[derive(TypewaySchema)] — OpenAPI schema derivation from struct definitions
// ---------------------------------------------------------------------------

/// Derives a `ToSchema` implementation for a struct with named fields.
///
/// Struct-level and field-level doc comments become `description` values in the
/// generated OpenAPI schema. Supports `#[serde(rename_all = "...")]` on the
/// struct and `#[serde(rename = "...")]` on individual fields.
///
/// # Example
///
/// ```ignore
/// /// A user account.
/// #[derive(TypewaySchema)]
/// struct User {
///     /// The unique user identifier.
///     id: u32,
///     /// The user's display name.
///     name: String,
/// }
/// ```
///
/// Generates an `impl typeway_openapi::ToSchema for User` that returns an
/// object schema with `id` and `name` properties, each carrying its doc
/// comment as a description.
///
/// # Serde rename support
///
/// ```ignore
/// #[derive(TypewaySchema)]
/// #[serde(rename_all = "camelCase")]
/// struct Article {
///     article_title: String,
///     tag_list: Vec<String>,
/// }
/// ```
///
/// The generated schema uses `articleTitle` and `tagList` as property names.
/// Per-field `#[serde(rename = "...")]` overrides `rename_all`.
///
/// Supported rename strategies: `camelCase`, `snake_case`, `PascalCase`,
/// `SCREAMING_SNAKE_CASE`, `kebab-case`.
#[proc_macro_derive(TypewaySchema)]
pub fn derive_typeway_schema(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match derive_typeway_schema_impl(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_typeway_schema_impl(input: syn::DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let name_str = name.to_string();

    // Extract struct-level doc comment.
    let struct_doc = extract_doc_string(&input.attrs);

    // Check for #[serde(rename_all = "...")].
    let rename_all = extract_serde_rename_all(&input.attrs);

    // Get the struct fields.
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) => &named.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    name,
                    "TypewaySchema only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "TypewaySchema only supports structs",
            ));
        }
    };

    // Generate property entries as (name_str, schema) pairs.
    let property_entries: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            let field_type = &field.ty;

            // Determine the serialized field name.
            let field_name_str =
                if let Some(rename) = extract_serde_field_rename(&field.attrs) {
                    rename
                } else if let Some(ref strategy) = rename_all {
                    apply_rename_strategy(&field_ident.to_string(), strategy)
                } else {
                    field_ident.to_string()
                };

            let field_doc = extract_doc_string(&field.attrs);

            match field_doc {
                Some(doc) => quote! {
                    (#field_name_str, <#field_type as ::typeway_openapi::ToSchema>::schema()
                        .with_description(#doc))
                },
                None => quote! {
                    (#field_name_str, <#field_type as ::typeway_openapi::ToSchema>::schema())
                },
            }
        })
        .collect();

    let struct_description = match struct_doc {
        Some(doc) => quote! { Some(#doc) },
        None => quote! { None },
    };

    let expanded = quote! {
        impl ::typeway_openapi::ToSchema for #name {
            fn schema() -> ::typeway_openapi::spec::Schema {
                use ::typeway_openapi::spec::Schema as __Schema;
                __Schema::object_with_properties(
                    vec![#(#property_entries),*],
                    #struct_description,
                )
            }

            fn type_name() -> &'static str {
                #name_str
            }
        }
    };

    Ok(expanded)
}

/// Extract the combined doc comment string from `#[doc = "..."]` attributes.
fn extract_doc_string(attrs: &[syn::Attribute]) -> Option<String> {
    let doc_lines: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(lit) = &nv.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        return Some(s.value().trim().to_string());
                    }
                }
            }
            None
        })
        .filter(|s| !s.is_empty())
        .collect();

    if doc_lines.is_empty() {
        None
    } else {
        Some(doc_lines.join("\n"))
    }
}

/// Extract `rename_all` value from `#[serde(rename_all = "...")]`.
fn extract_serde_rename_all(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let mut result = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result = Some(lit.value());
            }
            Ok(())
        });
        if result.is_some() {
            return result;
        }
    }
    None
}

/// Extract `rename` value from `#[serde(rename = "...")]` on a field.
fn extract_serde_field_rename(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let mut result = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result = Some(lit.value());
            }
            Ok(())
        });
        if result.is_some() {
            return result;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// #[derive(ToProtoType)] — protobuf message derivation from struct definitions
// ---------------------------------------------------------------------------

/// Derives a `ToProtoType` implementation for a struct with named fields.
///
/// Each field is mapped to a [`ProtoField`](typeway_grpc::ProtoField) entry.
/// Field tags can be specified explicitly with `#[proto(tag = N)]`; fields
/// without an explicit tag are auto-numbered based on their 1-indexed position.
///
/// `Option<T>` fields produce `optional` proto fields. `Vec<T>` fields produce
/// `repeated` proto fields (except `Vec<u8>`, which maps to `bytes`).
///
/// # Example
///
/// ```ignore
/// #[derive(ToProtoType)]
/// struct User {
///     #[proto(tag = 1)]
///     id: u32,
///     #[proto(tag = 2)]
///     name: String,
///     #[proto(tag = 3)]
///     bio: Option<String>,
/// }
/// ```
#[proc_macro_derive(ToProtoType, attributes(proto))]
pub fn derive_to_proto_type(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match derive_to_proto_type_impl(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_to_proto_type_impl(input: syn::DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let name_str = name.to_string();

    match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) => derive_to_proto_type_struct(name, name_str, &named.named),
            _ => Err(syn::Error::new_spanned(
                name,
                "ToProtoType only supports structs with named fields or enums",
            )),
        },
        syn::Data::Enum(data) => {
            let is_simple = data.variants.iter().all(|v| v.fields.is_empty());
            if is_simple {
                derive_to_proto_type_simple_enum(name, name_str, data)
            } else {
                derive_to_proto_type_oneof_enum(name, name_str, data)
            }
        }
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            name,
            "ToProtoType does not support unions",
        )),
    }
}

fn derive_to_proto_type_struct(
    name: &Ident,
    name_str: String,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> syn::Result<TokenStream2> {
    let mut field_entries = Vec::new();
    let mut collect_stmts = Vec::new();

    for (i, field) in fields.iter().enumerate() {
        let field_ident = field.ident.as_ref().unwrap();
        let field_name_str = field_ident.to_string();
        let field_ty = &field.ty;
        let tag = extract_proto_tag(&field.attrs).unwrap_or((i as u32) + 1);
        let field_doc = extract_doc_string(&field.attrs);

        // Detect Option<T>, Vec<T>, and HashMap<K,V>/BTreeMap<K,V> to set
        // optional/repeated/map and use the appropriate inner types.
        let (proto_type_ty, optional, repeated, is_map_field) =
            if let Some(inner) = is_option_type(field_ty) {
                (inner.clone(), true, false, false)
            } else if is_vec_u8(field_ty) {
                // Vec<u8> maps to bytes — use Vec<u8> directly, not repeated.
                (field_ty.clone(), false, false, false)
            } else if let Some(inner) = is_vec_type(field_ty) {
                (inner.clone(), false, true, false)
            } else if is_map_type(field_ty).is_some() {
                // Map types — use the original type so ToProtoType dispatches correctly.
                (field_ty.clone(), false, false, true)
            } else {
                (field_ty.clone(), false, false, false)
            };

        let doc_expr = match &field_doc {
            Some(doc) => quote! { ::core::option::Option::Some(#doc.to_string()) },
            None => quote! { ::core::option::Option::None },
        };

        let field_entry = if is_map_field {
            let (key_ty, val_ty) = is_map_type(field_ty).unwrap();
            quote! {
                ::typeway_grpc::ProtoField {
                    name: #field_name_str.to_string(),
                    proto_type: "map".to_string(),
                    tag: #tag,
                    repeated: false,
                    optional: false,
                    is_map: true,
                    map_key_type: ::core::option::Option::Some(
                        <#key_ty as ::typeway_grpc::ToProtoType>::proto_type_name().to_string()
                    ),
                    map_value_type: ::core::option::Option::Some(
                        <#val_ty as ::typeway_grpc::ToProtoType>::proto_type_name().to_string()
                    ),
                    doc: #doc_expr,
                }
            }
        } else {
            quote! {
                ::typeway_grpc::ProtoField {
                    name: #field_name_str.to_string(),
                    proto_type: <#proto_type_ty as ::typeway_grpc::ToProtoType>::proto_type_name().to_string(),
                    tag: #tag,
                    repeated: #repeated,
                    optional: #optional,
                    is_map: false,
                    map_key_type: ::core::option::Option::None,
                    map_value_type: ::core::option::Option::None,
                    doc: #doc_expr,
                }
            }
        };
        field_entries.push(field_entry);

        collect_stmts.push(quote! {
            msgs.extend(<#proto_type_ty as ::typeway_grpc::ToProtoType>::collect_messages());
        });
    }

    let expanded = quote! {
        impl ::typeway_grpc::ToProtoType for #name {
            fn proto_type_name() -> &'static str {
                #name_str
            }

            fn is_message() -> bool {
                true
            }

            fn message_definition() -> ::core::option::Option<::std::string::String> {
                ::core::option::Option::Some(::typeway_grpc::build_message(#name_str, &[
                    #(#field_entries),*
                ]))
            }

            fn collect_messages() -> ::std::vec::Vec<::std::string::String> {
                let mut msgs = ::std::vec::Vec::new();
                #(#collect_stmts)*
                if let ::core::option::Option::Some(def) = Self::message_definition() {
                    msgs.push(def);
                }
                msgs
            }

            fn proto_fields() -> ::std::vec::Vec<::typeway_grpc::ProtoField> {
                ::std::vec![#(#field_entries),*]
            }
        }
    };

    Ok(expanded)
}

/// Generate a `ToProtoType` impl for a simple (fieldless) enum as a protobuf enum.
fn derive_to_proto_type_simple_enum(
    name: &Ident,
    name_str: String,
    data: &syn::DataEnum,
) -> syn::Result<TokenStream2> {
    let mut variant_names = Vec::new();
    let mut variant_tags = Vec::new();

    for (i, variant) in data.variants.iter().enumerate() {
        let tag = extract_proto_tag(&variant.attrs).unwrap_or(i as u32);
        let proto_name = to_screaming_snake(&variant.ident.to_string());
        variant_names.push(proto_name);
        variant_tags.push(tag);
    }

    let expanded = quote! {
        impl ::typeway_grpc::ToProtoType for #name {
            fn proto_type_name() -> &'static str {
                #name_str
            }

            fn is_message() -> bool {
                true
            }

            fn message_definition() -> ::core::option::Option<::std::string::String> {
                let mut lines = ::std::vec![::std::format!("enum {} {{", #name_str)];
                #(
                    lines.push(::std::format!("  {} = {};", #variant_names, #variant_tags));
                )*
                lines.push("}".to_string());
                ::core::option::Option::Some(lines.join("\n"))
            }

            fn collect_messages() -> ::std::vec::Vec<::std::string::String> {
                let mut msgs = ::std::vec::Vec::new();
                if let ::core::option::Option::Some(def) = Self::message_definition() {
                    msgs.push(def);
                }
                msgs
            }
        }
    };

    Ok(expanded)
}

/// Generate a `ToProtoType` impl for a tagged enum as a protobuf `oneof` in a
/// wrapper message.
fn derive_to_proto_type_oneof_enum(
    name: &Ident,
    name_str: String,
    data: &syn::DataEnum,
) -> syn::Result<TokenStream2> {
    let oneof_name = to_snake_case(&name_str);

    let mut variant_field_names = Vec::new();
    let mut variant_types: Vec<syn::Type> = Vec::new();
    let mut variant_tags = Vec::new();
    let mut collect_stmts = Vec::new();

    for (i, variant) in data.variants.iter().enumerate() {
        let tag = extract_proto_tag(&variant.attrs).unwrap_or((i + 1) as u32);
        let field_name = to_snake_case(&variant.ident.to_string());
        variant_field_names.push(field_name);
        variant_tags.push(tag);

        match &variant.fields {
            syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let ty = fields.unnamed[0].ty.clone();
                collect_stmts.push(quote! {
                    msgs.extend(<#ty as ::typeway_grpc::ToProtoType>::collect_messages());
                });
                variant_types.push(ty);
            }
            syn::Fields::Unnamed(_) => {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "ToProtoType oneof variants must have exactly one field",
                ));
            }
            syn::Fields::Named(_) => {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "ToProtoType oneof variants must use tuple syntax, e.g., Variant(Type)",
                ));
            }
            syn::Fields::Unit => {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "mixed unit and data variants are not supported; \
                     all variants must have fields for oneof generation",
                ));
            }
        }
    }

    let expanded = quote! {
        impl ::typeway_grpc::ToProtoType for #name {
            fn proto_type_name() -> &'static str {
                #name_str
            }

            fn is_message() -> bool {
                true
            }

            fn message_definition() -> ::core::option::Option<::std::string::String> {
                let mut lines = ::std::vec![::std::format!("message {} {{", #name_str)];
                lines.push(::std::format!("  oneof {} {{", #oneof_name));
                #(
                    lines.push(::std::format!("    {} {} = {};",
                        <#variant_types as ::typeway_grpc::ToProtoType>::proto_type_name(),
                        #variant_field_names,
                        #variant_tags,
                    ));
                )*
                lines.push("  }".to_string());
                lines.push("}".to_string());
                ::core::option::Option::Some(lines.join("\n"))
            }

            fn collect_messages() -> ::std::vec::Vec<::std::string::String> {
                let mut msgs = ::std::vec::Vec::new();
                #(#collect_stmts)*
                if let ::core::option::Option::Some(def) = Self::message_definition() {
                    msgs.push(def);
                }
                msgs
            }
        }
    };

    Ok(expanded)
}

/// Extract a `#[proto(tag = N)]` attribute value from field attributes.
fn extract_proto_tag(attrs: &[syn::Attribute]) -> Option<u32> {
    for attr in attrs {
        if attr.path().is_ident("proto") {
            if let Ok(meta) = attr.parse_args::<syn::MetaNameValue>() {
                if meta.path.is_ident("tag") {
                    if let syn::Expr::Lit(lit) = &meta.value {
                        if let syn::Lit::Int(int) = &lit.lit {
                            return int.base10_parse().ok();
                        }
                    }
                }
            }
        }
    }
    None
}

/// If the type is `Option<T>`, return `Some(T)`.
fn is_option_type(ty: &syn::Type) -> Option<&syn::Type> {
    if let syn::Type::Path(path) = ty {
        if let Some(seg) = path.path.segments.last() {
            if seg.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return Some(inner);
                    }
                }
            }
        }
    }
    None
}

/// If the type is `Vec<T>`, return `Some(T)`.
fn is_vec_type(ty: &syn::Type) -> Option<&syn::Type> {
    if let syn::Type::Path(path) = ty {
        if let Some(seg) = path.path.segments.last() {
            if seg.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return Some(inner);
                    }
                }
            }
        }
    }
    None
}

/// Check if the type is `Vec<u8>` (which maps to protobuf `bytes`).
fn is_vec_u8(ty: &syn::Type) -> bool {
    if let syn::Type::Path(path) = ty {
        if let Some(seg) = path.path.segments.last() {
            if seg.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(syn::Type::Path(inner_path))) =
                        args.args.first()
                    {
                        if let Some(inner_seg) = inner_path.path.segments.last() {
                            return inner_seg.ident == "u8"
                                && inner_seg.arguments.is_none();
                        }
                    }
                }
            }
        }
    }
    false
}

/// If the type is `HashMap<K, V>` or `BTreeMap<K, V>`, return `Some((K, V))`.
fn is_map_type(ty: &syn::Type) -> Option<(syn::Type, syn::Type)> {
    if let syn::Type::Path(path) = ty {
        if let Some(seg) = path.path.segments.last() {
            if seg.ident == "HashMap" || seg.ident == "BTreeMap" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    let mut types = args.args.iter().filter_map(|a| {
                        if let syn::GenericArgument::Type(t) = a {
                            Some(t)
                        } else {
                            None
                        }
                    });
                    if let (Some(k), Some(v)) = (types.next(), types.next()) {
                        return Some((k.clone(), v.clone()));
                    }
                }
            }
        }
    }
    None
}

/// Apply a serde rename strategy to a snake_case field name.
fn apply_rename_strategy(name: &str, strategy: &str) -> String {
    match strategy {
        "camelCase" => {
            let mut result = String::new();
            let mut capitalize_next = false;
            for c in name.chars() {
                if c == '_' {
                    capitalize_next = true;
                } else if capitalize_next {
                    result.extend(c.to_uppercase());
                    capitalize_next = false;
                } else {
                    result.push(c);
                }
            }
            result
        }
        "snake_case" => name.to_string(),
        "PascalCase" => name
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().to_string() + &c.collect::<String>(),
                    None => String::new(),
                }
            })
            .collect(),
        "SCREAMING_SNAKE_CASE" => name.to_uppercase(),
        "kebab-case" => name.replace('_', "-"),
        _ => name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// #[derive(TypewayCodec)] — compile-time specialized protobuf encode/decode
// ---------------------------------------------------------------------------

/// Derive `TypewayEncode` and `TypewayDecode` for a struct.
///
/// Generates specialized encode/decode functions with no runtime dispatch.
/// Each field is encoded/decoded directly based on its Rust type and proto
/// tag number.
///
/// # Attributes
///
/// - `#[proto(tag = N)]` — Set the protobuf field tag number (default: 1-indexed position)
///
/// # Supported field types
///
/// | Rust type | Proto type | Wire type |
/// |-----------|-----------|-----------|
/// | `u32` | uint32 | varint (0) |
/// | `u64` | uint64 | varint (0) |
/// | `i32` | int32 | varint (0) |
/// | `i64` | int64 | varint (0) |
/// | `bool` | bool | varint (0) |
/// | `f32` | float | 32-bit (5) |
/// | `f64` | double | 64-bit (1) |
/// | `String` | string | len-delimited (2) |
/// | `Vec<u8>` | bytes | len-delimited (2) |
/// | `Vec<T>` | repeated T | (varies) |
/// | `Option<T>` | optional T | (varies) |
///
/// # Example
///
/// ```ignore
/// #[derive(TypewayCodec)]
/// struct User {
///     #[proto(tag = 1)]
///     id: u32,
///     #[proto(tag = 2)]
///     name: String,
///     #[proto(tag = 3)]
///     active: bool,
/// }
/// ```
#[proc_macro_derive(TypewayCodec, attributes(proto))]
pub fn derive_typeway_codec(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match derive_typeway_codec_impl(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_typeway_codec_impl(input: syn::DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) => derive_typeway_codec_struct(name, &named.named),
            _ => Err(syn::Error::new_spanned(
                name,
                "TypewayCodec only supports structs with named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            name,
            "TypewayCodec only supports structs (not enums or unions)",
        )),
    }
}

/// Information about a single field for codec generation.
struct CodecField {
    ident: Ident,
    ty: syn::Type,
    tag: u32,
    codec_kind: CodecKind,
}

/// What kind of encoding a field needs.
enum CodecKind {
    /// Varint (wire type 0): u32, u64, i32, i64
    Varint,
    /// Bool (wire type 0, but needs special encode/decode)
    Bool,
    /// Fixed 32-bit (wire type 5): f32
    Fixed32,
    /// Fixed 64-bit (wire type 1): f64
    Fixed64,
    /// Length-delimited String (wire type 2)
    LenString,
    /// Length-delimited BytesStr (wire type 2, zero-copy decode)
    LenBytesStr,
    /// Length-delimited bytes (wire type 2)
    LenBytes,
    /// Optional wrapper around another kind
    Optional(Box<CodecKind>),
    /// Repeated (Vec<T>) wrapper — element kind + element type for iteration
    Repeated(Box<CodecKind>),
    /// Nested message that also implements TypewayEncode/TypewayDecode
    Message,
    /// Optional nested message
    OptionalMessage,
    /// Repeated nested message
    RepeatedMessage,
}

fn classify_type(ty: &syn::Type) -> CodecKind {
    if let Some(inner) = is_option_type(ty) {
        let inner_kind = classify_type(inner);
        match inner_kind {
            CodecKind::Message => CodecKind::OptionalMessage,
            other => CodecKind::Optional(Box::new(other)),
        }
    } else if is_vec_u8(ty) {
        CodecKind::LenBytes
    } else if let Some(inner) = is_vec_type(ty) {
        let inner_kind = classify_type(inner);
        match inner_kind {
            CodecKind::Message => CodecKind::RepeatedMessage,
            other => CodecKind::Repeated(Box::new(other)),
        }
    } else {
        classify_scalar(ty)
    }
}

fn classify_scalar(ty: &syn::Type) -> CodecKind {
    let ty_str = quote!(#ty).to_string().replace(' ', "");
    match ty_str.as_str() {
        "u32" | "u64" | "i32" | "i64" => CodecKind::Varint,
        "bool" => CodecKind::Bool,
        "f32" => CodecKind::Fixed32,
        "f64" => CodecKind::Fixed64,
        "String" => CodecKind::LenString,
        "BytesStr" | "typeway_protobuf::BytesStr" => CodecKind::LenBytesStr,
        _ => CodecKind::Message,
    }
}

fn wire_type_for_kind(kind: &CodecKind) -> u8 {
    match kind {
        CodecKind::Varint | CodecKind::Bool => 0,
        CodecKind::Fixed64 => 1,
        CodecKind::LenString | CodecKind::LenBytesStr | CodecKind::LenBytes | CodecKind::Message => 2,
        CodecKind::Fixed32 => 5,
        CodecKind::Optional(inner) | CodecKind::Repeated(inner) => wire_type_for_kind(inner),
        CodecKind::OptionalMessage | CodecKind::RepeatedMessage => 2,
    }
}

fn derive_typeway_codec_struct(
    name: &Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> syn::Result<TokenStream2> {
    // Parse fields.
    let mut codec_fields = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let ident = field.ident.clone().unwrap();
        let tag = extract_proto_tag(&field.attrs).unwrap_or((i as u32) + 1);
        let codec_kind = classify_type(&field.ty);
        codec_fields.push(CodecField {
            ident,
            ty: field.ty.clone(),
            tag,
            codec_kind,
        });
    }

    // Generate encode_to body.
    let encode_stmts: Vec<TokenStream2> = codec_fields
        .iter()
        .map(gen_encode_field)
        .collect();

    // Generate encoded_len body.
    let len_stmts: Vec<TokenStream2> = codec_fields
        .iter()
        .map(gen_encoded_len_field)
        .collect();

    // Generate decode body.
    let field_defaults: Vec<TokenStream2> = codec_fields
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let ty = &f.ty;
            quote! { let mut #ident: #ty = ::core::default::Default::default(); }
        })
        .collect();

    let decode_arms: Vec<TokenStream2> = codec_fields
        .iter()
        .map(gen_decode_arm)
        .collect();

    let decode_bytes_arms: Vec<TokenStream2> = codec_fields
        .iter()
        .map(gen_decode_bytes_arm)
        .collect();

    let field_names: Vec<&Ident> = codec_fields.iter().map(|f| &f.ident).collect();

    Ok(quote! {
        impl ::typeway_protobuf::TypewayEncode for #name {
            fn encoded_len(&self) -> usize {
                let mut len: usize = 0;
                #(#len_stmts)*
                len
            }

            fn encode_to(&self, buf: &mut ::std::vec::Vec<u8>) {
                #(#encode_stmts)*
            }
        }

        impl ::typeway_protobuf::TypewayDecode for #name {
            fn typeway_decode(
                bytes: &[u8],
            ) -> ::core::result::Result<Self, ::typeway_protobuf::TypewayDecodeError> {
                #(#field_defaults)*
                let mut offset: usize = 0;

                while offset < bytes.len() {
                    let (tag_wire, consumed) =
                        ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                    offset += consumed;
                    let field_number = (tag_wire >> 3) as u32;
                    let wire_type = (tag_wire & 0x07) as u8;

                    match field_number {
                        #(#decode_arms)*
                        _ => {
                            let skipped =
                                ::typeway_protobuf::tw_skip_wire_value(&bytes[offset..], wire_type)?;
                            offset += skipped;
                        }
                    }
                }

                Ok(#name { #(#field_names),* })
            }

            fn typeway_decode_bytes(
                input: ::bytes::Bytes,
            ) -> ::core::result::Result<Self, ::typeway_protobuf::TypewayDecodeError> {
                let bytes = &input[..];
                #(#field_defaults)*
                let mut offset: usize = 0;

                while offset < bytes.len() {
                    let (tag_wire, consumed) =
                        ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                    offset += consumed;
                    let field_number = (tag_wire >> 3) as u32;
                    let wire_type = (tag_wire & 0x07) as u8;

                    match field_number {
                        #(#decode_bytes_arms)*
                        _ => {
                            let skipped =
                                ::typeway_protobuf::tw_skip_wire_value(&bytes[offset..], wire_type)?;
                            offset += skipped;
                        }
                    }
                }

                Ok(#name { #(#field_names),* })
            }
        }
    })
}

/// Precompute the tag+wiretype byte(s) at macro expansion time.
fn precompute_tag_byte(field_number: u32, wire_type: u8) -> u8 {
    ((field_number << 3) | (wire_type as u32)) as u8
}

/// Emit code to write a precomputed tag byte (fields 1-15).
fn emit_tag_push(tag: u32, wt: u8) -> TokenStream2 {
    let byte = precompute_tag_byte(tag, wt);
    if tag < 16 {
        // Single byte — just push the constant.
        quote! { buf.push(#byte); }
    } else {
        // Multi-byte tag — use the varint encoder.
        quote! { ::typeway_protobuf::tw_encode_tag(buf, #tag, #wt); }
    }
}

fn gen_encode_field(f: &CodecField) -> TokenStream2 {
    let ident = &f.ident;
    let tag = f.tag;
    let wt = wire_type_for_kind(&f.codec_kind);
    let tag_push = emit_tag_push(tag, wt);

    match &f.codec_kind {
        CodecKind::Varint => quote! {
            if self.#ident != 0 {
                #tag_push
                ::typeway_protobuf::tw_encode_varint(buf, self.#ident as u64);
            }
        },
        CodecKind::Bool => {
            // Tag + value as two bytes pushed together.
            let tag_byte = precompute_tag_byte(tag, wt);
            quote! {
                if self.#ident {
                    buf.extend_from_slice(&[#tag_byte, 1]);
                }
            }
        },
        CodecKind::Fixed32 => quote! {
            if self.#ident != 0.0 {
                #tag_push
                buf.extend_from_slice(&self.#ident.to_le_bytes());
            }
        },
        CodecKind::Fixed64 => quote! {
            if self.#ident != 0.0 {
                #tag_push
                buf.extend_from_slice(&self.#ident.to_le_bytes());
            }
        },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            if !self.#ident.is_empty() {
                #tag_push
                ::typeway_protobuf::tw_encode_varint(buf, self.#ident.len() as u64);
                buf.extend_from_slice(self.#ident.as_bytes());
            }
        },
        CodecKind::LenBytes => quote! {
            if !self.#ident.is_empty() {
                #tag_push
                ::typeway_protobuf::tw_encode_varint(buf, self.#ident.len() as u64);
                buf.extend_from_slice(&self.#ident);
            }
        },
        CodecKind::Message => quote! {
            {
                let nested = ::typeway_protobuf::TypewayEncode::encode_to_vec(&self.#ident);
                if !nested.is_empty() {
                    #tag_push
                    ::typeway_protobuf::tw_encode_varint(buf, nested.len() as u64);
                    buf.extend_from_slice(&nested);
                }
            }
        },
        CodecKind::Optional(inner) => {
            let inner_encode = gen_encode_optional_inner(tag, wt, inner);
            quote! {
                if let Some(ref val) = self.#ident {
                    #inner_encode
                }
            }
        }
        CodecKind::OptionalMessage => quote! {
            if let Some(ref val) = self.#ident {
                let nested = ::typeway_protobuf::TypewayEncode::encode_to_vec(val);
                #tag_push
                ::typeway_protobuf::tw_encode_varint(buf, nested.len() as u64);
                buf.extend_from_slice(&nested);
            }
        },
        CodecKind::Repeated(inner) => {
            if is_packable(inner) {
                let item_write = gen_packed_item_write(inner);
                let is_varint = matches!(inner.as_ref(), CodecKind::Varint);
                let packed_tag_push = emit_tag_push(tag, 2);
                if is_varint {
                    quote! {
                        if !self.#ident.is_empty() {
                            #packed_tag_push
                            let len_pos = buf.len();
                            buf.push(0); // placeholder for length
                            let data_start = buf.len();
                            buf.reserve(self.#ident.len() * 10);
                            // Batch unsafe write: ONE set_len for all varints.
                            unsafe {
                                let base = buf.as_mut_ptr();
                                let mut pos = data_start;
                                for item in &self.#ident {
                                    let mut v = *item as u64;
                                    while v >= 0x80 {
                                        *base.add(pos) = (v as u8 & 0x7F) | 0x80;
                                        v >>= 7;
                                        pos += 1;
                                    }
                                    *base.add(pos) = v as u8;
                                    pos += 1;
                                }
                                buf.set_len(pos);
                            }
                            let packed_len = buf.len() - data_start;
                            if packed_len < 0x80 {
                                buf[len_pos] = packed_len as u8;
                            } else {
                                let data = buf[data_start..].to_vec();
                                buf.truncate(len_pos);
                                ::typeway_protobuf::tw_encode_varint(buf, packed_len as u64);
                                buf.extend_from_slice(&data);
                            }
                        }
                    }
                } else {
                    // Fixed-size types: length is known without iterating.
                    let packed_len_expr = match inner.as_ref() {
                        CodecKind::Fixed32 => quote! { self.#ident.len() * 4 },
                        CodecKind::Fixed64 => quote! { self.#ident.len() * 8 },
                        CodecKind::Bool => quote! { self.#ident.len() },
                        _ => unreachable!(),
                    };
                    quote! {
                        if !self.#ident.is_empty() {
                            let packed_len = #packed_len_expr;
                            #packed_tag_push
                            ::typeway_protobuf::tw_encode_varint(buf, packed_len as u64);
                            for item in &self.#ident {
                                #item_write
                            }
                        }
                    }
                }
            } else {
                // Non-packable (strings, messages): per-element tag.
                let item_encode = gen_encode_repeated_item(tag, wt, inner);
                quote! {
                    for item in &self.#ident {
                        #item_encode
                    }
                }
            }
        }
        CodecKind::RepeatedMessage => quote! {
            for item in &self.#ident {
                let nested = ::typeway_protobuf::TypewayEncode::encode_to_vec(item);
                #tag_push
                ::typeway_protobuf::tw_encode_varint(buf, nested.len() as u64);
                buf.extend_from_slice(&nested);
            }
        },
    }
}

/// Returns true if the inner type can use packed encoding (scalars only).
fn is_packable(kind: &CodecKind) -> bool {
    matches!(kind, CodecKind::Varint | CodecKind::Bool | CodecKind::Fixed32 | CodecKind::Fixed64)
}

/// Generate the per-item write for packed encoding (no tag per item).
/// For varints, uses the unchecked variant (caller pre-reserves capacity).
fn gen_packed_item_write(kind: &CodecKind) -> TokenStream2 {
    match kind {
        CodecKind::Varint => quote! {
            unsafe { ::typeway_protobuf::tw_encode_varint_unchecked(buf, *item as u64); }
        },
        CodecKind::Bool => quote! {
            buf.push(if *item { 1 } else { 0 });
        },
        CodecKind::Fixed32 => quote! {
            buf.extend_from_slice(&item.to_le_bytes());
        },
        CodecKind::Fixed64 => quote! {
            buf.extend_from_slice(&item.to_le_bytes());
        },
        _ => quote! {},
    }
}

/// Generate the per-item length for packed encoding.
fn gen_packed_item_len(kind: &CodecKind) -> TokenStream2 {
    match kind {
        CodecKind::Varint => quote! {
            ::typeway_protobuf::tw_varint_len(*item as u64)
        },
        CodecKind::Bool => quote! { 1 },
        CodecKind::Fixed32 => quote! { 4 },
        CodecKind::Fixed64 => quote! { 8 },
        _ => quote! { 0 },
    }
}

/// Generate per-item read for packed decoding.
fn gen_packed_item_read(ident: &Ident, kind: &CodecKind) -> TokenStream2 {
    match kind {
        CodecKind::Varint => quote! {
            let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
            offset += consumed;
            #ident.push(val as _);
        },
        CodecKind::Bool => quote! {
            #ident.push(bytes[offset] != 0);
            offset += 1;
        },
        CodecKind::Fixed32 => quote! {
            if offset + 4 > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            #ident.push(f32::from_le_bytes([
                bytes[offset], bytes[offset + 1],
                bytes[offset + 2], bytes[offset + 3],
            ]));
            offset += 4;
        },
        CodecKind::Fixed64 => quote! {
            if offset + 8 > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            #ident.push(f64::from_le_bytes([
                bytes[offset], bytes[offset + 1],
                bytes[offset + 2], bytes[offset + 3],
                bytes[offset + 4], bytes[offset + 5],
                bytes[offset + 6], bytes[offset + 7],
            ]));
            offset += 8;
        },
        _ => quote! {},
    }
}

fn gen_encode_optional_inner(tag: u32, wt: u8, kind: &CodecKind) -> TokenStream2 {
    let tp = emit_tag_push(tag, wt);
    match kind {
        CodecKind::Varint => quote! {
            #tp
            ::typeway_protobuf::tw_encode_varint(buf, *val as u64);
        },
        CodecKind::Fixed32 => quote! {
            #tp
            buf.extend_from_slice(&val.to_le_bytes());
        },
        CodecKind::Fixed64 => quote! {
            #tp
            buf.extend_from_slice(&val.to_le_bytes());
        },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            #tp
            ::typeway_protobuf::tw_encode_varint(buf, val.len() as u64);
            buf.extend_from_slice(val.as_bytes());
        },
        CodecKind::LenBytes => quote! {
            #tp
            ::typeway_protobuf::tw_encode_varint(buf, val.len() as u64);
            buf.extend_from_slice(val);
        },
        _ => quote! {},
    }
}

fn gen_encode_repeated_item(tag: u32, wt: u8, kind: &CodecKind) -> TokenStream2 {
    let tp = emit_tag_push(tag, wt);
    match kind {
        CodecKind::Varint => quote! {
            #tp
            ::typeway_protobuf::tw_encode_varint(buf, *item as u64);
        },
        CodecKind::Fixed32 => quote! {
            #tp
            buf.extend_from_slice(&item.to_le_bytes());
        },
        CodecKind::Fixed64 => quote! {
            #tp
            buf.extend_from_slice(&item.to_le_bytes());
        },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            #tp
            ::typeway_protobuf::tw_encode_varint(buf, item.len() as u64);
            buf.extend_from_slice(item.as_bytes());
        },
        _ => quote! {},
    }
}

fn gen_encoded_len_field(f: &CodecField) -> TokenStream2 {
    let ident = &f.ident;
    let tag = f.tag;
    // Precompute tag length at macro expansion time.
    let wt = wire_type_for_kind(&f.codec_kind);
    let tag_byte_count = if tag < 16 { 1usize } else if tag < 2048 { 2 } else { 3 };
    let tag_len_expr = quote! { #tag_byte_count };
    let _ = wt; // used in computation above conceptually

    match &f.codec_kind {
        CodecKind::Varint => quote! {
            if self.#ident != 0 {
                len += #tag_len_expr + ::typeway_protobuf::tw_varint_len(self.#ident as u64);
            }
        },
        CodecKind::Bool => quote! {
            if self.#ident {
                len += #tag_len_expr + 1;
            }
        },
        CodecKind::Fixed32 => quote! {
            if self.#ident != 0.0 {
                len += #tag_len_expr + 4;
            }
        },
        CodecKind::Fixed64 => quote! {
            if self.#ident != 0.0 {
                len += #tag_len_expr + 8;
            }
        },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            if !self.#ident.is_empty() {
                len += #tag_len_expr
                    + ::typeway_protobuf::tw_varint_len(self.#ident.len() as u64)
                    + self.#ident.len();
            }
        },
        CodecKind::LenBytes => quote! {
            if !self.#ident.is_empty() {
                len += #tag_len_expr
                    + ::typeway_protobuf::tw_varint_len(self.#ident.len() as u64)
                    + self.#ident.len();
            }
        },
        CodecKind::Message => quote! {
            {
                let nested_len = ::typeway_protobuf::TypewayEncode::encoded_len(&self.#ident);
                if nested_len > 0 {
                    len += #tag_len_expr
                        + ::typeway_protobuf::tw_varint_len(nested_len as u64)
                        + nested_len;
                }
            }
        },
        CodecKind::Optional(inner) => {
            let inner_len = gen_encoded_len_optional_inner(tag, inner);
            quote! {
                if let Some(ref val) = self.#ident {
                    #inner_len
                }
            }
        }
        CodecKind::OptionalMessage => quote! {
            if let Some(ref val) = self.#ident {
                let nested_len = ::typeway_protobuf::TypewayEncode::encoded_len(val);
                len += #tag_len_expr
                    + ::typeway_protobuf::tw_varint_len(nested_len as u64)
                    + nested_len;
            }
        },
        CodecKind::Repeated(inner) => {
            if is_packable(inner) {
                let item_len = gen_packed_item_len(inner);
                quote! {
                    if !self.#ident.is_empty() {
                        let mut packed_len: usize = 0;
                        for item in &self.#ident {
                            packed_len += #item_len;
                        }
                        // tag + length varint + packed data
                        len += #tag_len_expr
                            + ::typeway_protobuf::tw_varint_len(packed_len as u64)
                            + packed_len;
                    }
                }
            } else {
                let item_len = gen_encoded_len_repeated_item(tag, inner);
                quote! {
                    for item in &self.#ident {
                        #item_len
                    }
                }
            }
        }
        CodecKind::RepeatedMessage => quote! {
            for item in &self.#ident {
                let nested_len = ::typeway_protobuf::TypewayEncode::encoded_len(item);
                len += #tag_len_expr
                    + ::typeway_protobuf::tw_varint_len(nested_len as u64)
                    + nested_len;
            }
        },
    }
}

fn gen_encoded_len_optional_inner(tag: u32, kind: &CodecKind) -> TokenStream2 {
    let tl = if tag < 16 { 1usize } else if tag < 2048 { 2 } else { 3 };
    let tag_len_expr = quote! { #tl };
    match kind {
        CodecKind::Varint => quote! {
            len += #tag_len_expr + ::typeway_protobuf::tw_varint_len(*val as u64);
        },
        CodecKind::Fixed32 => quote! { len += #tag_len_expr + 4; },
        CodecKind::Fixed64 => quote! { len += #tag_len_expr + 8; },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            len += #tag_len_expr
                + ::typeway_protobuf::tw_varint_len(val.len() as u64)
                + val.len();
        },
        _ => quote! {},
    }
}

fn gen_encoded_len_repeated_item(tag: u32, kind: &CodecKind) -> TokenStream2 {
    let tl = if tag < 16 { 1usize } else if tag < 2048 { 2 } else { 3 };
    let tag_len_expr = quote! { #tl };
    match kind {
        CodecKind::Varint => quote! {
            len += #tag_len_expr + ::typeway_protobuf::tw_varint_len(*item as u64);
        },
        CodecKind::Fixed32 => quote! { len += #tag_len_expr + 4; },
        CodecKind::Fixed64 => quote! { len += #tag_len_expr + 8; },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            len += #tag_len_expr
                + ::typeway_protobuf::tw_varint_len(item.len() as u64)
                + item.len();
        },
        _ => quote! {},
    }
}

/// Generate a decode arm for `typeway_decode_bytes` — uses `Bytes::slice()`
/// for `BytesStr` fields (zero-copy), delegates to `gen_decode_arm` for others.
fn gen_decode_bytes_arm(f: &CodecField) -> TokenStream2 {
    let ident = &f.ident;
    let tag = f.tag;
    let ident_str = ident.to_string();

    // For BytesStr fields, use Bytes::slice() — zero-copy.
    if matches!(&f.codec_kind, CodecKind::LenBytesStr) {
        return quote! {
            #tag => {
                let (str_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let str_len = str_len as usize;
                if offset + str_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                // Zero-copy: validate UTF-8, then slice the Bytes (refcount increment, no copy).
                ::core::str::from_utf8(&bytes[offset..offset + str_len])
                    .map_err(|_| ::typeway_protobuf::TypewayDecodeError::InvalidUtf8(#ident_str))?;
                #ident = unsafe {
                    ::typeway_protobuf::BytesStr::from_utf8_unchecked(
                        input.slice(offset..offset + str_len)
                    )
                };
                offset += str_len;
            }
        };
    }

    // For all other field types, use the same logic as typeway_decode.
    gen_decode_arm(f)
}

fn gen_decode_arm(f: &CodecField) -> TokenStream2 {
    let ident = &f.ident;
    let tag = f.tag;
    let ident_str = ident.to_string();

    match &f.codec_kind {
        CodecKind::Varint => quote! {
            #tag => {
                let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                #ident = val as _;
            }
        },
        CodecKind::Bool => quote! {
            #tag => {
                let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                #ident = val != 0;
            }
        },
        CodecKind::Fixed32 => quote! {
            #tag => {
                if offset + 4 > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                #ident = f32::from_le_bytes([
                    bytes[offset], bytes[offset + 1],
                    bytes[offset + 2], bytes[offset + 3],
                ]);
                offset += 4;
            }
        },
        CodecKind::Fixed64 => quote! {
            #tag => {
                if offset + 8 > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                #ident = f64::from_le_bytes([
                    bytes[offset], bytes[offset + 1],
                    bytes[offset + 2], bytes[offset + 3],
                    bytes[offset + 4], bytes[offset + 5],
                    bytes[offset + 6], bytes[offset + 7],
                ]);
                offset += 8;
            }
        },
        CodecKind::LenString => quote! {
            #tag => {
                let (str_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let str_len = str_len as usize;
                if offset + str_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                let slice = &bytes[offset..offset + str_len];
                ::core::str::from_utf8(slice)
                    .map_err(|_| ::typeway_protobuf::TypewayDecodeError::InvalidUtf8(#ident_str))?;
                #ident = unsafe { String::from_utf8_unchecked(slice.to_vec()) };
                offset += str_len;
            }
        },
        CodecKind::LenBytesStr => quote! {
            #tag => {
                let (str_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let str_len = str_len as usize;
                if offset + str_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                let slice = &bytes[offset..offset + str_len];
                ::core::str::from_utf8(slice)
                    .map_err(|_| ::typeway_protobuf::TypewayDecodeError::InvalidUtf8(#ident_str))?;
                #ident = ::typeway_protobuf::BytesStr::from(
                    unsafe { String::from_utf8_unchecked(slice.to_vec()) }
                );
                offset += str_len;
            }
        },
        CodecKind::LenBytes => quote! {
            #tag => {
                let (byte_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let byte_len = byte_len as usize;
                if offset + byte_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                #ident = bytes[offset..offset + byte_len].to_vec();
                offset += byte_len;
            }
        },
        CodecKind::Message => quote! {
            #tag => {
                let (msg_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let msg_len = msg_len as usize;
                if offset + msg_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                #ident = ::typeway_protobuf::TypewayDecode::typeway_decode(
                    &bytes[offset..offset + msg_len]
                )?;
                offset += msg_len;
            }
        },
        CodecKind::Optional(inner) => {
            let inner_decode = gen_decode_optional_inner(ident, &ident_str, inner);
            quote! { #tag => { #inner_decode } }
        }
        CodecKind::OptionalMessage => quote! {
            #tag => {
                let (msg_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let msg_len = msg_len as usize;
                if offset + msg_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                #ident = Some(::typeway_protobuf::TypewayDecode::typeway_decode(
                    &bytes[offset..offset + msg_len]
                )?);
                offset += msg_len;
            }
        },
        CodecKind::Repeated(inner) => {
            if is_packable(inner) {
                let is_varint = matches!(inner.as_ref(), CodecKind::Varint);
                if is_varint {
                    // Optimized packed varint decode: inline 1-byte fast path,
                    // pre-reserve Vec capacity.
                    quote! {
                        #tag => {
                            if wire_type == 2 {
                                let (packed_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                                offset += consumed;
                                let packed_len = packed_len as usize;
                                let packed_end = offset + packed_len;
                                if packed_end > bytes.len() {
                                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                                }
                                // Reserve worst case: at least 1 element per byte.
                                #ident.reserve(packed_len);
                                while offset < packed_end {
                                    // Inline 1-byte fast path (most common for small u32).
                                    let b = bytes[offset];
                                    if b < 0x80 {
                                        #ident.push(b as _);
                                        offset += 1;
                                    } else {
                                        let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                                        offset += consumed;
                                        #ident.push(val as _);
                                    }
                                }
                            } else {
                                let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                                offset += consumed;
                                #ident.push(val as _);
                            }
                        }
                    }
                } else {
                    let item_read = gen_packed_item_read(ident, inner);
                    quote! {
                        #tag => {
                            if wire_type == 2 {
                                let (packed_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                                offset += consumed;
                                let packed_len = packed_len as usize;
                                let packed_end = offset + packed_len;
                                if packed_end > bytes.len() {
                                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                                }
                                while offset < packed_end {
                                    #item_read
                                }
                            } else {
                                let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                                offset += consumed;
                                #ident.push(val as _);
                            }
                        }
                    }
                }
            } else {
                let item_decode = gen_decode_repeated_item(ident, &ident_str, inner);
                quote! { #tag => { #item_decode } }
            }
        }
        CodecKind::RepeatedMessage => quote! {
            #tag => {
                let (msg_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
                offset += consumed;
                let msg_len = msg_len as usize;
                if offset + msg_len > bytes.len() {
                    return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
                }
                #ident.push(::typeway_protobuf::TypewayDecode::typeway_decode(
                    &bytes[offset..offset + msg_len]
                )?);
                offset += msg_len;
            }
        },
    }
}

fn gen_decode_optional_inner(ident: &Ident, ident_str: &str, kind: &CodecKind) -> TokenStream2 {
    match kind {
        CodecKind::Varint => quote! {
            let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
            offset += consumed;
            #ident = Some(val as _);
        },
        CodecKind::Fixed32 => quote! {
            if offset + 4 > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            #ident = Some(f32::from_le_bytes([
                bytes[offset], bytes[offset + 1],
                bytes[offset + 2], bytes[offset + 3],
            ]));
            offset += 4;
        },
        CodecKind::Fixed64 => quote! {
            if offset + 8 > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            #ident = Some(f64::from_le_bytes([
                bytes[offset], bytes[offset + 1],
                bytes[offset + 2], bytes[offset + 3],
                bytes[offset + 4], bytes[offset + 5],
                bytes[offset + 6], bytes[offset + 7],
            ]));
            offset += 8;
        },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            let (str_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
            offset += consumed;
            let str_len = str_len as usize;
            if offset + str_len > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            {
                let slice = &bytes[offset..offset + str_len];
                ::core::str::from_utf8(slice)
                    .map_err(|_| ::typeway_protobuf::TypewayDecodeError::InvalidUtf8(#ident_str))?;
                #ident = Some(unsafe { String::from_utf8_unchecked(slice.to_vec()) });
            }
            offset += str_len;
        },
        _ => quote! {
            let skipped = ::typeway_protobuf::tw_skip_wire_value(&bytes[offset..], wire_type)?;
            offset += skipped;
        },
    }
}

fn gen_decode_repeated_item(ident: &Ident, ident_str: &str, kind: &CodecKind) -> TokenStream2 {
    match kind {
        CodecKind::Varint => quote! {
            let (val, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
            offset += consumed;
            #ident.push(val as _);
        },
        CodecKind::Fixed32 => quote! {
            if offset + 4 > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            #ident.push(f32::from_le_bytes([
                bytes[offset], bytes[offset + 1],
                bytes[offset + 2], bytes[offset + 3],
            ]));
            offset += 4;
        },
        CodecKind::Fixed64 => quote! {
            if offset + 8 > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            #ident.push(f64::from_le_bytes([
                bytes[offset], bytes[offset + 1],
                bytes[offset + 2], bytes[offset + 3],
                bytes[offset + 4], bytes[offset + 5],
                bytes[offset + 6], bytes[offset + 7],
            ]));
            offset += 8;
        },
        CodecKind::LenString | CodecKind::LenBytesStr => quote! {
            let (str_len, consumed) = ::typeway_protobuf::tw_decode_varint(&bytes[offset..])?;
            offset += consumed;
            let str_len = str_len as usize;
            if offset + str_len > bytes.len() {
                return Err(::typeway_protobuf::TypewayDecodeError::UnexpectedEof);
            }
            {
                let slice = &bytes[offset..offset + str_len];
                ::core::str::from_utf8(slice)
                    .map_err(|_| ::typeway_protobuf::TypewayDecodeError::InvalidUtf8(#ident_str))?;
                #ident.push(unsafe { String::from_utf8_unchecked(slice.to_vec()) });
            }
            offset += str_len;
        },
        _ => quote! {
            let skipped = ::typeway_protobuf::tw_skip_wire_value(&bytes[offset..], wire_type)?;
            offset += skipped;
        },
    }
}
