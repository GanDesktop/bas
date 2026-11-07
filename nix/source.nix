{ lib }:
lib.fileset.toSource {
  root = ../.;
  fileset = lib.fileset.unions [
    ../Cargo.toml
    ../Cargo.lock
    ../bin
    ../crates
    ../scripts/build.rs
    ../scripts/build_metadata.rs
  ];
}
