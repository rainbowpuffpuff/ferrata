# Guest images measured

Built by `methods/` with RISC Zero Rust 1.91.1 (risc0-build 3.0.5).

| guest | image id | ELF bytes |
|---|---|---:|
| program (`guests/program`) | `262cb95415b775b5ba5c3117c4653a8243bf5caee151131c1b46ff2b24b74883` | 592728 |
| consumer (`guests/consumer`) | `bfc974b5d0083c9544ffccd4ca19a881d1d462a7d718a8b184bf5572566e5b75` | 411092 |
| LEZ's `scripted` (`guests/scripted`) | `15591e6c407b329d2ef37f0eaea7c98ad9770dc87b5dabc41faee368c08aa355` | 268468 |
| comparison only: the program with software Keccak (`guests/program-keccak-soft`) | `307e82726ac622a0a7be640573b26203626592a25b6f79d56c84d2f3230f2ac0` | 596008 |
| comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) | `9af2e06a01c9a55c979af4f04cc8be8d64eb7dc1af8997b9375fb907dba89f5d` | 408008 |
