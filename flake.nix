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
      lib = pkgs.lib;
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
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "wlx-overlay-s";
          version = "0.6-unstable-11-04-2024";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "libmonado-rs-0.1.0" = "sha256-ja7OW/YSmfzaQoBhu6tec9v8fyNDknLekE2eY7McLPE=";
              "openxr-0.18.0" = "sha256-ktkbhmExstkNJDYM/HYOwAwv3acex7P9SP0KMAOKhQk=";
              "ovr_overlay-0.0.0" = "sha256-5IMEI0IPTacbA/1gibYU7OT6r+Bj+hlQjDZ3Kg0L2gw=";
              "smithay-0.3.0" = "sha256-I6XXB5Kort09440dbXQ0+2F4U3ulq1c9x3od+gQ6Chs=";
              "vulkano-0.34.0" = "sha256-0ZIxU2oItT35IFnS0YTVNmM775x21gXOvaahg/B9sj8=";
              "wlx-capture-0.3.12" = "sha256-32WnAnNUSfsAA8WB9da3Wqb4acVlXh6HWsY9tPzCHEE=";
            };
          };

          nativeBuildInputs = with pkgs; [
            makeWrapper
            pkg-config
            rustPlatform.bindgenHook
          ];

          buildInputs = with pkgs; [
            cmake
            python3
            libGL
            wayland
            alsa-lib
            dbus
            fontconfig
            libxkbcommon
            openvr
            openxr-loader
            pipewire
            xorg.libX11
            xorg.libXext
            xorg.libXrandr
          ];

          env.SHADERC_LIB_DIR = "${lib.getLib pkgs.shaderc}/lib";

          postPatch = ''
            substituteAllInPlace src/res/watch.yaml \
              --replace '"pactl"' '"${lib.getExe' pkgs.pulseaudio "pactl"}"'

            # TODO: src/res/keyboard.yaml references 'whisper_stt'
          '';

          postInstall = ''
            patchelf $out/bin/wlx-overlay-s \
              --add-needed ${lib.getLib pkgs.wayland}/lib/libwayland-client.so.0 \
              --add-needed ${lib.getLib pkgs.libxkbcommon}/lib/libxkbcommon.so.0 \
              --add-needed ${lib.getLib pkgs.libGL}/lib/libEGL.so.1 \
              --add-needed ${lib.getLib pkgs.libGL}/lib/libGL.so.1 \
              --add-needed ${lib.getLib pkgs.vulkan-loader}/lib/libvulkan.so.1 \
              --add-needed ${lib.getLib pkgs.libuuid}/lib/libuuid.so.1
          '';
        };
      };
    };
  };
}

