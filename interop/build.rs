fn main() -> std::io::Result<()> {
    let mut config = prost_build::Config::new();
    config.compile_protos(
        &["proto/test.proto", "proto/messages.proto", "proto/empty.proto"],
        &["proto"],
    )?;
    println!("cargo:rerun-if-changed=proto/test.proto");
    println!("cargo:rerun-if-changed=proto/messages.proto");
    println!("cargo:rerun-if-changed=proto/empty.proto");
    Ok(())
}
