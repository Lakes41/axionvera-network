fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/grpc")
        .compile(
            &["../../proto/network.proto", "../../proto/gateway.proto"],
            &["../../proto"],
        )?;

    println!("cargo:rerun-if-changed=proto/network.proto");
    println!("cargo:rerun-if-changed=proto/gateway.proto");
    
    Ok(())
}
