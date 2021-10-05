fn main() {
    println!("cargo:rerun-if-changed=protos/");
    protobuf_codegen_pure::Codegen::new()
        .out_dir("src/protos")
        .inputs(&["protos/FeatureCollection.proto"])
        .includes(&["protos"])
        .run()
        .expect("protoc");
}
