# Agent Workflow Guide

## Pre-commit hooks (enforced by lefthook.yml)

Before every commit, these run in parallel and must pass:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- `scripts/check-nodes-docs.sh` — verifies NODES.md, docs/nodes/, and registry.json are coherent

## Keeping docs & website in sync

Whenever you change node manifests, add/remove nodes, or change the CLI:

1. **Regenerate per-node docs:**
   ```bash
   cargo build
   make docs
   ```

2. **Regenerate registry:**
   ```bash
   cargo build
   make registry
   ```

3. **Update NODES.md** — add/remove rows in the tables, link to docs/nodes/

4. **Update website/ pages** if the change affects public-facing content:
   - `website/guide/cli.md` — CLI command changes
   - `website/guide/install.md` — install method changes
   - `website/nodes/index.md` — node list changes
   - `website/docs/` — architecture, contract, flow-spec changes
   - `website/index.md` — feature list changes

5. **Build website** to verify:
   ```bash
   cd website && npm run build
   ```

## Registry drift check

`scripts/check-nodes-docs.sh` cross-references three sources:

- Workspace members in root `Cargo.toml` (source of truth for active crates)
- `docs/NODES.md` (every crate must appear as a markdown link)
- `docs/nodes/*.md` (every crate must have a per-node doc)
- `docs/registry.json` (must exist; generated from --describe output)

If a crate is removed, its corresponding docs and registry entry must also be removed. If a crate is added, docs must be generated.

## Website independence

`website/` is a standalone VitePress project. It shares no symlinks or imports with `docs/`. Content in `website/` is maintained separately. When updating:

- Prefer linking to GitHub blob for detailed technical content (per-node docs, full specs)
- Keep landing page, guide, and node overview concise and user-facing
