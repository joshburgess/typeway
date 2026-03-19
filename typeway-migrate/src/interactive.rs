//! Interactive prompts for resolving ambiguous migration decisions.
//!
//! When `--interactive` is passed, the migration tool pauses at each
//! ambiguity and asks the user to make a decision. Without the flag,
//! behavior is unchanged (warnings are emitted as TODO comments).

use anyhow::Result;
use dialoguer::{Confirm, Select};

use crate::model::*;

/// Run interactive prompts for ambiguous decisions in the model.
/// Modifies the model in-place based on user responses.
///
/// If stdin is not a TTY (e.g., piped input), dialoguer will use defaults.
pub fn resolve_ambiguities(model: &mut ApiModel) -> Result<()> {
    resolve_auth_detection(model)?;
    resolve_unknown_extractors(model)?;
    resolve_validation(model)?;
    resolve_effects(model)?;
    resolve_websocket(model)?;
    Ok(())
}

/// Prompt for each endpoint detected as requiring auth.
fn resolve_auth_detection(model: &mut ApiModel) -> Result<()> {
    for endpoint in &mut model.endpoints {
        if !endpoint.requires_auth {
            continue;
        }

        let auth_ty = endpoint
            .auth_type
            .as_deref()
            .unwrap_or("unknown");

        let prompt = format!(
            "Handler `{}` has `{}` as first argument.\n  \
             → Wrap in Protected<{}, E> and use bind_auth!()?",
            endpoint.handler.name, auth_ty, auth_ty
        );

        let confirmed = Confirm::new()
            .with_prompt(&prompt)
            .default(true)
            .interact_opt()?
            .unwrap_or(true);

        if !confirmed {
            endpoint.requires_auth = false;
            endpoint.auth_type = None;
            endpoint.bind_macro = BindMacro::Bind;
        }
    }
    Ok(())
}

