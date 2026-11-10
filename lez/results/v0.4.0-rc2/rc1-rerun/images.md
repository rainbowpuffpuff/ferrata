# Guest images measured

Built by `methods/` with RISC Zero Rust 1.91.1 (risc0-build 3.0.5).

| guest | image id | ELF bytes |
|---|---|---:|
| program (`guests/program`) | `59b68c3c8d9f5e7a3424124b891ecde4d1db92529c79b4114306674d589172ab` | 592728 |
| consumer (`guests/consumer`) | `2b03539db9a7ce75bd302a7348161c58d70e72c908cbce6825e099ffb1c4f105` | 411092 |
| LEZ's `scripted` (`guests/scripted`) | `4b597adb3f1a43db85829c411a987234cc84584d8bcd947f0774b852adf0da2b` | 268468 |
| comparison only: the program with software Keccak (`guests/program-keccak-soft`) | `f27e8450407ebda4792103288a143b533e32bd2c345e6bcee917c9742b1a7a1c` | 596008 |
| comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) | `c3901b42ebf64b887862647b41a0aa6cb923881aa6b51b5c2ff89556ab4f3f35` | 408008 |
