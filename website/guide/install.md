# Install

## Binary (Linux x86_64 / aarch64)

```bash
curl -sSL https://raw.githubusercontent.com/sonyarianto/ngalir/main/scripts/install.sh | bash
```

Override the install directory or pin a version:

```bash
NGALIR_VERSION=v0.1.0 NGALIR_INSTALL_DIR=~/.local/bin bash -c "$(curl -sSL https://raw.githubusercontent.com/sonyarianto/ngalir/main/scripts/install.sh)"
```

## Docker

```bash
docker pull ghcr.io/sonyarianto/ngalir:latest
docker run --rm ghcr.io/sonyarianto/ngalir:latest --help
```

Or use Docker Compose for the full stack (CLI + web UI + webhook + schedule):

```bash
docker compose up -d
```

## Build from source

```bash
cargo build --release
# Binaries are in target/release/ngalir and target/release/na-*
```
