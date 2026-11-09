.DEFAULT_GOAL := help

NIX_PROFILE_OPTIONS ?=

.PHONY: help install install-bas install-bas-service install-acp-test-agent dev-mobile test-mobile package-mobile package-mobile-local

# Installers update the same Nix profile, so keep them sequential even with -j.
.NOTPARALLEL:

help:
	@printf '%s\n' \
		'make dev-mobile           Preview the Rust mobile UI locally; rebuild on save' \
		'make test-mobile          Run isolated Android headless conversation tests' \
		'make package-mobile-local Build the APK with an on-device BAS service' \
		'make package-mobile       Build the Android APK with system dynamic colors' \
		'make install              Install bas and bast into your Nix profile' \
		'make install-bas          Install bas and bast' \
		'make install-bas-service  Install bas and enable/restart its systemd user service' \
		'make install-acp-test-agent  Build, test and install the ACP test agent' \
		'' \
		'Pass Nix options with NIX_PROFILE_OPTIONS, e.g. NIX_PROFILE_OPTIONS="--profile /path/to/profile -L"'

install: install-bas

install-bas:
	./scripts/install-bas $(NIX_PROFILE_OPTIONS)

install-bas-service:
	./scripts/install-bas --service $(NIX_PROFILE_OPTIONS)

install-acp-test-agent:
	./scripts/install-acp-test-agent $(NIX_PROFILE_OPTIONS)

dev-mobile:
	nix develop .#ui --command nsh run bin/bas-mobile --target linux

package-mobile:
	nix develop .#android --command ./scripts/mobile-android package --abi arm64-v8a --out target/phone-test/apk

test-mobile:
	nix develop --command cargo build -p bas -p acp-test-agent -p bas-browser-bridge
	nix develop --command cargo test -p bas-mobile

package-mobile-local:
	./scripts/package-mobile-local
