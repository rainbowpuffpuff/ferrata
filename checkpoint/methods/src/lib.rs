//! The checkpoint (rule A), checkpoint_b (rule B) and micro-benchmark guests' ELFs and image ids,
//! and the segment guest's.
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
pub use methods_segment::{SEGMENT_ELF, SEGMENT_ID, SEGMENT_PATH};
