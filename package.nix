{ lib, pkgs, ... }:
pkgs.rustPlatform.buildRustPackage rec {
  pname = "wlx-overlay-s";
  version = "67435d5fc9bd4469a5c96ba7743e45761db0e7c3";

  src = ./.;

  cargoHash = "sha256-ISKsYwIC1R4nMzakStKrCEtOxJfne8H6TCQLpNG6owE=";

  nativeBuildInputs = with pkgs; [
    makeWrapper
    pkg-config
    rustPlatform.bindgenHook
  ];

  buildInputs = with pkgs; [
    alsa-lib
    dbus
    fontconfig
    gtk3
    gdk-pixbuf
    glib
    libGL
    libxkbcommon
    openvr
    openxr-loader
    pipewire
    shaderc
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

  env.SHADERC_LIB_DIR = "${lib.getLib pkgs.shaderc}/lib";

  # Even though vulkan-loader is in buildInputs, it has to be added to the list for it to work for some reason
  postFixup = ''
    wrapProgram $out/bin/wlx-overlay-s \
      --suffix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath buildInputs}

    wrapProgram $out/bin/uidev \
      --suffix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (buildInputs ++ [ pkgs.vulkan-loader ])}
  '';

  postInstall = ''
    install -Dm644 $src/wlx-overlay-s/wlx-overlay-s.desktop $out/share/applications/wlx-overlay-s.desktop
    install -Dm644 $src/wlx-overlay-s/wlx-overlay-s.svg $out/share/icons/hicolor/scalable/apps/wlx-overlay-s.svg
  '';

  meta = with lib; {
    description = "Wayland/X11 desktop overlay for SteamVR and OpenXR, Vulkan edition";
    homepage = "https://github.com/galister/wlx-overlay-s";
    license = licenses.gpl3Only;
    maintainers = with maintainers; [ passivelemon ];
    platforms = platforms.linux;
    mainProgram = "wlx-overlay-s";
  };
}

