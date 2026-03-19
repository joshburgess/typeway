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

                match typeway_migrate::parse::axum::parse_axum_file(&source) {
                    Ok(model) => {
                        if model.endpoints.is_empty() {
                            continue;
                        }
                        println!("{}:", path.display());
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

                        // Summary line.
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
