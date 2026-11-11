{
  description = "ferrata_bridge_ui: the Ferrata bridge's Logos mini-app (ui_qml module, prototype)";

  # Pinned to the builder commit modules/README.md describes (the same lock as ferrata_attest_module).
  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosQmlModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
