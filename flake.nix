{
  description = "WlxOverlay-S";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs = { ... } @ inputs:
  inputs.flake-parts.lib.mkFlake { inherit inputs; } {
    systems = [ "x86_64-linux" ];

    perSystem = { self', system, ... }:
    let
      pkgs = import inputs.nixpkgs { inherit system; };
    in
    {
      # To use, just checkout the last stable release and enter the shell
      devShells = {
        default = pkgs.mkShell {
          packages = [
            self'.packages.default.nativeBuildInputs
            self'.packages.default.buildInputs
          ];
        };
      };
      packages = {
        default = pkgs.wlx-overlay-s.overrideAttrs { 
          version = "0.6";
          src = ./.;
        };
      };
    };
  };
}

