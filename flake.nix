{
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
      lib = pkgs.lib;
    in
    {
      # To use uidev, run `nix run .#uidev`
      # The devShell doesn't really work due to vulkan library import issues
      devShells = {
        default = pkgs.mkShell {
          packages = [
            self'.packages.default.nativeBuildInputs
            self'.packages.default.buildInputs
          ];
          shellHook = ''
            export SHADERC_LIB_DIR=${lib.getLib pkgs.shaderc}/lib
          '';
        };
      };
      packages = {
        default = pkgs.callPackage ./package.nix { inherit lib pkgs; };
        uidev = self'.packages.default.overrideAttrs {
          meta.mainProgram = "uidev";
        };
      };
    };
  };
}

