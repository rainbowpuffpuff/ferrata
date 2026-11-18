//! The checkpoint guest verifies segment receipts, so it needs the segment guest's image id at
//! compile time. methods-segment (a build dependency) has already built the segment guest; its id
//! is handed to the checkpoint guest's compilation through `FERRATA_SEGMENT_ID`, which risc0-build's
//! inner cargo inherits (it removes only CARGO* variables) and the guest reads with `env!`.
fn main() {
    let id = methods_segment::SEGMENT_ID.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    std::env::set_var("FERRATA_SEGMENT_ID", &id);
    println!("cargo:rustc-env=FERRATA_SEGMENT_ID={id}");
    risc0_build::embed_methods();
}
