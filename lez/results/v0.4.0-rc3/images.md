# Guest images measured

Built by `methods/` with RISC Zero Rust 1.91.1 (risc0-build 3.0.5).

| guest | image id | ELF bytes |
|---|---|---:|
| program (`guests/program`) | `efd9aef79f65f57f9fc38c9bba69dc87058de2229de6d1be1e3de76935d23254` | 627064 |
| consumer (`guests/consumer`) | `9dc21f01d175ad6a384fb2e69732fa304aa2185bea6b826a5535fc3cfac71df3` | 471408 |
| LEZ's `scripted` (`guests/scripted`) | `3af31a8d2cfef9ecc9291221af63fe9fb7a11e161b7f255e56c038d60d2add48` | 268228 |
| comparison only: the program with software Keccak (`guests/program-keccak-soft`) | `44f54150925cc1d4f3695ef0b185e1f90aefcfe0809a11e86a58ff67ef189f9d` | 630292 |
| comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) | `36b90b3ecd1918f4c55c06c7b9d3fe413df9792c5b358013c5c060582e4d957f` | 468408 |
