{ lib, rustPlatform, stdenv }:

rustPlatform.buildRustPackage {
  pname = "multi-web";
  version = "0.1.0";
  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
    allowBuiltinFetchGit = true;
  };

  cargoBuildFlags = [ "-p" "multi-web" ];
  cargoTestFlags = [ "-p" "multi-web" ];
  doCheck = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 \
      target/${stdenv.hostPlatform.rust.rustcTarget}/release/libmulti_web.so \
      "$out/lib/libmulti_web.so"
    runHook postInstall
  '';

  meta = {
    description = "Web searches with independent engine prefixes for anyrun";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.linux;
  };
}
