ENDPOINT ?= base-substreams-tier1-prod.kan-sst2.pinax.io:443
START_BLOCK ?= 26990279
STOP_BLOCK ?= 26990521
SPKG_DIR ?= spkg
STORE_MANIFEST ?= substreams-stores.yaml
STORE_FED_MANIFEST ?= substreams-store-fed.yaml
STORE_STATE_MANIFEST ?= substreams-store-state.yaml
NUL_AUDIT_MANIFEST ?= substreams-nul-audit.yaml
RELEASE_DIST ?= dist
RELEASE_CONTRACT ?= packages/base-uniswap-v4-v0.5.0.json

.DEFAULT_GOAL := pack

.PHONY: protogen
protogen:
	substreams protogen substreams.yaml

.PHONY: check-generated
check-generated:
	./scripts/check-generated.sh

.PHONY: test
test:
	cargo test --locked --all-targets

.PHONY: lint
lint:
	cargo fmt --all --check
	cargo clippy --locked --all-targets -- -D warnings

.PHONY: build
build:
	cargo build --locked --target wasm32-unknown-unknown --release

.PHONY: pack
pack: build
	mkdir -p $(SPKG_DIR)
	substreams pack substreams.yaml -o $(SPKG_DIR)/{spkgDefaultName}

.PHONY: run
run: build
	substreams run -e $(ENDPOINT) substreams.yaml map_events -s $(START_BLOCK) -t $(STOP_BLOCK)

.PHONY: verify-live
verify-live: build
	ENDPOINT=$(ENDPOINT) ./scripts/verify-substreams-events.mjs

.PHONY: download-event-package
download-event-package:
	./scripts/download-event-package.sh

.PHONY: stores-test
stores-test:
	cargo test --locked --manifest-path stores/Cargo.toml
	cargo test --locked --manifest-path store-probe/Cargo.toml
	cargo test --locked --manifest-path store-state/Cargo.toml
	cargo test --locked --manifest-path store-metadata/Cargo.toml
	cargo test --locked --manifest-path store-tick-shards/Cargo.toml
	cargo test --locked --manifest-path nul-audit/Cargo.toml

.PHONY: stores-lint
stores-lint:
	cargo fmt --manifest-path stores/Cargo.toml --all --check
	cargo clippy --locked --manifest-path stores/Cargo.toml --all-targets -- -D warnings
	cargo fmt --manifest-path store-probe/Cargo.toml --all --check
	cargo clippy --locked --manifest-path store-probe/Cargo.toml --all-targets -- -D warnings
	cargo fmt --manifest-path store-state/Cargo.toml --all --check
	cargo clippy --locked --manifest-path store-state/Cargo.toml --all-targets -- -D warnings
	cargo fmt --manifest-path store-metadata/Cargo.toml --all --check
	cargo clippy --locked --manifest-path store-metadata/Cargo.toml --all-targets -- -D warnings
	cargo fmt --manifest-path store-tick-shards/Cargo.toml --all --check
	cargo clippy --locked --manifest-path store-tick-shards/Cargo.toml --all-targets -- -D warnings
	cargo fmt --manifest-path nul-audit/Cargo.toml --all --check
	cargo clippy --locked --manifest-path nul-audit/Cargo.toml --all-targets -- -D warnings

.PHONY: stores-build
stores-build:
	cargo build --locked --manifest-path stores/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-probe/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-state/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-metadata/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-tick-shards/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path nul-audit/Cargo.toml --release --target wasm32-unknown-unknown

.PHONY: nul-audit-build
nul-audit-build:
	cargo build --locked --manifest-path nul-audit/Cargo.toml --release --target wasm32-unknown-unknown

.PHONY: nul-audit-package
nul-audit-package: nul-audit-build
	mkdir -p downloads
	substreams pack "$(NUL_AUDIT_MANIFEST)" --output-file downloads/uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg
	./scripts/verify-nul-audit-package.sh downloads/uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg

.PHONY: release-nul-audit-package
release-nul-audit-package: nul-audit-package
	@test ! -e "$(RELEASE_DIST)" || (echo "refusing existing RELEASE_DIST $(RELEASE_DIST)" >&2; exit 1)
	mkdir -p "$(RELEASE_DIST)"
	cp downloads/uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg "$(RELEASE_DIST)/"
	./scripts/verify-nul-audit-package.sh "$(RELEASE_DIST)/uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg"

.PHONY: stores-package
stores-package: download-event-package stores-build
	mkdir -p downloads
	substreams pack "$(STORE_MANIFEST)" --output-file downloads/uniswap-v4-base-state-stores-v0.1.0.spkg

.PHONY: stores-validate
stores-validate: stores-test stores-lint stores-package
	substreams info "$(STORE_MANIFEST)" map_store_probe --json >/dev/null
	substreams info "$(STORE_FED_MANIFEST)" map_reducer_inputs --json >/dev/null
	substreams info "$(STORE_STATE_MANIFEST)" map_store_state_inputs --json >/dev/null
	substreams info "$(NUL_AUDIT_MANIFEST)" map_nul_metadata_audit --json >/dev/null
	./scripts/test-store-cache-boundary.sh
	./scripts/test-store-state-cache-boundary.sh
	./scripts/test-nul-audit-cache-boundary.sh
	$(MAKE) nul-audit-package

.PHONY: release-packages
release-packages:
	@test ! -e "$(RELEASE_DIST)" || (echo "refusing existing RELEASE_DIST $(RELEASE_DIST)" >&2; exit 1)
	mkdir -p "$(RELEASE_DIST)"
	cp release/v0.4.0/uniswap-v4-base-state-stores-v0.1.0.spkg "$(RELEASE_DIST)/"
	cp release/v0.4.0/uniswap-v4-base-store-fed-reducer-v0.1.0.spkg "$(RELEASE_DIST)/"
	cp release/v0.5.0/uniswap-v4-base-store-state-reducer-v0.5.0.spkg "$(RELEASE_DIST)/"
	./scripts/verify-release-packages.sh "$(RELEASE_DIST)" "$(RELEASE_CONTRACT)"

.PHONY: release-packages-v0.4.0
release-packages-v0.4.0:
	@test ! -e "$(RELEASE_DIST)" || (echo "refusing existing RELEASE_DIST $(RELEASE_DIST)" >&2; exit 1)
	mkdir -p "$(RELEASE_DIST)"
	cp release/v0.4.0/*.spkg "$(RELEASE_DIST)/"
	./scripts/verify-release-packages.sh "$(RELEASE_DIST)" packages/base-uniswap-v4-v0.4.0.json

.PHONY: validate
validate: check-generated test lint pack stores-validate
