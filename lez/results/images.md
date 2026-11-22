# Guest images measured

Built by `methods/` with RISC Zero Rust 1.91.1 (risc0-build 3.0.5).

| guest | image id | ELF bytes |
|---|---|---:|
| program (`guests/program`) | `816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df` | 627064 |
| consumer (`guests/consumer`) | `8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96` | 471408 |
| LEZ's `scripted` (`guests/scripted`) | `8cb0e3f54dd11a2aea7551227c43e6416a6218e101cc7779c98f4f1a53d64e61` | 268228 |
| comparison only: the program with software Keccak (`guests/program-keccak-soft`) | `efe9c58caa0eacd4c837815d7dbb3c003011b478f9cc5b85fca083b6f0a5cb3e` | 630292 |
| comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) | `fa9bef9049151bc92ef32de84e0fdc1153447f115b090b8b6414330ce9d243cc` | 468408 |
