.PHONY: all
all: lint test build

.PHONY: build
build:
	cargo build

.PHONY: lint
lint:
	cargo fmt -- --check

.PHONY: test
test:
	cargo test

.PHONY: run
run:
	cargo run --bin feedcruncher -- --config config-test.toml

.PHONY: run-server
run-server:
	cargo run --bin feedserver
