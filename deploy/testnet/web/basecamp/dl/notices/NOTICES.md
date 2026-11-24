# Third-party notices for the Basecamp release of Ferrata's Logos packages

These notices go with every package of the release (`linux-amd64/*.lgx`, `darwin-arm64/*.lgx`, built from commit
`a0f38827`). Ferrata's own code in them is licensed MIT OR Apache-2.0 (the repository's
[LICENSE-MIT](https://github.com/rainbowpuffpuff/ferrata/blob/a0f388279e2de21305e0a8d1b0777f627ad165b9/LICENSE-MIT) and [LICENSE-APACHE](https://github.com/rainbowpuffpuff/ferrata/blob/a0f388279e2de21305e0a8d1b0777f627ad165b9/LICENSE-APACHE)); nothing here changes that. The packages
also contain code by others, listed below with its licence; the licence texts are in [`licenses/`](licenses/).

## What each package contains

| Package | Platforms | Files in its variant | Code by others in it |
|---|---|---|---|
| `ferrata_bridge_ui` (the mini-app) | linux-amd64, darwin-arm64 | QML, JavaScript, a PNG | none |
| `ferrata_attest_module`, `ferrata_bridge_module`, `ferrata_attestor_module`, `ferrata_monitor_module`, `ferrata_relayer_module` | linux-amd64, darwin-arm64 | the plugin; `libssl` and `libcrypto` (OpenSSL); `libboost_system` (Boost) | the plugin: Rust crates and the Rust standard library ([`crates.md`](crates.md)); OpenSSL; Boost |
| `ferrata_updater_module` | linux-amd64 | the plugin; OpenSSL; Boost; `libpcsclite` (pcsc-lite) | as above, plus pcsc-lite, plus the native libraries of the two archives below, statically linked: **conveyed under the GNU GPL version 3** |

Qt and Logos Core are not in the packages: the plugins use the ones of the Logos Core or Basecamp that loads them.

## The shared libraries in the packages

| Library | Version | Licence | Text | Source |
|---|---|---|---|---|
| OpenSSL (`libssl`, `libcrypto`) | 3.5.1 | `Apache-2.0` | [`licenses/Apache-2.0.txt`](licenses/Apache-2.0.txt) | https://github.com/openssl/openssl/tree/openssl-3.5.1 |
| Boost.System (`libboost_system`) | 1.87.0 | `BSL-1.0` | [`licenses/BSL-1.0-boost-1.87.0.txt`](licenses/BSL-1.0-boost-1.87.0.txt) | https://github.com/boostorg/boost/tree/boost-1.87.0 |
| pcsc-lite (`libpcsclite`; updater only) | 2.3.0 | the licence in its `COPYING` (BSD-3-Clause for the library) | [`licenses/pcsc-lite-2.3.0-COPYING.txt`](licenses/pcsc-lite-2.3.0-COPYING.txt) | https://github.com/LudovicRousseau/PCSC/tree/2.3.0 |

The versions are those of the modules' nixpkgs pin (`e9f00bd893984bc8ce46c895c3bf7cac95331127`, through
logos-module-builder `5f74cdb`) for both `x86_64-linux` and `aarch64-darwin` (`nix eval` on the build box, 23 November
2026), and the packaged `libcrypto` of both platforms names itself "OpenSSL 3.5.1 1 Jul 2025".

## The Rust code in each plugin

[`crates.md`](crates.md) lists, per module and platform, every crate linked into the plugin (`cargo tree -e
normal,no-proc-macro --target T --locked` on the exported module, `modules/scripts/module_notices.py`), with its
version, licence and source, then each licence or notice file of those packages once; crates whose package has no such
file are listed with the authors their `Cargo.toml` names, under the standard texts [`licenses/MIT.txt`](licenses/MIT.txt),
[`licenses/Apache-2.0.txt`](licenses/Apache-2.0.txt) and [`licenses/BSD-3-Clause.txt`](licenses/BSD-3-Clause.txt).
They include the Logos Rust SDK and the LEZ crates the modules vendor. The Rust standard library is linked too: it is
MIT OR Apache-2.0, with the notices of its toolchain's `COPYRIGHT` file (https://github.com/rust-lang/rust/blob/master/COPYRIGHT).

## `ferrata_updater_module`: the native libraries, and the GNU GPL version 3

