//! The checkpoint guests for zones on Logos Blockchain 0.3.5-rc.1 (`checkpoint_0_3_5`, rule (A); `checkpoint_b_0_3_5`,
//! rule (B)): their ELFs and image ids, and the segment guest's (the default build's, which they verify).
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
pub use methods_segment::{SEGMENT_ELF, SEGMENT_ID, SEGMENT_PATH};
