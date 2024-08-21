fn main() {
    ::capnpc::CompilerCommand::new()
        .file("eval.capnp")
        .file("catrank.capnp")
        .file("carsales.capnp")
        .import_path("..")
        .run()
        .expect("compiling schemas");
}
