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

        /// Generate TypewayCodec + ToProtoType + BytesStr for high-performance
        /// binary gRPC. Without this flag, generates serde-only types.
        #[arg(long)]
        codec: bool,

        /// Additional directories to search for imported .proto files.
        #[arg(long, short = 'I')]
        include: Vec<PathBuf>,
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

    /// Generate a service specification from a .proto file.
    ///
    /// Reads a .proto file and produces either a structured JSON spec
    /// (the gRPC equivalent of an OpenAPI spec) or an HTML documentation page.
    SpecFromProto {
        /// The .proto file to read.
        #[arg(long)]
        file: PathBuf,

        /// Output format: "json" or "html".
        #[arg(long, default_value = "json")]
        format: String,

        /// Output file path. If omitted, prints to stdout.
        #[arg(long)]
        output: Option<PathBuf>,
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
            codec,
            include,
        } => {
            let source = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {}", file.display(), e))?;

            // Parse with import resolution if include dirs are provided.
            let proto = if include.is_empty() {
                typeway_grpc::proto_parse::parse_proto(&source)
            } else {
                let dirs: Vec<&str> = include.iter().map(|p| p.to_str().unwrap_or(".")).collect();
                typeway_grpc::proto_parse::parse_proto_with_imports(&source, &dirs)
            }
            .map_err(|e| anyhow::anyhow!("failed to parse proto: {}", e))?;

            let rust_code = if codec {
                typeway_grpc::codegen::generate_typeway_from_proto_with_codec(&proto)
            } else {
                typeway_grpc::codegen::generate_typeway_from_proto(&proto)
            };

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

        Command::SpecFromProto {
            file,
            format,
            output,
        } => {
            let source = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {}", file.display(), e))?;

            let proto_file = typeway_grpc::proto_parse::parse_proto(&source)
                .map_err(|e| anyhow::anyhow!("failed to parse proto: {}", e))?;

            let spec = spec_from_parsed_proto(&proto_file, &source);

            let result = match format.as_str() {
                "json" => serde_json::to_string_pretty(&spec)
                    .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))?,
                "html" => typeway_grpc::docs_page::generate_docs_html(&spec),
                other => anyhow::bail!("unsupported format '{}' — use 'json' or 'html'", other),
            };

            if let Some(out_path) = output {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&out_path, &result)?;
                eprintln!(
                    "Generated {} spec from {} -> {}",
                    format,
                    file.display(),
                    out_path.display()
                );
            } else {
                println!("{}", result);
            }

            Ok(())
        }
    }
}

/// Build a [`GrpcServiceSpec`] from a parsed proto file.
fn spec_from_parsed_proto(
    proto_file: &typeway_grpc::ProtoFile,
    raw_proto: &str,
) -> typeway_grpc::GrpcServiceSpec {
    use indexmap::IndexMap;
    use typeway_grpc::spec::*;

    let package = proto_file.package.clone();

    // Take the first service (or synthesize an empty one).
    let service = proto_file.services.first();
    let service_name = service
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Service".to_string());

    let mut methods = IndexMap::new();
    let mut messages = IndexMap::new();

    // Collect messages from the parsed proto file.
    for msg in &proto_file.messages {
        let fields: Vec<FieldSpec> = msg
            .fields
            .iter()
            .map(|f| FieldSpec {
                name: f.name.clone(),
                proto_type: f.proto_type.clone(),
                tag: f.tag,
                repeated: f.repeated,
                optional: f.optional,
                is_map: false,
                map_key_type: None,
                map_value_type: None,
                description: None,
            })
            .collect();
        messages.insert(
            msg.name.clone(),
            MessageSpec {
                name: msg.name.clone(),
                fields,
                description: None,
            },
        );
    }

    // Collect methods from the first service.
    if let Some(svc) = service {
        for rpc in &svc.methods {
            let full_path = format!("/{}.{}/{}", package, service_name, rpc.name);
            methods.insert(
                rpc.name.clone(),
                MethodSpec {
                    name: rpc.name.clone(),
                    full_path,
                    rest_path: String::new(),
                    http_method: String::new(),
                    request_type: rpc.input_type.clone(),
                    response_type: rpc.output_type.clone(),
                    server_streaming: false,
                    client_streaming: false,
                    description: None,
                    summary: None,
                    tags: Vec::new(),
                    requires_auth: false,
                },
            );
        }
    }

    GrpcServiceSpec {
        proto: raw_proto.to_string(),
        service: ServiceInfo {
            name: service_name,
            package: package.clone(),
            full_name: if package.is_empty() {
                service
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Service".to_string())
            } else {
                format!(
                    "{}.{}",
                    package,
                    service
                        .map(|s| s.name.as_str())
                        .unwrap_or("Service")
                )
            },
            description: None,
            version: None,
        },
        methods,
        messages,
    }
}
