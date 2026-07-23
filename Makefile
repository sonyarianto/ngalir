.PHONY: docs check-docs registry

docs:
	@echo "Cargo build required first. Run: cargo build"
	@scripts/generate-node-docs.sh target/debug

registry:
	@echo "Cargo build required first. Run: cargo build"
	@scripts/generate-registry.sh target/debug docs/registry.json

check-docs:
	@scripts/check-nodes-docs.sh
