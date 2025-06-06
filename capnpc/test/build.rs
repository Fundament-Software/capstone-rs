fn main() {
    capnpc::CompilerCommand::new()
        .crate_provides("external_crate", [0xe6f94f52f7be8fe2])
        .file("test.capnp")
        .file("in-submodule.capnp")
        .file("in-other-submodule.capnp")
        .file("schema/test-in-dir.capnp")
        .file("schema-with-src-prefix/test-in-src-prefix-dir.capnp")
        .import_path("..")
        .src_prefix("schema-with-src-prefix")
        .raw_code_generator_request_path(
            std::env::var("OUT_DIR").expect("OUT_DIR env var is not set")
                + "/raw_code_gen_request.bin",
        )
        .run()
        .expect("compiling schema");

    capnpc::CompilerCommand::new()
        .file("test-default-parent-module.capnp")
        .file("test-default-parent-module-override.capnp")
        .import_path("..")
        .default_parent_module(vec![
            "test_default_parent_module".into(),
            "test_default_parent_module_inner".into(),
        ])
        .run()
        .expect("compiling schema");

    let mut output_path =
        std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR env var is not set"));

    // `capnp compile` will create this directory
    output_path.push("inner-output-path");
    capnpc::CompilerCommand::new()
        .file("test-output-path.capnp")
        .import_path("..")
        .output_path(output_path)
        .run()
        .expect("compiling schema");

    // Have to do this test last. This is obviously unsafe.
    unsafe { std::env::remove_var("OUT_DIR") };
    let error = capnpc::CompilerCommand::new().run().unwrap_err().extra;
    assert!(error.starts_with("Could not access `OUT_DIR` environment variable"));
}
