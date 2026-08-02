ENDPOINT ?= base-substreams-tier1-prod.kan-sst2.pinax.io:443
START_BLOCK ?= 26990279
STOP_BLOCK ?= 26990521
SPKG_DIR ?= spkg

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

.PHONY: verify-state-parity
verify-state-parity:
	ENDPOINT=$(ENDPOINT) ./scripts/verify-state-parity.sh

.PHONY: build-parquet-fixture
build-parquet-fixture:
	@test -n "$(OUTPUT)" || (echo "set OUTPUT to a new dump directory" >&2; exit 1)
	ENDPOINT=$(ENDPOINT) ./scripts/build-parquet-fixture.sh "$(OUTPUT)"

.PHONY: validate
validate: check-generated test lint pack
