{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };
  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];
      perSystem =
        { pkgs, ... }:
        {
          formatter = pkgs.nixfmt;

          devShells.default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [ pkg-config ];
            buildInputs = with pkgs; [
              libGL
              seatd
              libdisplay-info
              libxkbcommon
              libgbm
              wayland
            ];
            packages = with pkgs; [
              rustc
              cargo
              clippy
              cargo-insta
              rustfmt
              rust-analyzer
            ];
          };
        };
    };
}
