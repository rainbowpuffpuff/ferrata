# feat(idl): describe tuples as positional-field structs in generated IDLs

Target branch: `refactor/lez-v0.4.x` (the LEZ v0.4 line; on top of its head, `c90ca0b`).

## Description

`generate_idl` (the source-reading IDL generator of the LEZ v0.4 line) refuses any tuple type:

```
$ spel generate-idl <crate> --message BridgeMessage
error: cannot describe `(Root , u128)` (in `BridgeMessage::Spend`): only paths and arrays are supported
```

`idl_type` handles `Type::Path` and `Type::Array` and falls through to that error for everything
else, so a program whose messages carry a tuple — `Vec<(Root, u128)>` in a `Spend` message is a
natural shape for fee splits — cannot have an IDL generated at all. Programs work around it by
naming a struct with the same fields, which is exactly the encoding a tuple already has.

This PR describes a non-empty tuple as that struct. `(Root, u128)` becomes a definition

```json
{ "name": "Tuple_Root_u128", "kind": "struct",
  "fields": [ { "name": "0", "type": { "defined": "Root" } }, { "name": "1", "type": "u128" } ] }
```

and every use of the tuple becomes `{ "defined": "Tuple_Root_u128" }`. The definition is named
after its element types (non-alphanumeric characters stripped, joined with `_`), so two uses of
the same tuple share one definition. A derived name never stands for two types: if the crate defines a type of
that name, or another tuple whose element types differ only in the stripped characters (`(Vec<u8>, u8)` and
`(Vecu8, u8)`) took it first, generation fails with `Unsupported` instead of sharing a wrong definition.

**The encoding argument.** Borsh serializes a tuple as its elements in order, with no prefix, tag
or length of its own; a struct with positional fields serializes as its fields in order, likewise
with nothing of its own. So the IDL needs no new type form: `Tuple_Root_u128` is an ordinary
struct whose fields happen to be named `"0"` and `"1"` — the names the framework already gives
positional fields (enum tuple variants, `Pair(u64, Inner)` in the tests), that `IdlField`'s docs
call "positional" and `decode` renders as the one value or an array of several. Every decoder
(`decode_named`, `decode_state`, and `spel inspect` through them) reads it unchanged, and so does
anything else that consumes the IDL grammar (primitives, `vec`, `option`, `defined`, `array`).

The empty tuple `()` and references stay refused (`()` has no elements to name and no Borsh
representation anyone should ship; a reference is still a new form), so the generator's set of
supported shapes grows only by tuples with at least one element.

## Changes

- `spel-framework-core/src/idl_gen.rs`: `idl_type` describes `Type::Tuple` with non-empty
  elements as a struct definition with positional fields, named after the element types, and
  returns `IdlType::Defined` for it. Nested tuples describe their inner tuple first, so
  `((u8, u16), bool)` is `Tuple_u8u16_bool` with field `"0"` of type `Tuple_u8_u16`. A derived
  name that a crate type or a different tuple already holds is refused. The error for what is
  still refused now reads "only paths, arrays and non-empty tuples are supported".
- `spel-framework-core/tests/idl_gen.rs`: `an_unsupported_type_fails` now uses the empty tuple,
  which is still refused (a reference is covered by a new test of its own), and new tests cover
  a message with a tuple field and a `Vec` of tuples, a tuple nested in a tuple, two uses of the
  same tuple sharing one definition, a value Borsh-encoded as the tuple decoding through
  `decode_named` — alone and inside a whole message — to the positional fields, and the two
  collisions refused.
- `CHANGELOG.md`: entry under Unreleased.


## Test plan

```
cargo test --locked -p spel-framework-core --features idl-gen,decode --test idl_gen
```

```
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

14 of those are the suite as it is on the target branch; the 7 new ones are
`tuples_are_described_as_structs_with_positional_fields`, `a_tuple_nested_in_a_tuple_is_described_too`,
`the_same_tuple_is_defined_once`, `a_tuple_whose_name_a_crate_type_holds_fails`,
`two_tuples_whose_names_coincide_fail`, `tuple_values_decode_through_the_idl` and
`a_reference_type_fails`. Mutants of the change were each caught: numbering the fields from `1`
fails the two description tests; giving each occurrence of a tuple its own definition (no sharing)
fails all four tuple tests, `the_same_tuple_is_defined_once` first; dropping either collision check
fails its own test.

`cargo clippy --locked -p spel-framework-core --all-targets --features idl-gen,decode` and
`cargo fmt --all --check` report for this change exactly what they report for the branch without
it (the workspace's warning and formatting baselines are unchanged; per `quality.yml` clippy is
collected, not yet denied, on that line).

## How Ferrata uses it

Ferrata's Ethereum-LEZ bridge has `BridgeMessage::Spend { fees: Vec<(Root, u128)> }`, which is
what the error above is from. With this PR the generator describes the bridge program's 28
messages and 32 types with **0 differences** against Ferrata's hand-written IDL (whose name for
that tuple is `FeeShare`), and SPEL's own decoder decodes all 50 of its committed vectors on the
generated IDL: see `idl/README.md`, section "SPEL v0.4 generates them", in the Ferrata
repository. This PR was drafted for that use and is not otherwise used by Ferrata.
