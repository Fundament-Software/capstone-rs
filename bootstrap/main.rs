pub fn main() {
    let root = std::path::Path::new(std::env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    std::env::set_current_dir(root).unwrap();
    let output = capnp_sys::call(
        ["capnp/schema.capnp"].iter(),
        [root.to_str().unwrap()].iter(),
        ["capnp/".to_string()].iter(),
        false,
    )
    .expect("Failed to run capnp command");

    ::capnpc::codegen::CodeGenerationCommand::new()
        .output_directory(root.join("capnp/src"))
        .capnp_root("crate")
        .run(output.as_slice())
        .expect("failed to bootstrap schema");

    let output = capnp_sys::call(
        [
            "capnp-rpc/schema/rpc.capnp",
            "capnp-rpc/schema/rpc-twoparty.capnp",
        ]
        .iter(),
        [root.to_str().unwrap()].iter(),
        ["capnp-rpc/schema/".to_string()].iter(),
        false,
    )
    .expect("Failed to run capnp command");

    ::capnpc::codegen::CodeGenerationCommand::new()
        .output_directory(root.join("capnp-rpc/src"))
        .run(output.as_slice())
        .expect("failed to bootstrap RPC schema");
}