The updater's plugin links LEZ's node-side crates, which link, statically, the native libraries of two prebuilt
archives: the same two archives the command-line release describes ([`releases/third-party/THIRD-PARTY-NOTICES.md`](https://github.com/rainbowpuffpuff/ferrata/blob/a0f388279e2de21305e0a8d1b0777f627ad165b9/releases/third-party/THIRD-PARTY-NOTICES.md)).
`modules/scripts/stage-updater.sh` places them in the exported module (`rust-lib/vendor/rapidsnark/`, `rust-lib/vendor/lbc/`;
`rust-lib/.cargo/config.toml` names them as `RAPIDSNARK_LIB_DIR` and `LBC_ROOT_DIR`); on the build box, 23 November
2026, the export's files were checked against the sha256 that `scripts/native-deps.json` pins: rapidsnark's 4 libraries
and the circuits' 18 files, all equal.

**`rapidsnark-linux-x86_64-pic-v0.0.8.zip`** (sha256 `59bdd709eed96235de061f352893f4650c923b54b591052118593012bb1cd831`):

| Component | Version | Licence | In the archive | Source |
|---|---|---|---|---|
| rapidsnark | v0.0.8 (iden3/rapidsnark 2921b0eb33369f4d329a6902e97959246b802e6c) | `LGPL-3.0-only` | `lib/librapidsnark.a` | https://github.com/iden3/rapidsnark/tree/v0.0.8 |
| ffiasm field arithmetic (Fr, Fq) | iden3/ffiasm aa90166dc4c5a075b835a398e15cc1e06ac90e95 (rapidsnark v0.0.8's submodule) | `LGPL-3.0-only` | `lib/libfr.a`, `lib/libfq.a`; `lib/librapidsnark.a` (`misc`, `naf`, `splitparstr`, `alt_bn128`) | https://github.com/iden3/ffiasm/tree/aa90166dc4c5a075b835a398e15cc1e06ac90e95 |
| JSON for Modern C++ (nlohmann/json) | 350ff4f7ced7c4117eae2fb93df02823c8021fcb (rapidsnark v0.0.8's submodule; header-only) | `MIT` | `lib/librapidsnark.a` | https://github.com/nlohmann/json/tree/350ff4f7ced7c4117eae2fb93df02823c8021fcb |
| GNU MP (GMP) | 6.3.0 | `LGPL-3.0-or-later OR GPL-2.0-or-later` | `lib/libgmp.a` | `gmp-6.3.0.tar.xz` (below) |

**`logos-blockchain-circuits-v0.5.7-linux-x86_64.tar.gz`** (sha256 `92a0c6ad810d486631518b7b9b247a2b66f66af6f8f8ddce381acbbdfedc0f7f`; each circuit's proving key, verification key and witness data embedded in the plugin):

| Component | Version | Licence | In the archive | Source |
|---|---|---|---|---|
| Logos Blockchain circuits (witness generators pol, poc, poq, signature; their proving and verification keys) | v0.5.7 (ebf7ddf5b5b625ef4ea7c803171c670beebd05ef) | `MIT OR Apache-2.0` | `{pol,poc,poq,signature}/lib*.a` and the key files | https://github.com/logos-blockchain/logos-blockchain-circuits/tree/v0.5.7 |
| circom C++ runtime (calcwit, circom.hpp, fr), compiled into each witness generator by circom | circom 2.2.2 | `GPL-3.0-only` | `{pol,poc,poq,signature}/lib*.a` | https://github.com/iden3/circom/tree/v2.2.2/code_producers/src/c_elements |
| circomlib templates (bitify, comparators), compiled into the witness generators | iden3/circomlib 35e54ea21da3e8762557234298dbb553c175ea8d (the circuits' submodule) | `LGPL-3.0-only` | `{pol,poc,poq,signature}/lib*.a` | https://github.com/iden3/circomlib/tree/35e54ea21da3e8762557234298dbb553c175ea8d |
| JSON for Modern C++ (nlohmann/json), compiled into each witness generator (circom's main.cpp) | 3.10.5 (Ubuntu 22.04's `nlohmann-json3-dev` 3.10.5-2; header-only) | `MIT` | `{pol,poc,poq,signature}/lib*.a` | https://github.com/nlohmann/json/tree/v3.10.5 |
| GNU MP (GMP) | 6.2.1 | `LGPL-3.0-or-later OR GPL-2.0-or-later` | `lib/libgmp.a` | `gmp-6.2.1.tar.xz` (below) |

The plugin thus contains C++ code generated by the circom compiler with circom's C++ runtime (`GPL-3.0-only`) and
libraries under the GNU LGPL version 3 (GMP, rapidsnark, ffiasm, circomlib). As a combined work, **the
`ferrata_updater_module` package's plugin is conveyed under the GNU General Public License, version 3**
([`licenses/GPL-3.0.txt`](licenses/GPL-3.0.txt)), which the permissive licences of the rest of its code permit (the cautious
reading, as for the command-line binary). The source code of the repository stays MIT OR Apache-2.0. The other
packages contain none of this code.

### Copyright notices and licence texts

| Component | Notice | Licence text |
|---|---|---|
| GNU MP (GMP) | Copyright Free Software Foundation, Inc. The GNU MP Library is free software; you can redistribute it and/or modify it under the terms of either the GNU Lesser General Public License as published by the Free Software Foundation, version 3 or later, or the GNU General Public License as published by the Free Software Foundation, version 2 or later. | `licenses/LGPL-3.0.txt`, `licenses/GPL-3.0.txt`, `licenses/GPL-2.0.txt` (as shipped in gmp-6.3.0.tar.xz) |
| rapidsnark, ffiasm, circomlib | Part of the iden3 project, copyright 0KIMS association, published under the GNU LGPL version 3 (their `COPYING` / `LICENSE` files). | `licenses/LGPL-3.0.txt`, `licenses/GPL-3.0.txt` (the LGPL version 3 adds to the GPL version 3) |
| circom C++ runtime | Part of the iden3 project (circom), copyright 0KIMS association, published under the GNU GPL version 3 (`COPYING`). | `licenses/GPL-3.0.txt` |
| JSON for Modern C++, in rapidsnark (`350ff4f7`) | Copyright (c) 2013-2020 Niels Lohmann | `licenses/MIT-nlohmann-json.txt` |
| JSON for Modern C++ 3.10.5, in the circuits | Copyright (c) 2013-2022 Niels Lohmann | `licenses/MIT-nlohmann-json-3.10.5.txt` |
| Logos Blockchain circuits | Copyright © 2025-2026 Logos | `licenses/MIT-logos-blockchain-circuits.txt` (MIT, chosen of MIT OR Apache-2.0) |

The texts named there: [`licenses/GPL-3.0.txt`](licenses/GPL-3.0.txt), [`licenses/LGPL-3.0.txt`](licenses/LGPL-3.0.txt),
[`licenses/GPL-2.0.txt`](licenses/GPL-2.0.txt), [`licenses/MIT-nlohmann-json.txt`](licenses/MIT-nlohmann-json.txt),
[`licenses/MIT-nlohmann-json-3.10.5.txt`](licenses/MIT-nlohmann-json-3.10.5.txt),
[`licenses/MIT-logos-blockchain-circuits.txt`](licenses/MIT-logos-blockchain-circuits.txt).

### Changing and relinking the LGPL libraries (LGPL version 3, section 4)

You may modify GMP, rapidsnark, ffiasm or circomlib and run the updater with your version. Everything needed to
rebuild it is public: Ferrata at `a0f38827`, and `modules/scripts/basecamp-release.sh build` (or `export.sh`, then `nix
build .#lgx-portable` in the exported module). Put your builds of rapidsnark's libraries (`librapidsnark.a`, `libfr.a`,
`libfq.a`, `libgmp.a`) in the export's `rust-lib/vendor/rapidsnark/` and the circuits' directory in
`rust-lib/vendor/lbc/`, and rebuild.

### Corresponding source, and the written offer

- **GMP**: the exact source archives, checked against `scripts/native-deps.json`, are published beside these notices:

  | File | Bytes | sha256 |
  |---|---:|---|
  | [`sources/gmp-6.3.0.tar.xz`](sources/gmp-6.3.0.tar.xz) | 2,094,196 | `a3c2b80201b89e68616f4ad30bc66aee4927c3ce50e33929ca819d5c43538898` |
  | [`sources/gmp-6.2.1.tar.xz`](sources/gmp-6.2.1.tar.xz) | 2,027,316 | `fd4829912cddd12f84181c3451cc752be224643e87fac497b69edddadc49b4f2` |

- **The rest of the plugin**: the source at the URLs and commits in the tables above, Ferrata's at `a0f38827`
  (https://github.com/rainbowpuffpuff/ferrata), and every crate at the version and source of the exported module's
  `Cargo.lock` ([`crates.md`](crates.md)).
- **Written offer**: for at least three years after this release, and for as long as its packages are offered, anyone
  who has the `ferrata_updater_module` package may ask for the complete Corresponding Source of it (GNU GPL version 3,
  section 6), for no more than the cost of physically performing the distribution, through the issue tracker of
  https://github.com/rainbowpuffpuff/ferrata.
