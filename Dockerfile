# syntax=docker/dockerfile:1
# ============================================================================
# Enterprise Brain gateway — demo image.
#
#   Stage 1 (builder, full Rust toolchain — DISCARDED): compile the release
#   `service` binary and PROVISION the compiled scope artifacts + retrieval
#   index from the fixtures (both are generated, so a fresh clone lacks them).
#
#   Stage 2 (runtime, slim, NON-ROOT): the binary plus exactly the data it
#   serves — no compiler, no source, no toolchain in this layer.
# ============================================================================

# ---------- builder ----------
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY . .
# Build the gateway binary, then provision the two on-disk inputs it reads at
# startup — the M1 scope artifacts and the tantivy index — from the fixtures.
RUN cargo build --release -p service \
 && cargo run --release -p scope-compiler -- compile --fixtures fixtures --out compiler/artifacts \
 && cargo run --release -p retrieval     -- index   --fixtures fixtures --out retrieval/idx

# ---------- runtime ----------
FROM debian:bookworm-slim AS runtime
# curl is the port-liveness half of the healthcheck (doctor is the config half).
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl \
 && rm -rf /var/lib/apt/lists/*
# A non-root service account (S5b-3): the gateway never runs as root.
RUN useradd --system --create-home --home-dir /app --shell /usr/sbin/nologin brain
WORKDIR /app
# The binary + ONLY the data it serves. No compiler, no source here.
COPY --from=builder /build/target/release/service /usr/local/bin/service
COPY --from=builder /build/fixtures               /app/fixtures
COPY --from=builder /build/compiler/artifacts     /app/compiler/artifacts
COPY --from=builder /build/retrieval/idx          /app/retrieval/idx
# The demo world (keys, tokens, config, ledger, alerts) lives on a mounted
# volume the bootstrap one-shot writes and the gateway reads.
RUN mkdir -p /data && chown -R brain:brain /app /data
USER brain
EXPOSE 8787
# ENTRYPOINT is the binary; compose supplies the per-service command. The
# default command serves the gateway on loopback 127.0.0.1:8787 (a security
# invariant — the server refuses a non-loopback bind).
ENTRYPOINT ["service"]
CMD ["--fixtures", "fixtures", "--artifacts", "compiler/artifacts", "--idx", "retrieval/idx", "--config", "/data/dev-out/config.json"]
