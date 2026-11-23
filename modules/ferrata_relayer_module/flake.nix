{
  description = "ferrata_relayer_module: the relayer daemon for Logos Core (prototype)";

  # The builder supplies the code generator and the logos-rust-sdk source the crate links. Pinned to
  # the commit modules/README.md describes.
  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
      # The relayer links risc0-zkp's prover code only for the Groth16 step's input.json (risc0-groth16's
      # `prove`); it never proves in-process (r0vm proves, over IPC). On macOS risc0-sys would compile RISC
      # Zero's Metal kernels with Xcode's `xcrun metal` and a cache under $HOME, neither of which a Nix build
      # has (risc0-build-kernel 2.0.1 failed on GitHub's macos-15: read-only file system). This is
      # risc0-build-kernel's own switch: an empty kernel library in their place, read only by the Metal prover
      # the relayer never starts. On Linux risc0-sys compiles no kernels (no `cuda`), so nothing reads it there.
      rustEnv = { RISC0_SKIP_BUILD_KERNELS = "1"; };
    };
}
