fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema_root = std::path::Path::new("../../spec/proto");
    let schemas = [
        schema_root.join("garive/v1/common.proto"),
        schema_root.join("garive/host/v1/host.proto"),
        schema_root.join("garive/process/v1/process.proto"),
    ];
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    prost_build::compile_protos(&schemas, &[schema_root])?;
    for schema in schemas {
        println!("cargo:rerun-if-changed={}", schema.display());
    }
    Ok(())
}
