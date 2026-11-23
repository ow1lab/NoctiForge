{pkgs, ...}: {
  languages.rust = {
    enable = true;
    channel = "nightly";
    components = ["rustc" "cargo" "clippy" "rust-analyzer"];
  };
  packages = with pkgs; [
    cargo-nextest
  ];
  git-hooks.hooks = {
    clippy.enable = true;
  };
}
