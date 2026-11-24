# The names program's wire crate, v1: the source of the deployed `ferrata_names` v1

`src/lib.rs` is the wire crate of `ferrata_names` v1, the build our LEZ v0.4.0-rc3 test network has run, live and
immutable, since 23 November 2026 (image id `0xee9578ca…f790` at LEZ v0.4.0-rc3; [`names/image-ids.md`](../../names/image-ids.md)).
v2 ([`lez/names-wire`](../names-wire/src/lib.rs), the review's fixes) replaced it in this tree; v1 differs from v2 in
this file only: the guest (`lez/guests/names`), `lez/methods-names` and the crate's manifest are the same. It is kept
here so that the deployed program can be rebuilt from this repository.

It is not a member of `lez/`'s workspace and no CI job builds it. `scripts/reproduce/deployed.sh build` rebuilds it as
the source `names` of `scripts/reproduce/deployed-programs.json`: in its copy of the tree this file replaces
`lez/names-wire/src/lib.rs` (the path v1 was built at, which enters the guest's build: `scripts/guest-rustc.py`), LEZ
v0.4.0-rc3 is selected, and `cargo build --release -p methods-names` builds `lez/guests/names`.
