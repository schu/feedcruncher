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
