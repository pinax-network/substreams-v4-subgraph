ENDPOINT ?= base-substreams-tier1-prod.kan-sst2.pinax.io:443
START_BLOCK ?= 26990279
STOP_BLOCK ?= 26990521
SPKG_DIR ?= spkg
STORE_MANIFEST ?= substreams-stores.yaml
STORE_FED_MANIFEST ?= substreams-store-fed.yaml
STORE_STATE_MANIFEST ?= substreams-store-state.yaml
RELEASE_DIST ?= dist

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

.PHONY: stores-build
stores-build:
	cargo build --locked --manifest-path stores/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-probe/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-state/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-metadata/Cargo.toml --release --target wasm32-unknown-unknown
	cargo build --locked --manifest-path store-tick-shards/Cargo.toml --release --target wasm32-unknown-unknown

.PHONY: stores-package
stores-package: download-event-package stores-build
	mkdir -p downloads
	substreams pack "$(STORE_MANIFEST)" --output-file downloads/uniswap-v4-base-state-stores-v0.1.0.spkg

.PHONY: stores-validate
stores-validate: stores-test stores-lint stores-package
	substreams info "$(STORE_MANIFEST)" map_store_probe --json >/dev/null
	substreams info "$(STORE_FED_MANIFEST)" map_reducer_inputs --json >/dev/null
	substreams info "$(STORE_STATE_MANIFEST)" map_store_state_inputs --json >/dev/null
	./scripts/test-store-cache-boundary.sh
	./scripts/test-store-state-cache-boundary.sh

.PHONY: release-packages
release-packages:
	@test ! -e "$(RELEASE_DIST)" || (echo "refusing existing RELEASE_DIST $(RELEASE_DIST)" >&2; exit 1)
	mkdir -p "$(RELEASE_DIST)"
	cp release/v0.4.0/*.spkg release/v0.4.0/*.spkg.sha256 "$(RELEASE_DIST)/"
	./scripts/verify-release-packages.sh "$(RELEASE_DIST)"

.PHONY: validate
validate: check-generated test lint pack stores-validate
