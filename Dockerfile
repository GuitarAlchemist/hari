# Project Hari — AGI Research
# Multi-stage Rust build with Docker sandboxing

# Build stage
FROM rust:1.85-slim AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /hari
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/

RUN cargo build --release 2>/dev/null || true
RUN cargo build --release

# Runtime stage — minimal, sandboxed
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for safety
RUN useradd -m -s /bin/bash hari
USER hari
WORKDIR /home/hari

COPY --from=builder /hari/target/release/hari-* ./

# Resource limits enforced via docker run:
# docker run --memory=4g --cpus=2 --read-only --tmpfs /tmp hari
CMD ["./hari-core"]
