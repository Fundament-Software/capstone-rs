fn main() {
    ::capnpc::CompilerCommand::new()
        .file("example.capnp")
        .file("log_sink_example.capnp")
        .file("test_schema.capnp")
        .run()
        .expect("compiling schemas");
}
