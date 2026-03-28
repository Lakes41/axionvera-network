fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../proto");
    let network_proto = proto_dir.join("network.proto");
    let gateway_proto = proto_dir.join("gateway.proto");

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(format!(
            "{}/src/grpc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .compile(&[network_proto.clone(), gateway_proto.clone()], &[proto_dir.clone()])?;

    println!("cargo:rerun-if-changed={}", network_proto.display());
    println!("cargo:rerun-if-changed={}", gateway_proto.display());

    Ok(())
}
