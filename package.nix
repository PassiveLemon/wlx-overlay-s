{ lib, pkgs, ... }:
pkgs.rustPlatform.buildRustPackage rec {
  pname = "wlx-overlay-s";
  version = "25.4.0";

  src = pkgs.fetchFromGitHub {
    owner = "galister";
    repo = "wlx-overlay-s";
    rev = "v${version}";
    hash = "sha256-sddB0DhtCRbCaj+yksm3UOdy0NJ5FVZeQx4eNkqLBqI=";
  };

  useFetchCargoVendor = true;
  cargoHash = "sha256-OwRUjjUMkQIIh9LGWioqDb7dgYForPrJnf/lmDKDmwk=";

  postPatch = ''
    substituteAllInPlace src/res/watch.yaml \
      --replace '"pactl"' '"${lib.getExe' pkgs.pulseaudio "pactl"}"'

    # TODO: src/res/keyboard.yaml references 'whisper_stt'
  '';

  buildFeatures = [ "uidev" ];

  nativeBuildInputs = with pkgs; [
    makeWrapper
    pkg-config
    rustPlatform.bindgenHook
  ];

  buildInputs = with pkgs; [
    alsa-lib
    dbus
    fontconfig
    libGL
    libxkbcommon
    openvr
    openxr-loader
    pipewire
    xorg.libX11
    xorg.libXext
    xorg.libXrandr
    wayland

    # Uidev
    cmake
    vulkan-loader
    xorg.libXcursor
    xorg.libXi
  ];

  postFixup = ''
    wrapProgram $out/bin/wlx-overlay-s \
      --suffix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath buildInputs}
  '';

  env.SHADERC_LIB_DIR = "${lib.getLib pkgs.shaderc}/lib";

  meta = with lib; {
    description = "Wayland/X11 desktop overlay for SteamVR and OpenXR, Vulkan edition";
    homepage = "https://github.com/galister/wlx-overlay-s";
    license = licenses.gpl3Only;
    maintainers = with maintainers; [ passivelemon ];
    platforms = platforms.linux;
    mainProgram = "wlx-overlay-s";
  };
}

