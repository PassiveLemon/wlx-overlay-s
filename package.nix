{ lib
, pkgs
}:
pkgs.rustPlatform.buildRustPackage (finalAttrs: {
  pname = "wayvr";
  version = "2e07818072936058656def2ceca73d21c0b3ebf4";

  src = ./.;

  cargoHash = "sha256-nYI9sGx7F4Jxrt1Rtdi6sia6IqGXsU2Ugx4COk8mGSk=";

  # Taken from nixpkgs wayvr package
  postPatch = ''
    substituteAllInPlace dash-frontend/src/util/pactl_wrapper.rs \
      --replace-fail '"pactl"' '"${lib.getExe' pkgs.pulseaudio "pactl"}"'

    # steam_utils also calls xdg-open as well as steam. Those should probably be pulled from the environment
    substituteInPlace dash-frontend/src/util/steam_utils.rs \
      --replace-fail '"pkill"' '"${lib.getExe' pkgs.procps "pkill"}"'
  '';

  nativeBuildInputs = with pkgs; [
    cmake
    makeWrapper
    pkg-config
    rustPlatform.bindgenHook
    shaderc
  ];

  buildInputs = with pkgs; [
    alsa-lib
    dav1d
    dbus
    libinput
    libx11
    libxcb
    libxcursor
    libxext
    libxi
    libxkbcommon
    libxrandr
    openssl
    openvr
    openxr-loader
    pipewire
    udev
    vulkan-loader
    vulkan-headers
  ];

  buildFeatures = [
    "uidev"
  ];

  env.SHADERC_LIB_DIR = "${lib.getLib pkgs.shaderc}/lib";

  postFixup = ''
    wrapProgram $out/bin/wayvr \
      --suffix LD_LIBRARY_PATH : ${lib.makeLibraryPath finalAttrs.buildInputs}

    wrapProgram $out/bin/uidev \
      --suffix LD_LIBRARY_PATH : ${lib.makeLibraryPath finalAttrs.buildInputs}
  '';

  postInstall = ''
    install -Dm644 $src/wayvr/wayvr.desktop $out/share/applications/wayvr.desktop
    install -Dm644 $src/wayvr/wayvr.svg $out/share/icons/hicolor/scalable/apps/wayvr.svg
  '';

  meta = with lib; {
    description = "lightweight OpenXR/OpenVR overlay for Wayland and X11 desktops";
    homepage = "https://github.com/wlx-team/wayvr";
    license = licenses.gpl3Only;
    maintainers = with maintainers; [ passivelemon ];
    platforms = platforms.linux;
    mainProgram = "wayvr";
  };
})

