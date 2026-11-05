{
  lib,
  pkgs,
  rustPlatform,
  makeWrapper,
  buildTag ? null,
}:
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "bas";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../build.rs
      ../build_metadata.rs
      ../build_metadata
      ../src
      ../crates
      ../tests
    ];
  };
  # Content identity survives source snapshots without .git and includes local edits.
  BAS_BUILD_TAG =
    if buildTag != null then
      buildTag
    else
      "nix-source-${builtins.substring 0 32 (builtins.baseNameOf finalAttrs.src)}";
  cargoLock.lockFile = ../Cargo.lock;
  cargoTestFlags = [ "--workspace" "--all-targets" ];
  nativeBuildInputs = [ makeWrapper ];
  postInstall = ''
    wrapProgram "$out/bin/bas" \
      --set-default BAS_ACP_PATCHELF ${pkgs.patchelf}/bin/patchelf \
      --set-default BAS_ACP_ELF_LOADER ${pkgs.stdenv.cc.bintools.dynamicLinker} \
      --set-default BAS_ACP_ELF_LIBRARY_PATH ${
        lib.makeLibraryPath [
          pkgs.stdenv.cc.cc.lib
          pkgs.glibc
          pkgs.zlib
          pkgs.openssl
        ]
      }
    install -Dm644 ${../contrib/bas-user.service} "$out/share/systemd/user/bas.service"
    substituteInPlace "$out/share/systemd/user/bas.service" \
      --replace-fail '@bas@' "$out/bin/bas"
    install -Dm644 ${../contrib/daemon.json} "$out/share/bas/daemon.example.json"
  '';

  meta = {
    description = "Basic Agent Service";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "bast";
  };
})
