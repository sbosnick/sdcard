{
  description = "A Generic Rust Library";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, crane, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let

        # Define the minimum supported rust version.
        msrv = "1.56.0";

        # Get nixpkgs with the rust-overlay applied.
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlay ];
        };

        # Create a version of crane for the given Rust toolchain.
        mkCraneLib = toolchain: (crane.mkLib pkgs).overrideScope' (final: prev: {
          cargo = toolchain;
          clippy = toolchain;
          rustc = toolchain;
          rustfmt = toolchain;
         });

        # Create a set of derivation for the crane build sequence we support.
        mkCraneBuild = craneLib: let
          src = ./.;
          dep = craneLib.buildDepsOnly {
            inherit src;
          };
          build = craneLib.cargoBuild {
            inherit src;
            cargoArtifacts = dep;
          };
          clippy = craneLib.cargoClippy {
            inherit src;
            cargoArtifacts = build;
            cargoClippyExtraArgs = "-- --deny warnings";
          };
          fmt = craneLib.cargoFmt {
            inherit src;
          };
        in { inherit dep build clippy fmt; };

        # The versions of crane for the different Rust toolchains we are using.
        craneLibStable = mkCraneLib pkgs.rust-bin.stable.latest.default;
        craneLibBeta = mkCraneLib pkgs.rust-bin.beta.latest.default;
        craneLibNightly = mkCraneLib pkgs.rust-bin.nightly.latest.default;
        craneLibMsrv = mkCraneLib pkgs.rust-bin.stable.${msrv}.default;

        # The derivation sets for the different versions of crane.
        my-crate-stable = mkCraneBuild craneLibStable;
        my-crate-beta = mkCraneBuild craneLibBeta;
        my-crate-nightly = mkCraneBuild craneLibNightly;
        my-crate-msrv = mkCraneBuild craneLibMsrv;
      in
      {
        checks = {
          clippy = my-crate-stable.clippy;
          fmt = my-crate-stable.fmt;
        };

        packages = {
          default = my-crate-stable.build;
          ci-stable = my-crate-stable.build;
          ci-beta = my-crate-beta.build;
          ci-nightly = my-crate-nightly.build;
          ci-msrv = my-crate-msrv.build;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = builtins.attrValues self.checks;

          nativeBuildInputs = [
            pkgs.rust-bin.stable.latest.default
            pkgs.cargo-edit
          ];
        };
      });
}
