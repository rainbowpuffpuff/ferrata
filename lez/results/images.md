# Guest images measured

Built by `methods/` with RISC Zero Rust 1.91.1 (risc0-build 3.0.5).

| guest | image id | ELF bytes |
|---|---|---:|
| program (`guests/program`) | `2f3566d4f460620867a93b557776400b1039142ce92203072170c162ba2ec130` | 592752 |
| consumer (`guests/consumer`) | `711584ffe09f0b115132e33ce1485666710bc73e696e66fcc54d00aad0db949f` | 411108 |
| LEZ's `scripted` (`guests/scripted`) | `4b597adb3f1a43db85829c411a987234cc84584d8bcd947f0774b852adf0da2b` | 268468 |
| comparison only: the program with software Keccak (`guests/program-keccak-soft`) | `04465c59781d63382c330465dcf9b242f855546c374555eacb994cc23c018716` | 596032 |
| comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) | `040c0b25b30af7abeae5d2c6cfd1df746149db9eb2324832cae8d2b264530b80` | 408024 |
