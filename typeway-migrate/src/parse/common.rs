//! Shared handler signature analysis utilities.

use syn::{FnArg, Pat, PatIdent, PatTupleStruct, ReturnType, Type, TypePath};

use crate::model::{ExtractorKind, ExtractorModel};

/// Analyze a function argument to determine its extractor kind and inner type.
pub fn analyze_extractor(arg: &FnArg) -> Option<ExtractorModel> {
    let pat_type = match arg {
        FnArg::Typed(pt) => pt,
        FnArg::Receiver(_) => return None,
    };

    let full_type = (*pat_type.ty).clone();
    let kind = classify_type(&full_type);
    let inner_type = extract_inner_type(&full_type);
    let var_name = extract_var_name(&pat_type.pat);
    let pattern = (*pat_type.pat).clone();

    Some(ExtractorModel {
        kind,
        pattern,
        full_type,
        inner_type,
        var_name,
    })
}

/// Classify a type as an extractor kind.
fn classify_type(ty: &Type) -> ExtractorKind {
    match ty {
        Type::Path(TypePath { path, .. }) => ExtractorKind::from_type_path(path),
        _ => ExtractorKind::Unknown,
    }
}

/// Extract the inner type from a wrapper type like `Path<u32>` → `u32`.
pub fn extract_inner_type(ty: &Type) -> Option<Type> {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            let last = path.segments.last()?;
            match &last.arguments {
                syn::PathArguments::AngleBracketed(args) => {
                    let first_arg = args.args.first()?;
                    match first_arg {
                        syn::GenericArgument::Type(inner) => Some(inner.clone()),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Extract a variable name from a pattern.
///
/// Handles:
/// - `path: Path<T>` → `path`
/// - `Path(id): Path<T>` → `id`
/// - `State(state): State<T>` → `state`
fn extract_var_name(pat: &Pat) -> Option<syn::Ident> {
    match pat {
        Pat::Ident(PatIdent { ident, .. }) => Some(ident.clone()),
        Pat::TupleStruct(PatTupleStruct { elems, .. }) => {
            // Path(id) → id
            if let Some(Pat::Ident(PatIdent { ident, .. })) = elems.first() {
                Some(ident.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract the return type from a function signature.
pub fn extract_return_type(output: &ReturnType) -> Type {
    match output {
        ReturnType::Default => syn::parse_quote! { () },
        ReturnType::Type(_, ty) => (**ty).clone(),
    }
}

/// Extract capture types from a `Path<T>` extractor.
///
/// - `Path<u32>` → `[u32]`
/// - `Path<(u32, String)>` → `[u32, String]`
pub fn extract_path_capture_types(inner: &Type) -> Vec<Type> {
    match inner {
        Type::Tuple(tuple) => tuple.elems.iter().cloned().collect(),
        other => vec![other.clone()],
    }
}

/// Extract capture variable names from a `Path(x)` or `Path((a, b))` pattern.
pub fn extract_path_var_names(pat: &Pat) -> Vec<syn::Ident> {
    match pat {
        Pat::TupleStruct(PatTupleStruct { elems, .. }) => {
            let mut names = Vec::new();
            for elem in elems {
                match elem {
                    Pat::Ident(PatIdent { ident, .. }) => {
                        names.push(ident.clone());
                    }
                    Pat::Tuple(tuple) => {
                        for inner in &tuple.elems {
                            if let Pat::Ident(PatIdent { ident, .. }) = inner {
                                names.push(ident.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            names
        }
        Pat::Ident(PatIdent { ident, .. }) => vec![ident.clone()],
        _ => Vec::new(),
    }
}
