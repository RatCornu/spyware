{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        flake-utils.follows = "flake-utils";
        nixpkgs.follows = "nixpkgs";
      };
    };
  };

  outputs = { self, flake-utils, rust-overlay, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit overlays system; };
        rust = pkgs.rust-bin.nightly.latest;
      in with pkgs; rec {
        packages = {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "spyware";
            version = "0.1.0";

            nativeBuildInputs = [
              rust.minimal
              gcc
              cmake
              gnumake
              libopus
            ];

            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            meta = with pkgs.lib; {
              description = "Bot discord pour la Foire aux Monstres";
              homepage = "https://github.com/BathazarPatiachvili/spyware";
              license = licenses.gpl3;
            };
          };
        };

      });
}

