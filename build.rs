fn main() {
    tonic_build::compile_protos("proto/routeguide_lib.proto")
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
