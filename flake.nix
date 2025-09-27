{
  description = "Simple flake with just devShell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      overlays = [ (import rust-overlay) ];
      pkgs = import nixpkgs { inherit system overlays; };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          protobuf
          just
          rust-analyzer
          rustfmt

          # Rust with both musl + gnu targets
          (pkgs.rust-bin.stable.latest.default.override {
            targets = [ "x86_64-unknown-linux-musl" "x86_64-unknown-linux-gnu" ];
          })
        ];
      };
    };
}