/// Prompt for each Unknown extractor to classify it.
fn resolve_unknown_extractors(model: &mut ApiModel) -> Result<()> {
    for endpoint in &mut model.endpoints {
        let mut i = 0;
        while i < endpoint.handler.extractors.len() {
            if endpoint.handler.extractors[i].kind != ExtractorKind::Unknown {
                i += 1;
                continue;
            }

            let ext = &endpoint.handler.extractors[i];
            let type_str = {
                let ty = &ext.full_type;
                let ts = quote::quote! { #ty };
                ts.to_string()
            };

            let prompt = format!(
                "Handler `{}` uses unknown extractor type `{}`.\n  What is it?",
                endpoint.handler.name, type_str
            );

            let items = &[
                "Authentication extractor (wrap in Protected)",
                "Regular extractor (pass through as-is)",
                "Skip (add TODO comment)",
            ];

            let selection = Select::new()
                .with_prompt(&prompt)
                .items(items)
                .default(2) // default to "Skip"
                .interact_opt()?
                .unwrap_or(2);

            match selection {
                0 => {
                    // Auth extractor: mark endpoint as requiring auth.
                    endpoint.requires_auth = true;
                    endpoint.auth_type = Some(type_str.clone());
                    endpoint.bind_macro = BindMacro::BindAuth;
                }
                1 => {
                    // Regular extractor: leave as-is, no warning needed.
                    // Remove the unknown-extractor warning if one was added.
                    model.warnings.retain(|w| !w.contains(&type_str));
                }
                _ => {
                    // Skip: leave as Unknown, warning stays.
                }
            }

            i += 1;
        }
    }
    Ok(())
}

/// Prompt for each endpoint with detected validation patterns.
fn resolve_validation(model: &mut ApiModel) -> Result<()> {
    for endpoint in &mut model.endpoints {
        if !endpoint.has_validation {
            continue;
        }

        let validator_name = endpoint
            .validator_name
            .as_deref()
            .unwrap_or("Validator");

        let prompt = format!(
            "Handler `{}` appears to contain manual validation (.is_empty(), .len()).\n  \
             → Generate Validated<{}, E> wrapper?",
            endpoint.handler.name, validator_name
        );

        let confirmed = Confirm::new()
            .with_prompt(&prompt)
            .default(true)
            .interact_opt()?
            .unwrap_or(true);

        if !confirmed {
            endpoint.has_validation = false;
            endpoint.validator_name = None;
            if endpoint.bind_macro == BindMacro::BindValidated {
                endpoint.bind_macro = BindMacro::Bind;
            }
        }
    }
    Ok(())
}

/// Prompt for each detected middleware effect.
fn resolve_effects(model: &mut ApiModel) -> Result<()> {
    let mut retained_effects = Vec::new();

    for effect in model.detected_effects.drain(..) {
        let source_hint = match effect.effect_name.as_str() {
            "CorsRequired" => " (from CorsLayer)",
            "TracingRequired" => " (from TraceLayer)",
            "RateLimitRequired" => " (from RateLimitLayer)",
            _ => "",
        };

        let prompt = format!(
            "Detected {}{} middleware.\n  \
             → Wrap public endpoints in Requires<{}, E> and use EffectfulServer?",
            effect.effect_name, source_hint, effect.effect_name
        );

        let confirmed = Confirm::new()
            .with_prompt(&prompt)
            .default(true)
            .interact_opt()?
            .unwrap_or(true);

        if confirmed {
            retained_effects.push(effect);
        }
    }

    model.detected_effects = retained_effects;
    Ok(())
}

/// Prompt for WebSocket handlers about session-typed protocols.
fn resolve_websocket(model: &mut ApiModel) -> Result<()> {
    for endpoint in &mut model.endpoints {
        let has_ws = endpoint
            .handler
            .extractors
            .iter()
            .any(|e| e.kind == ExtractorKind::WebSocketUpgrade);

        if !has_ws {
            continue;
        }

        let prompt = format!(
            "Handler `{}` uses WebSocket upgrade.\n  \
             → Add session-typed protocol? (You'll need to define the protocol type manually)",
            endpoint.handler.name
        );

        let confirmed = Confirm::new()
            .with_prompt(&prompt)
            .default(false)
            .interact_opt()?
            .unwrap_or(false);

        if confirmed {
            // Add a warning reminding the user to define the protocol type.
            model.warnings.push(format!(
                "TODO: Define a session-typed protocol for WebSocket handler `{}`",
                endpoint.handler.name
            ));
        }
    }
    Ok(())
}

/// Filter endpoints to only those matching the given path patterns.
///
/// Patterns are matched against `endpoint.path.raw_pattern`. A pattern
/// matches if `raw_pattern` starts with the pattern (prefix match) or
/// equals it exactly.
pub fn filter_partial(model: &mut ApiModel, patterns: &[String]) {
    model.endpoints.retain(|ep| {
        patterns
            .iter()
            .any(|pat| ep.path.raw_pattern == *pat || ep.path.raw_pattern.starts_with(&format!("{}/", pat)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_model() -> ApiModel {
        ApiModel {
            endpoints: vec![],
            state_type: None,
            layers: vec![],
            passthrough_items: vec![],
            use_items: vec![],
            prefix: None,
            warnings: vec![],
            detected_effects: vec![],
        }
    }

    #[test]
    fn resolve_ambiguities_on_empty_model() {
        let mut model = empty_model();
        // Should not panic or error on an empty model.
        resolve_ambiguities(&mut model).expect("should succeed on empty model");
    }

    #[test]
    fn filter_partial_retains_matching_endpoints() {
        let mut model = empty_model();

        // We need real syn types for the model. Parse minimal handler stubs.
        let source = r#"
            async fn list_users() -> String { String::new() }
        "#;
        let item: syn::Item = syn::parse_str(&format!("{}", source.trim())).unwrap();
        let (name, return_type) = if let syn::Item::Fn(f) = &item {
            (
                f.sig.ident.clone(),
                match &f.sig.output {
                    syn::ReturnType::Type(_, ty) => (**ty).clone(),
                    syn::ReturnType::Default => syn::parse_str("()").unwrap(),
                },
            )
        } else {
            panic!("expected fn");
        };

        let handler = HandlerModel {
            name: name.clone(),
            is_async: true,
            extractors: vec![],
            return_type: return_type.clone(),
            body: vec![],
            attrs: vec![],
        };

        model.endpoints.push(EndpointModel {
            method: HttpMethod::Get,
            path: PathModel::from_axum_path("/users"),
            handler,
            request_body: None,
            response_type: return_type.clone(),
            requires_auth: false,
            auth_type: None,
            has_validation: false,
            validator_name: None,
            bind_macro: BindMacro::Bind,
        });

        // Add a second endpoint for /users/{id}.
        let handler2 = HandlerModel {
            name: syn::Ident::new("get_user", proc_macro2::Span::call_site()),
            is_async: true,
            extractors: vec![],
            return_type: return_type.clone(),
            body: vec![],
            attrs: vec![],
        };

        model.endpoints.push(EndpointModel {
            method: HttpMethod::Get,
            path: PathModel::from_axum_path("/users/{id}"),
            handler: handler2,
            request_body: None,
            response_type: return_type.clone(),
            requires_auth: false,
            auth_type: None,
            has_validation: false,
            validator_name: None,
            bind_macro: BindMacro::Bind,
        });

        // Add a third endpoint for /posts.
        let handler3 = HandlerModel {
            name: syn::Ident::new("list_posts", proc_macro2::Span::call_site()),
            is_async: true,
            extractors: vec![],
            return_type: return_type.clone(),
            body: vec![],
            attrs: vec![],
        };

        model.endpoints.push(EndpointModel {
            method: HttpMethod::Get,
            path: PathModel::from_axum_path("/posts"),
            handler: handler3,
            request_body: None,
            response_type: return_type,
            requires_auth: false,
            auth_type: None,
            has_validation: false,
            validator_name: None,
            bind_macro: BindMacro::Bind,
        });

        assert_eq!(model.endpoints.len(), 3);

        filter_partial(&mut model, &["/users".to_string()]);

        assert_eq!(model.endpoints.len(), 2);
        assert!(model.endpoints.iter().all(|ep| ep.path.raw_pattern.starts_with("/users")));
    }
}
