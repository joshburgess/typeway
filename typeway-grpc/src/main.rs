use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "typeway-grpc")]
#[command(about = "Bidirectional .proto <-> Typeway API type conversion")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a .proto file from a Typeway API type in a Rust source file.
    ///
    /// For full API type parsing, use `ApiToProto::to_proto()` in your code.
    /// This subcommand provides guidance on doing so.
    ProtoFromApi {
        /// The Rust source file containing the API type.
        #[arg(long)]
        file: PathBuf,

        /// Output .proto file path.
        #[arg(long, default_value = "service.proto")]
        output: PathBuf,

        /// gRPC service name.
        #[arg(long, default_value = "Service")]
        service_name: String,

        /// Proto package name.
        #[arg(long, default_value = "api.v1")]
        package: String,
    },

    /// Generate Typeway Rust code from a .proto file.
    ApiFromProto {
        /// The .proto file to read.
        #[arg(long)]
        file: PathBuf,

        /// Output Rust source file path.
        #[arg(long, default_value = "src/api.rs")]
        output: PathBuf,

        /// Print to stdout instead of writing a file.
        #[arg(long)]
        dry_run: bool,
    },

    /// Compare two .proto files and report breaking changes.
    ///
    /// Exits with code 1 if any breaking changes are detected, making
    /// it suitable for use in CI pipelines.
    Diff {
        /// The old (baseline) .proto file.
        #[arg(long)]
        old: PathBuf,

        /// The new .proto file.
        #[arg(long)]
        new: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::ProtoFromApi {
            file: _,
            output,
            service_name,
            package,
        } => {
            // Full API type parsing would require syn to walk the Rust AST,
            // which is out of scope for this CLI. Direct users to the
            // programmatic approach instead.
            eprintln!(
                "Note: For full API type parsing, use ApiToProto::to_proto() in your code."
            );
            eprintln!(
                "This CLI handles .proto -> Typeway conversion. For the reverse direction,"
            );
            eprintln!("add this to your code:");
            eprintln!();
            eprintln!(
                "  let proto = <MyAPI as typeway_grpc::ApiToProto>::to_proto(\"{}\", \"{}\");",
                service_name, package
            );
            eprintln!(
                "  std::fs::write(\"{}\", proto).unwrap();",
                output.display()
            );
            Ok(())
        }

        Command::ApiFromProto {
            file,
            output,
            dry_run,
        } => {
            let source = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {}", file.display(), e))?;

            let proto = typeway_grpc::proto_parse::parse_proto(&source)
                .map_err(|e| anyhow::anyhow!("failed to parse proto: {}", e))?;

            let rust_code = typeway_grpc::codegen::generate_typeway_from_proto(&proto);

            if dry_run {
                println!("{}", rust_code);
            } else {
                if let Some(parent) = output.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&output, &rust_code)?;
                eprintln!("Generated {} from {}", output.display(), file.display());
            }

            Ok(())
        }

        Command::Diff { old, new } => {
            let old_src = std::fs::read_to_string(&old)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {}", old.display(), e))?;
            let new_src = std::fs::read_to_string(&new)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {}", new.display(), e))?;

            let changes = typeway_grpc::diff::diff_protos(&old_src, &new_src)
                .map_err(|e| anyhow::anyhow!("failed to diff protos: {}", e))?;

            if changes.is_empty() {
                println!("No changes detected.");
                return Ok(());
            }

            let breaking: Vec<_> = changes
                .iter()
                .filter(|c| c.kind == typeway_grpc::ChangeKind::Breaking)
                .collect();
            let compatible: Vec<_> = changes
                .iter()
                .filter(|c| c.kind == typeway_grpc::ChangeKind::Compatible)
                .collect();

            if !breaking.is_empty() {
                println!("BREAKING CHANGES ({}):", breaking.len());
                for c in &breaking {
                    println!("  x {}: {}", c.location, c.description);
                }
            }
            if !compatible.is_empty() {
                println!("Compatible changes ({}):", compatible.len());
                for c in &compatible {
                    println!("  + {}: {}", c.location, c.description);
                }
            }

            if !breaking.is_empty() {
                std::process::exit(1);
            }

            Ok(())
        }
    }
}
