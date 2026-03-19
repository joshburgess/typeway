use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "typeway-migrate")]
#[command(about = "Bidirectional Axum <-> Typeway code migration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert Axum code to Typeway.
    AxumToTypeway {
        /// Process a single file.
        #[arg(long)]
        file: Option<PathBuf>,

        /// Process all .rs files in a directory.
        #[arg(long, default_value = "src/")]
        dir: PathBuf,

        /// Print output to stdout instead of writing files.
        #[arg(long)]
        dry_run: bool,

        /// Also update Cargo.toml (add typeway, comment out axum).
        #[arg(long)]
        update_cargo: bool,
    },

    /// Convert Typeway code to Axum.
    TypewayToAxum {
        /// Process a single file.
        #[arg(long)]
        file: Option<PathBuf>,

        /// Process all .rs files in a directory.
        #[arg(long, default_value = "src/")]
        dir: PathBuf,

        /// Print output to stdout instead of writing files.
        #[arg(long)]
        dry_run: bool,

        /// Also update Cargo.toml (add axum, comment out typeway).
        #[arg(long)]
        update_cargo: bool,
    },

    /// Analyze Axum code and report what would be converted.
    Check {
        /// Process a single file.
        #[arg(long)]
        file: Option<PathBuf>,

        /// Process all .rs files in a directory.
        #[arg(long, default_value = "src/")]
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::AxumToTypeway {
            file,
            dir,
            dry_run,
            update_cargo,
        } => {
            let files = collect_files(file.as_deref(), &dir)?;
            for path in files {
                let source = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;

                match typeway_migrate::axum_to_typeway(&source) {
                    Ok(output) => {
                        if dry_run {
                            println!("// === {} ===\n{}", path.display(), output);
                        } else {
                            // Create backup.
                            let backup = path.with_extension("rs.bak");
                            std::fs::copy(&path, &backup).with_context(|| {
                                format!("failed to create backup {}", backup.display())
                            })?;

                            std::fs::write(&path, &output).with_context(|| {
                                format!("failed to write {}", path.display())
                            })?;

                            eprintln!(
                                "Converted {} (backup: {})",
                                path.display(),
                                backup.display()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Skipping {} — {}", path.display(), e);
                    }
                }
            }

            if update_cargo {
                let cargo_path = find_cargo_toml(file.as_deref(), &dir)?;
                let updated = typeway_migrate::cargo_editor::update_cargo_for_typeway(&cargo_path)
                    .with_context(|| {
                        format!("failed to update {}", cargo_path.display())
                    })?;
                if dry_run {
                    println!("// === {} ===\n{}", cargo_path.display(), updated);
                } else {
                    let backup = cargo_path.with_extension("toml.bak");
                    std::fs::copy(&cargo_path, &backup).with_context(|| {
                        format!("failed to create backup {}", backup.display())
                    })?;
                    std::fs::write(&cargo_path, &updated).with_context(|| {
                        format!("failed to write {}", cargo_path.display())
                    })?;
                    eprintln!(
                        "Updated {} (backup: {})",
                        cargo_path.display(),
                        backup.display()
                    );
                }
            }
        }

        Command::TypewayToAxum {
            file,
            dir,
            dry_run,
            update_cargo,
        } => {
            let files = collect_files(file.as_deref(), &dir)?;
            for path in files {
                let source = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;

                match typeway_migrate::typeway_to_axum(&source) {
                    Ok(output) => {
                        if dry_run {
                            println!("// === {} ===\n{}", path.display(), output);
                        } else {
                            // Create backup.
                            let backup = path.with_extension("rs.bak");
                            std::fs::copy(&path, &backup).with_context(|| {
                                format!("failed to create backup {}", backup.display())
                            })?;

                            std::fs::write(&path, &output).with_context(|| {
                                format!("failed to write {}", path.display())
                            })?;

                            eprintln!(
                                "Converted {} (backup: {})",
                                path.display(),
                                backup.display()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Skipping {} — {}", path.display(), e);
                    }
                }
            }

            if update_cargo {
                let cargo_path = find_cargo_toml(file.as_deref(), &dir)?;
                let updated = typeway_migrate::cargo_editor::update_cargo_for_axum(&cargo_path)
                    .with_context(|| {
                        format!("failed to update {}", cargo_path.display())
                    })?;
                if dry_run {
                    println!("// === {} ===\n{}", cargo_path.display(), updated);
                } else {
                    let backup = cargo_path.with_extension("toml.bak");
                    std::fs::copy(&cargo_path, &backup).with_context(|| {
                        format!("failed to create backup {}", backup.display())
                    })?;
                    std::fs::write(&cargo_path, &updated).with_context(|| {
                        format!("failed to write {}", cargo_path.display())
                    })?;
                    eprintln!(
                        "Updated {} (backup: {})",
                        cargo_path.display(),
                        backup.display()
                    );
                }
            }
        }

        Command::Check { file, dir } => {
            let files = collect_files(file.as_deref(), &dir)?;
            for path in files {
                let source = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;

                // Heuristic: if the source contains typeway-specific constructs,
                // use the Typeway parser; otherwise use the Axum parser.
                let is_typeway = source.contains("typeway_path!")
                    || source.contains("type API")
                        && (source.contains("GetEndpoint")
                            || source.contains("PostEndpoint")
                            || source.contains("DeleteEndpoint")
                            || source.contains("PutEndpoint")
                            || source.contains("PatchEndpoint"));

                let parse_result = if is_typeway {
                    typeway_migrate::parse::typeway::parse_typeway_file(&source)
                } else {
                    typeway_migrate::parse::axum::parse_axum_file(&source)
                };

                match parse_result {
                    Ok(model) => {
                        if model.endpoints.is_empty() {
                            continue;
                        }

                        let framework = if is_typeway { "Typeway" } else { "Axum" };
                        println!("{} ({} source):", path.display(), framework);
                        println!(
                            "  {} endpoints found:",
                            model.endpoints.len()
                        );
                        for ep in &model.endpoints {
                            println!(
                                "    {:?} {} \u{2192} {}",
                                ep.method,
                                ep.path.raw_pattern,
                                ep.handler.name
                            );
                        }
                        if !model.layers.is_empty() {
                            println!("  {} layers", model.layers.len());
                        }
                        if model.state_type.is_some() {
                            println!("  State type detected");
                        }
                        if let Some(ref prefix) = model.prefix {
                            println!("  Nest prefix: {}", prefix);
                        }

                        // Auth detection report.
                        let auth_endpoints: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| ep.requires_auth)
                            .collect();
                        if !auth_endpoints.is_empty() {
                            println!("  Auth detection:");
                            for ep in &auth_endpoints {
                                let auth_ty = ep
                                    .auth_type
                                    .as_deref()
                                    .unwrap_or("unknown");
                                println!(
                                    "    {}: Protected ({})",
                                    ep.handler.name, auth_ty
                                );
                            }
                        }

                        // Effects detection report.
                        if !model.detected_effects.is_empty() {
                            println!("  Effects detected:");
                            for effect in &model.detected_effects {
                                let source_hint = match effect.effect_name.as_str() {
                                    "CorsRequired" => " (from CorsLayer)",
                                    "TracingRequired" => " (from TraceLayer)",
                                    "RateLimitRequired" => " (from RateLimitLayer)",
                                    _ => "",
                                };
                                println!(
                                    "    {}{}",
                                    effect.effect_name, source_hint
                                );
                            }
                        }

                        // Validation candidates report.
                        let validation_endpoints: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| ep.has_validation)
                            .collect();
                        if !validation_endpoints.is_empty() {
                            println!("  Validation candidates:");
                            for ep in &validation_endpoints {
                                println!(
                                    "    {}: body validation patterns detected",
                                    ep.handler.name
                                );
                            }
                        }

                        // Query extractor report.
                        let query_endpoints: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| {
                                ep.handler
                                    .extractors
                                    .iter()
                                    .any(|e| {
                                        e.kind
                                            == typeway_migrate::model::ExtractorKind::Query
                                    })
                            })
                            .collect();
                        if !query_endpoints.is_empty() {
                            println!("  Query extractors:");
                            for ep in &query_endpoints {
                                let query_ext = ep
                                    .handler
                                    .extractors
                                    .iter()
                                    .find(|e| {
                                        e.kind
                                            == typeway_migrate::model::ExtractorKind::Query
                                    });
                                let type_str = if let Some(ext) = query_ext {
                                    if let Some(ref inner) = ext.inner_type {
                                        let ts = quote::quote! { #inner };
                                        format!("Query<{}>", ts)
                                    } else {
                                        "Query<...>".to_string()
                                    }
                                } else {
                                    "Query<...>".to_string()
                                };
                                println!(
                                    "    {}: {}",
                                    ep.handler.name, type_str
                                );
                            }
                        }

                        // Cookie extractor report.
                        let cookie_endpoints: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| {
                                ep.handler
                                    .extractors
                                    .iter()
                                    .any(|e| {
                                        e.kind == typeway_migrate::model::ExtractorKind::Cookie
                                            || e.kind == typeway_migrate::model::ExtractorKind::CookieJar
                                    })
                            })
                            .collect();
                        if !cookie_endpoints.is_empty() {
                            println!("  Cookie extractors:");
                            for ep in &cookie_endpoints {
                                let ext = ep
                                    .handler
                                    .extractors
                                    .iter()
                                    .find(|e| {
                                        e.kind == typeway_migrate::model::ExtractorKind::Cookie
                                            || e.kind == typeway_migrate::model::ExtractorKind::CookieJar
                                    });
                                let type_str = if let Some(ext) = ext {
                                    let ty = &ext.full_type;
                                    format!("{}", quote::quote! { #ty })
                                } else {
                                    "Cookie".to_string()
                                };
                                println!(
                                    "    {}: {}",
                                    ep.handler.name, type_str
                                );
                            }
                        }

                        // Form/Multipart extractor report.
                        let form_endpoints: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| {
                                ep.handler
                                    .extractors
                                    .iter()
                                    .any(|e| {
                                        e.kind == typeway_migrate::model::ExtractorKind::Form
                                            || e.kind == typeway_migrate::model::ExtractorKind::Multipart
                                    })
                            })
                            .collect();
                        if !form_endpoints.is_empty() {
                            println!("  Form/Multipart extractors:");
                            for ep in &form_endpoints {
                                let ext = ep
                                    .handler
                                    .extractors
                                    .iter()
                                    .find(|e| {
                                        e.kind == typeway_migrate::model::ExtractorKind::Form
                                            || e.kind == typeway_migrate::model::ExtractorKind::Multipart
                                    });
                                let type_str = if let Some(ext) = ext {
                                    let ty = &ext.full_type;
                                    format!("{}", quote::quote! { #ty })
                                } else {
                                    "Form/Multipart".to_string()
                                };
                                println!(
                                    "    {}: {}",
                                    ep.handler.name, type_str
                                );
                            }
                        }

                        // WebSocket extractor report.
                        let ws_endpoints: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| {
                                ep.handler
                                    .extractors
                                    .iter()
                                    .any(|e| {
                                        e.kind == typeway_migrate::model::ExtractorKind::WebSocketUpgrade
                                    })
                            })
                            .collect();
                        if !ws_endpoints.is_empty() {
                            println!("  WebSocket endpoints:");
                            for ep in &ws_endpoints {
                                println!(
                                    "    {}: WebSocketUpgrade",
                                    ep.handler.name
                                );
                            }
                        }

                        // Bind macro report (Typeway sources).
                        if is_typeway {
                            let bind_count = model
                                .endpoints
                                .iter()
                                .filter(|ep| ep.bind_macro == typeway_migrate::model::BindMacro::Bind)
                                .count();
                            let bind_auth_count = model
                                .endpoints
                                .iter()
                                .filter(|ep| ep.bind_macro == typeway_migrate::model::BindMacro::BindAuth)
                                .count();
                            let bind_validated_count = model
                                .endpoints
                                .iter()
                                .filter(|ep| ep.bind_macro == typeway_migrate::model::BindMacro::BindValidated)
                                .count();

                            if bind_auth_count > 0 || bind_validated_count > 0 {
                                println!("  Bind macros:");
                                if bind_count > 0 {
                                    println!("    bind!: {}", bind_count);
                                }
                                if bind_auth_count > 0 {
                                    println!("    bind_auth!: {}", bind_auth_count);
                                }
                                if bind_validated_count > 0 {
                                    println!("    bind_validated!: {}", bind_validated_count);
                                }
                            }
                        }

                        // Check for impl IntoResponse handlers.
                        let impl_into_response_handlers: Vec<_> = model
                            .endpoints
                            .iter()
                            .filter(|ep| {
                                if let syn::Type::ImplTrait(impl_trait) = &ep.response_type {
                                    impl_trait.bounds.iter().any(|b| {
                                        if let syn::TypeParamBound::Trait(t) = b {
                                            t.path
                                                .segments
                                                .last()
                                                .is_some_and(|s| s.ident == "IntoResponse")
                                        } else {
                                            false
                                        }
                                    })
                                } else {
                                    false
                                }
                            })
                            .map(|ep| ep.handler.name.to_string())
                            .collect();

                        if !impl_into_response_handlers.is_empty() {
                            println!(
                                "  Handlers returning impl IntoResponse: {}",
                                impl_into_response_handlers.join(", ")
                            );
                        }

                        if !model.warnings.is_empty() {
                            println!("  Warnings:");
                            for warning in &model.warnings {
                                println!("    - {}", warning);
                            }
                        }

                        // Summary line with counts.
                        let total = model.endpoints.len();
                        let protected_count = auth_endpoints.len();
                        let public_count = total - protected_count;
                        let effects_count = model.detected_effects.len();
                        let validation_count = validation_endpoints.len();

                        let mut summary_parts = Vec::new();
                        summary_parts.push(format!(
                            "{} endpoint{}",
                            total,
                            if total == 1 { "" } else { "s" }
                        ));
                        if protected_count > 0 || public_count > 0 {
                            summary_parts.push(format!(
                                "{} protected, {} public",
                                protected_count, public_count
                            ));
                        }
                        if effects_count > 0 {
                            summary_parts.push(format!(
                                "{} effect{}",
                                effects_count,
                                if effects_count == 1 { "" } else { "s" }
                            ));
                        }
                        if validation_count > 0 {
                            summary_parts.push(format!(
                                "{} validation candidate{}",
                                validation_count,
                                if validation_count == 1 { "" } else { "s" }
                            ));
                        }

                        println!("  Summary: {}", summary_parts.join(", "));

                        let warning_count = model.warnings.len();
                        if warning_count == 0 {
                            println!("  Ready to convert.");
                        } else {
                            println!(
                                "  {} warning{} \u{2014} review before converting.",
                                warning_count,
                                if warning_count == 1 { "" } else { "s" }
                            );
                        }

                        println!();
                    }
                    Err(e) => {
                        eprintln!("Skipping {} \u{2014} {}", path.display(), e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Find the Cargo.toml relative to the `--file` or `--dir` argument.
///
/// Walks upward from the given path looking for a `Cargo.toml`.
fn find_cargo_toml(file: Option<&std::path::Path>, dir: &std::path::Path) -> Result<PathBuf> {
    let start = if let Some(f) = file {
        f.parent().unwrap_or(std::path::Path::new("."))
    } else {
        dir
    };

    let start = if start.is_relative() {
        std::env::current_dir()?.join(start)
    } else {
        start.to_path_buf()
    };

    let mut candidate = start.as_path();
    loop {
        let cargo = candidate.join("Cargo.toml");
        if cargo.exists() {
            return Ok(cargo);
        }
        match candidate.parent() {
            Some(parent) => candidate = parent,
            None => {
                anyhow::bail!(
                    "could not find Cargo.toml starting from {}",
                    start.display()
                );
            }
        }
    }
}

fn collect_files(file: Option<&std::path::Path>, dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    if let Some(f) = file {
        return Ok(vec![f.to_path_buf()]);
    }

    let mut files = Vec::new();
    if dir.exists() {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().extension().is_some_and(|ext| ext == "rs") {
                files.push(entry.into_path());
            }
        }
    }
    Ok(files)
}
