{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      flake-utils,
      rust-overlay,
      nixpkgs,
      crane,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit overlays system; };
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib nixpkgs.legacyPackages.${system}).overrideToolchain rust;

        commonArgs = {
          src = pkgs.lib.cleanSourceWith { src = ./.; };

          nativeBuildInputs = with pkgs; [
            rust
            gcc
            cmake
            gnumake
            libopus
            fontconfig
            pkg-config
            makeWrapper
          ];

          buildInputs = with pkgs; [
            ffmpeg
            fontconfig
            freetype
            openssl
            yt-dlp
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in
      rec {
        packages = rec {
          spyware = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
            }
          );

          default = spyware;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [ git ] ++ packages.spyware.buildInputs ++ packages.spyware.nativeBuildInputs;
        };
      }
    );
}
