//! Shared code of the program fuzz targets.

/// The bridge program's guest source (`lez/guests/bridge/src/main.rs`), compiled on the host.
#[allow(dead_code, unused_imports, clippy::all)]
pub mod bridge_guest {
    include!(concat!(env!("OUT_DIR"), "/bridge_guest.rs"));

    /// The guest's `handle`: one delivery, `Ok(response)` or a refusal code.
    pub fn run(input: &ReceiveInput) -> Result<Response, u8> {
        handle(input)
    }
}
