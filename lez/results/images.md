# Guest images measured

Built by `methods/` with RISC Zero Rust 1.91.1 (risc0-build 3.0.5).

| guest | image id | ELF bytes |
|---|---|---:|
| program (`guests/program`) | `9f3eb00f54f911e41710fc57d6b0258846bd4db61b57674b8a33a6457bda3e7b` | 602912 |
| consumer (`guests/consumer`) | `f883e6a4dbd8e49439465f918884abe1379b02276ff88676f0a4bf958ee49068` | 419472 |
| LEZ's `scripted` (`guests/scripted`) | `4b597adb3f1a43db85829c411a987234cc84584d8bcd947f0774b852adf0da2b` | 268468 |
| comparison only: the program with software Keccak (`guests/program-keccak-soft`) | `0b08e4cf5e4c57766d33e89a19598f37736a5bc09a879a9d51262a1dd69c6d3e` | 606128 |
| comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) | `303225fe7bc58e6a46beb909a98e71a441f11a462d4e84a5729a2324e8ffd60e` | 416452 |
