fn main() {
    ::capnpc::CompilerCommand::new()
        .file("addressbook.capnp")
        .import_path("../..")
        .run()
        .unwrap();
}