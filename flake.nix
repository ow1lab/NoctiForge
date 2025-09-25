{
  description = "Simple flake with just devShell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          protobuf
          rustup
          rust-analyzer
          rustfmt
          just
          pkgsCross.musl64.buildPackages.stdenv
        ];
      };
    };
}
