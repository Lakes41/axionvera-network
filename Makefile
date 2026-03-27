.PHONY: help build test lint up down

# Default target when just running 'make'
.DEFAULT_GOAL := help

## help: Print this help message
help:
	@echo "Usage:"
	@echo "  make <target>"
	@echo ""
	@echo "Targets:"
	@awk '/^[a-zA-Z\-\_0-9]+:/ { \
		helpMessage = match(lastLine, /^## (.*)/); \
		if (helpMessage) { \
			helpCommand = substr($$1, 0, index($$1, ":")-1); \
			helpDesc = substr(lastLine, RSTART + 3, RLENGTH); \
			printf "  %-15s %s\n", helpCommand, helpDesc; \
		} \
	} \
	{ lastLine = $$0 }' $(MAKEFILE_LIST)

## build: Build the Soroban contracts and the network node (release mode)
build:
	npm run build:contracts
	cd network-node && cargo build --release

## test: Run Rust unit tests, Network Node tests, and TypeScript integration tests
test:
	npm run test:rust
	cd network-node && cargo test
	npm test

## lint: Run formatters, clippy, and TypeScript typechecking
lint:
	cd network-node && cargo fmt -- --check
	cd network-node && cargo clippy -- -D warnings
	npm run typecheck

## up: Start the local infrastructure using Docker Compose in detached mode
up:
	docker-compose up -d

## down: Stop and tear down the local Docker infrastructure
down:
	docker-compose down