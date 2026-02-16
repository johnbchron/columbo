{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    devshell.url = "github:numtide/devshell";
  };

  outputs = { nixpkgs, rust-overlay, devshell, flake-utils, crane, ... }: 
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          (import rust-overlay)
          devshell.overlays.default
        ];
      };

      toolchain_fn = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
        extensions = [ "rust-src" "rust-analyzer" ];
      });

      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain_fn;

      js_filter = path: _type: builtins.match ".*js$" path != null;
      js_or_cargo = path: type:
        (js_filter path type) || (craneLib.filterCargoSources path type);
      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = js_or_cargo;
        name = "source";
      };

      columbo_meta = craneLib.crateNameFromCargoToml {
        cargoToml = ./crates/columbo/Cargo.toml;
      };
      columbo_args = {
        inherit src;
        inherit (columbo_meta) pname version;

        strictDeps = true;
        doCheck = false;
      };
      cargoArtifacts = craneLib.buildDepsOnly columbo_args;
      columbo = craneLib.buildPackage (columbo_args // { inherit cargoArtifacts; });
    in {
      devShells.default = pkgs.devshell.mkShell {
        packages = [ (toolchain_fn pkgs) pkgs.gcc ];
        motd = "\n  Welcome to the {2}columbo{reset} shell.\n";
      };

      checks = {
        inherit columbo;
        columbo-clippy = craneLib.cargoClippy (columbo_args // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        });
        my-crate-nextest = craneLib.cargoNextest (columbo_args // {
          inherit cargoArtifacts;
          doCheck = true;
          partitions = 1;
          partitionType = "count";
          cargoNextestPartitionsExtraArgs = "--no-tests=pass";
        });
      };
    });
}
