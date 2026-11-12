{
  description = "ferrata_monitor_module: the watchers' monitor for Logos Core (prototype)";

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
    };
}
