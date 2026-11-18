| case | native | program |
|---|---|---|
| control: R -> a1 -> a2 | Ok(block 12111477, hops 2) | applied |
| control: R -> R-8191 (first block of the window) | Ok(block 12104582, hops 1) | applied |
| reused hop: R -> a1 twice | Err(150) | exit Some(150) |
| reused hop later: R -> a1 -> a2 -> a2 | Err(150) | exit Some(150) |
| loop back up: R -> a1 -> a2 -> a1 (a1's header, proofs of a1's state) | Err(150) | exit Some(150) |
| reversed order: a1 -> a2 first, then R -> a1 | Err(110) | exit Some(110) |
| padded header: a1's header with one byte appended | Err(101) | exit Some(101) |
| right state, wrong slot: a1's header with the proof of slot (R-8191) mod 8191 | Err(110) | exit Some(110) |
| right slot proof, another block's account proof (R-8191's state) | Err(110) | exit Some(110) |
| hop past the window from a hop record: R -> R-8191 -> (R-2*8191 is the edge) ok | Ok(block 12096391, hops 2) | applied |
