# Default command: List all available just commands
default:
    @just --list

update:
    nix flake update
