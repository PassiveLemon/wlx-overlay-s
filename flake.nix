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
      # To use, just run `nix run .#default -- --uidev=watch`
      devShells = {
        default = pkgs.mkShell {
          packages = [
            self'.packages.default.nativeBuildInputs
            self'.packages.default.buildInputs
          ];
        };
      };
      packages = {
        # Pinned to v25.4.0 because uidev doesn't work in newer versions
        default = pkgs.callPackage ./package.nix { inherit lib pkgs; };

        # Doesn't build with uidev for some reason
        overrideTest = pkgs.wlx-overlay-s.overrideAttrs (finalAttrs: prevAttrs: {
          buildFeatures = [ "uidev" ];

          buildInputs = prevAttrs.buildInputs ++ (with pkgs; [
            cmake
            vulkan-loader
            xorg.libXcursor
            xorg.libXi
          ]);

          postFixup = ''
            wrapProgram $out/bin/wlx-overlay-s \
              --suffix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath finalAttrs.buildInputs}
          '';
        });
      };
    };
  };
}

