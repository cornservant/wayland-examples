{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/7dcdd87373d72e7a2173e4db02e20d3d7f495cfd";
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
