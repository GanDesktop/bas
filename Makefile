.DEFAULT_GOAL := help

NIX_PROFILE_OPTIONS ?=

.PHONY: help install install-bas install-bas-service install-acp-test-agent

# Installers update the same Nix profile, so keep them sequential even with -j.
.NOTPARALLEL:

help:
	@printf '%s\n' \
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
