fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use the vendored protoc binary so no system install is required.
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    tonic_build::compile_protos("proto/pnl.proto")?;
    Ok(())
}
