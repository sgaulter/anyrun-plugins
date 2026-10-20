{ lib, rustPlatform, stdenv }:

rustPlatform.buildRustPackage {
  pname = "hyprland-focus";
  version = "0.1.0";
  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = if builtins.pathExists ../Cargo.lock then ../Cargo.lock else
      throw "Generate Cargo.lock with cargo generate-lockfile in anyrun-plugins, and add it to Git before building.";
    allowBuiltinFetchGit = true;
  };

  cargoBuildFlags = [ "-p" "hyprland-focus" ];
  cargoTestFlags = [ "-p" "hyprland-focus" ];
  doCheck = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 \
      target/${stdenv.hostPlatform.rust.rustcTarget}/release/libhyprland_focus.so \
      "$out/lib/libhyprland_focus.so"
    runHook postInstall
  '';

  meta = {
    description = "Live Hyprland window search for anyrun";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.linux;
  };
}
