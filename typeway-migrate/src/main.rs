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
                                "    {:?} {} → {}",
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
                        println!();
                    }
                    Err(e) => {
                        eprintln!("Skipping {} — {}", path.display(), e);
                    }
                }
            }
        }
    }

    Ok(())
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
