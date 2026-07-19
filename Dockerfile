FROM rust:latest AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev pkg-config && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release --workspace && \
    mkdir -p /out/bin && \
    for bin in target/release/na-* target/release/ngalir; do \
        [ -f "$bin" ] && [ -x "$bin" ] && cp "$bin" /out/bin/; \
    done && \
    ls -la /out/bin/

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/bin /usr/local/bin/

ENV NGALIR_NODE_PATH=/usr/local/bin

ENTRYPOINT ["ngalir"]
CMD ["--help"]
