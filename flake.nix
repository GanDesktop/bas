{
  description = "Bas — Basic Agent Service";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      eachSystem = nixpkgs.lib.genAttrs systems;
      eachLinux = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
      ];
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
      toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      packageFor =
        system:
        let
          pkgs = pkgsFor system;
          toolchain = toolchainFor pkgs;
        in
        pkgs.callPackage ./nix/package.nix {
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
        };
      testAgentFor =
        system:
        let
          pkgs = pkgsFor system;
          toolchain = toolchainFor pkgs;
        in
        pkgs.callPackage ./nix/acp-test-agent.nix {
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
        };
    in
    {
      packages = eachLinux (system: {
        default = packageFor system;
        bas = packageFor system;
        acp-test-agent = testAgentFor system;
      });
      checks = eachLinux (system: {
        package = packageFor system;
        acp-test-agent = testAgentFor system;
      });
      apps = eachLinux (system: {
        bas = {
          type = "app";
          program = "${packageFor system}/bin/bas";
        };
        bast = {
          type = "app";
          program = "${packageFor system}/bin/bast";
        };
        acp-test-agent = {
          type = "app";
          program = "${testAgentFor system}/bin/acp-test-agent";
        };
      });
      formatter = eachSystem (system: (pkgsFor system).nixfmt);
      devShells = eachSystem (
        system:
        let
          pkgs = pkgsFor system;

          toolchain = toolchainFor pkgs;
        in
        {
          mobile-runtime = pkgs.mkShell {
            packages = [
              (toolchain.override {
                targets = [
                  "aarch64-unknown-linux-gnu"
                  "x86_64-unknown-linux-gnu"
                ];
              })
              pkgs.cargo-zigbuild
              pkgs.zig
              pkgs.zstd
              pkgs.python3
              pkgs.curl
              pkgs.binutils
            ];
          };
          android-emulator =
            let
              androidPkgs = import nixpkgs {
                inherit system;
                config = {
                  allowUnfree = true;
                  android_sdk.accept_license = true;
                };
              };
              sdk =
                (androidPkgs.androidenv.composeAndroidPackages {
                  platformVersions = [ "35" ];
                  includeEmulator = true;
                  includeSystemImages = true;
                  systemImageTypes = [ "google_apis" ];
                  abiVersions = [ "x86_64" ];
                }).androidsdk;
            in
            pkgs.mkShell {
              packages = [
                sdk
                pkgs.jdk17
              ];
              ANDROID_HOME = "${sdk}/libexec/android-sdk";
              JAVA_HOME = "${pkgs.jdk17}";
            };
          ui = pkgs.mkShell {
            packages = [
              toolchain
              pkgs.pkg-config
              pkgs.clang
              pkgs.gtk4
              pkgs.gst_all_1.gstreamer
              pkgs.gst_all_1.gst-plugins-base
            ];
            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
          };
          android =
            let
              androidPkgs = import nixpkgs {
                inherit system;
                config = {
                  allowUnfree = true;
                  android_sdk.accept_license = true;
                };
              };
              ndkVersion = "29.0.14206865";
              sdk =
                (androidPkgs.androidenv.composeAndroidPackages {
                  platformVersions = [ "35" ];
                  buildToolsVersions = [ "35.0.1" ];
                  platformToolsVersion = "37.0.0";
                  cmdLineToolsVersion = "20.0";
                  includeNDK = true;
                  inherit ndkVersion;
                  includeCmake = false;
                  includeEmulator = false;
                  includeSystemImages = false;
                }).androidsdk;
              ndkHost = if pkgs.stdenv.isDarwin then "darwin-x86_64" else "linux-x86_64";
              ndkBin = "${sdk}/libexec/android-sdk/ndk/${ndkVersion}/toolchains/llvm/prebuilt/${ndkHost}/bin";
              androidRust = toolchain.override {
                targets = [
                  "aarch64-linux-android"
                  "armv7-linux-androideabi"
                  "x86_64-linux-android"
                ];
              };
            in
            pkgs.mkShell {
              packages = [
                androidRust
                sdk
                pkgs.jdk17
                pkgs.jq
                pkgs.pkg-config
                pkgs.python3
              ];
              ANDROID_HOME = "${sdk}/libexec/android-sdk";
              ANDROID_SDK_ROOT = "${sdk}/libexec/android-sdk";
              ANDROID_NDK_HOME = "${sdk}/libexec/android-sdk/ndk/${ndkVersion}";
              JAVA_HOME = "${pkgs.jdk17}";
              CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${ndkBin}/aarch64-linux-android28-clang";
              CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER = "${ndkBin}/armv7a-linux-androideabi28-clang";
              CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = "${ndkBin}/x86_64-linux-android28-clang";
              CC_aarch64_linux_android = "${ndkBin}/aarch64-linux-android28-clang";
              CC_armv7_linux_androideabi = "${ndkBin}/armv7a-linux-androideabi28-clang";
              CC_x86_64_linux_android = "${ndkBin}/x86_64-linux-android28-clang";
              AR_aarch64_linux_android = "${ndkBin}/llvm-ar";
              AR_armv7_linux_androideabi = "${ndkBin}/llvm-ar";
              AR_x86_64_linux_android = "${ndkBin}/llvm-ar";
              RUST_SRC_PATH = "${androidRust}/lib/rustlib/src/rust/library";
              shellHook = ''
                export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/20.0/bin:$ANDROID_HOME/build-tools/35.0.1:$PATH"
                echo "BAS Android: SDK 35, NDK ${ndkVersion}, $(rustc --version)"
              '';
            };
          default = pkgs.mkShell {
            packages = [
              toolchain
              pkgs.direnv
            ];

            env = {
              # rust-analyzer resolves std sources from here when `rust-src` is installed.
              RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
            };

            shellHook = ''
              echo "bas dev shell: $(rustc --version), $(cargo --version)"
            '';
          };
        }
      );
    };
}
