# PACT — Programmable Agent Contract Toolkit
# Copyright (c) 2026 Gabriel Lars Sabadin
# Licensed under the MIT License.

.PHONY: build run test check clean fmt lint install help wasm wasm-test

# Default target
help: ## Show this help message
	@echo "PACT v0.2 — Build & Development Targets"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

build: ## Build the workspace in debug mode
	cargo build

release: ## Build the workspace in release mode
	cargo build --release

test: ## Run all unit and doc tests
	cargo test

check-examples: build ## Type-check all example .pact files
	@for f in examples/*.pact; do \
		echo "Checking $$f ..."; \
		cargo run -- check $$f || exit 1; \
	done
	@echo "All examples passed."

build-hello: build ## Compile hello_agent to deployment artifacts
	cargo run -- build examples/hello_agent.pact --out-dir pact-out/hello

build-research: build ## Compile research_flow to deployment artifacts
	cargo run -- build examples/research_flow.pact --out-dir pact-out/research

build-examples: build-hello build-research ## Compile all examples

run-hello: build ## Run the hello_agent example
	cargo run -- run examples/hello_agent.pact --flow hello --args "world"

run-research: build ## Run the research_flow example
	cargo run -- run examples/research_flow.pact --flow research_and_report --args "AI safety"

test-examples: build ## Run tests in all example .pact files
	@for f in examples/*.pact; do \
		echo "Testing $$f ..."; \
		cargo run -- test $$f || exit 1; \
	done
	@echo "All example tests passed."

fmt: ## Format all Rust source files
	cargo fmt

lint: ## Run clippy lints
	cargo clippy -- -D warnings

clean: ## Remove build artifacts and generated output
	cargo clean
	rm -rf pact-out

install: ## Install the pact binary to ~/.cargo/bin
	cargo install --path crates/pact-cli

wasm: ## Build the WASM package
	wasm-pack build crates/pact-wasm --target web --out-dir ../../pkg

wasm-test: ## Run WASM native tests
	cargo test -p pact-wasm

all: fmt lint build test check-examples build-examples test-examples wasm ## Run everything
