.PHONY: docs check-docs

docs:
	@echo "Cargo build required first. Run: cargo build"
	@scripts/generate-node-docs.sh target/debug

check-docs:
	@scripts/check-nodes-docs.sh
