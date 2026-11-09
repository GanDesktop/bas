{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "acp-test-agent";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;

  src = import ./source.nix { inherit lib; };
  cargoLock = import ./cargo-lock.nix;
  cargoBuildFlags = [
    "-p"
    "acp-test-agent"
  ];
  cargoTestFlags = [
    "-p"
    "acp-test-agent"
    "--all-targets"
  ];

  meta = {
    description = "Scripted ACP v1/v2 agent for testing clients without a model";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "acp-test-agent";
  };
}
