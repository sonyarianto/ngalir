# Contributing

## Build

```bash
cargo build
```

## Test

```bash
cargo test --workspace
```

## Add a new node

Use the scaffold generator for new `na-*` node crates:

```bash
cargo run -p na-orchestrator -- init-node
```

This interactively prompts for:
- Node name, description
- Input/output field schemas
- Credential specs (API key, Basic Auth, OAuth2, Custom)

It generates a complete `crates/na-<name>/` crate with Cargo.toml, main.rs,
manifest, test skeleton, and auto-registers it as a workspace member.

## Node naming convention

- Binary: `na-<name>` (e.g., `na-slack`, `na-http`)
- Crate directory: `crates/na-<name>/`
- Cargo package name: `na-<name>`

## Node contract

Every node must implement:

- `--describe` — Print JSON manifest (name, version, description, input/output
  schemas, credential specs) to stdout
- `--version` — Print semver string
- default — Read JSON from stdin, process, write JSON to stdout

See `docs/node-contract.md` for the full protocol specification.

Reference implementation: `crates/na-echo/src/main.rs`

## Code style

- Follow existing patterns in the codebase
- No inline comments in code (doc comments on public items are fine)
- Use `exit_code` constants from `na-contract` for error handling
- Use `na_contract::read_secret(name)` for credential resolution
- Each node crate must include at least `test_manifest_structure` and
  `test_describe_output` tests

## PR workflow

1. Fork or create a branch
2. Implement your change
3. Run `cargo test --workspace` and `cargo clippy --workspace`
4. Submit a pull request
