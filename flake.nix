{
  description = "A Nix-flake-based Rust development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    fenix,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem
    (
      system: let
        pkgs = nixpkgs.legacyPackages.${system};
        f = with fenix.packages.${system};
          combine [
            latest.toolchain
            targets.wasm32-unknown-unknown.latest.rust-std
            rust-analyzer
          ];
      in {
        devShells.default = pkgs.mkShell {
          name = "leptos";

          packages = with pkgs; [
            f
            wasm-pack
            trunk
            leptosfmt
            symbolicator
          ];
        };
      }
    );
}
