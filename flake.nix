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
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      with pkgs; {
        packages = {
          spyware = pkgs.rustPlatform.buildRustPackage {
            pname = "spyware";
            version = "0.1.0";

            nativeBuildInputs = [
              rust
              gcc
              cmake
              gnumake
              libopus
              pkg-config
              makeWrapper
            ];

            buildInputs = [
              ffmpeg
              openssl
              yt-dlp
            ];

            postFixup = ''
              wrapProgram $out/bin/spyware --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.yt-dlp pkgs.ffmpeg pkgs.openssl ]}
            '';

            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            meta = with pkgs.lib; {
              description = "Bot discord pour la Foire aux Monstres";
              homepage = "https://github.com/RatCornu/spyware";
              license = licenses.gpl3;
            };
          };
        };
        defaultPackage = self.packages.${system}.spyware;
      });
}
